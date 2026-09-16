//! V-F09 — the assistance heap increment, weighed (v2.1 §12.5, §16.4, §24.7, R08, R35).
//!
//! §12.5 caps 「普通 assistance **堆**增量工作集」 at 12 MiB and says how to measure it:
//! 「按「开/关提示」的 RSS 差测量」, excluding the result store and the read-only catalog.
//!
//! This file weighs the **heap** with a counting global allocator, which is what the budget is
//! about; `crates/prc/tests/budgets.rs` measures the RSS difference between two child
//! processes, which is what §12.5 literally names. Two instruments, because they fail
//! differently — a leak shows in the allocator and may not show in RSS; allocator overhead
//! shows in RSS and not in the allocator — and agreement between them is what makes either
//! believable.
//!
//! **This file is on its own because the counter is process-global.** Rust runs tests in
//! parallel threads inside one binary, so a `live()` reading taken while another test is
//! allocating is meaningless. The first version of this test measured 22.9 MiB for a working
//! set that costs 8.8, because a sibling test was holding 8 MB of fixture at the time. The
//! mutex below serialises what remains.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_intelligence::ObservationScope;
use pr_intelligence::working_set::{WorkingSet, limits};
use std::sync::Mutex;
use xtask::measure::CountingAllocator;

/// The same instrument `xtask measure-baseline` uses.
///
/// Shared rather than reimplemented: two counting allocators that agree today and drift
/// tomorrow is how a budget quietly stops being enforced, and §31.2's objection to two subtly
/// different implementations is not only about decoders.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// Only one measurement at a time; see the module note.
static MEASURING: Mutex<()> = Mutex::new(());

fn scope() -> ObservationScope {
    ObservationScope::new("probe", "service", 0)
}

#[test]
fn a_saturated_working_set_fits_the_twelve_mebibyte_budget() {
    let _guard = MEASURING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Every §12.5 sub-budget filled to its stated limit — the worst case that is still
    // *inside* the budget — and the whole thing weighed.
    let before = ALLOC.live_bytes();
    let mut ws = WorkingSet::new();
    ws.saturate(&scope());
    let used = ALLOC.live_bytes().saturating_sub(before);
    let accounted = ws.accounted_bytes();

    eprintln!(
        "V-F09 heap: saturated working set = {used} B ({} KiB); cap {} B ({} KiB); \
         accounted {accounted} B; keys {} entries / {} B; fields {} B",
        used / 1024,
        limits::HEAP_INCREMENT,
        limits::HEAP_INCREMENT / 1024,
        ws.keys.len(),
        ws.keys.bytes(),
        ws.fields.bytes(),
    );

    assert!(
        used <= limits::HEAP_INCREMENT,
        "the saturated working set is {used} B, over §12.5's {} B cap",
        limits::HEAP_INCREMENT
    );

    // The caches' own accounting must be **conservative**: it may over-estimate, never
    // under-estimate. A budget that under-counts is a budget that is not enforced, which is
    // exactly the defect V-F09 found — the field cache claimed 2 MiB while costing 5.2.
    assert!(
        accounted >= used,
        "the caches account for {accounted} B but really cost {used} B; a budget that \
         under-counts does not bound anything"
    );

    // Dropping it gives the memory back, which is what makes "turn assistance off" mean
    // something rather than being a flag.
    drop(ws);
    let after = ALLOC.live_bytes();
    assert!(
        after < before + 64 * 1024,
        "after dropping the working set, {} B are still held",
        after.saturating_sub(before)
    );
}

#[test]
fn an_empty_working_set_allocates_nothing_at_all() {
    // The other end: having assistance available must not itself cost the budget. The 12 MiB
    // is a cap on what it may grow to, not a reservation made at start-up.
    //
    // Asserted structurally rather than from the allocator. The counter is process-global and
    // the test harness allocates on its own threads, so a delta this small is noise — and a
    // noisy assertion on a global counter is how a budget check becomes a flaky test that
    // someone eventually deletes. Every component being empty is the same claim, deterministic.
    let ws = WorkingSet::new();
    assert_eq!(ws.keys.len(), 0);
    assert_eq!(
        ws.keys.bytes(),
        0,
        "the key store reserved capacity up front"
    );
    assert_eq!(
        ws.fields.bytes(),
        0,
        "the field cache reserved capacity up front"
    );
    assert!(ws.metadata.is_empty() && ws.metadata.capacity() == 0);
    assert!(ws.scratch.is_empty() && ws.scratch.capacity() == 0);
    assert!(ws.analyser.is_empty() && ws.analyser.capacity() == 0);
    assert!(ws.discovery.is_empty() && ws.discovery.capacity() == 0);
    assert_eq!(ws.accounted_bytes(), 0);
}

#[test]
fn each_sub_budget_costs_no_more_than_it_claims() {
    // Per sub-item rather than only in total, because a total that passes can hide one
    // sub-item at 2.5x paid for by another at half — and the one at 2.5x is the one that will
    // grow when a user's keyspace is not the shape this fixture assumed.
    let _guard = MEASURING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let scope = scope();
    let mut ws = WorkingSet::new();

    let b0 = ALLOC.live_bytes();
    ws.saturate_keys(&scope);
    let keys_cost = ALLOC.live_bytes().saturating_sub(b0);
    assert!(
        keys_cost <= limits::OBSERVED_KEYS_BYTES,
        "the observed-name cache costs {keys_cost} B against a {} B budget",
        limits::OBSERVED_KEYS_BYTES
    );

    let b1 = ALLOC.live_bytes();
    ws.saturate_fields();
    let fields_cost = ALLOC.live_bytes().saturating_sub(b1);
    assert!(
        fields_cost <= limits::FIELD_NAMES_BYTES,
        "the field cache costs {fields_cost} B against a {} B budget",
        limits::FIELD_NAMES_BYTES
    );

    eprintln!("V-F09 heap: keys {keys_cost} B, fields {fields_cost} B");
}
