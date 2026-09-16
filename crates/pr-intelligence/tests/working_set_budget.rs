//! V-F09 — the assistance caches bound themselves (v2.1 §12.5, R08, R35).
//!
//! The behavioural half. `working_set_heap.rs` weighs the result with a counting allocator;
//! this file checks the rules that make the weighing come out right, and needs no allocator,
//! so it can run in parallel with everything else.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_intelligence::working_set::{WorkingSet, limits};
use pr_intelligence::{Observation, ObservationScope, ObservationStore, Origin};

fn scope() -> ObservationScope {
    ObservationScope::new("probe", "service", 0)
}

#[test]
fn the_sub_budgets_add_up_to_the_stated_total() {
    // §12.5 says 「10 MiB + 2 MiB 余量」. If the sub-items summed to something else, the cap
    // and its own breakdown would disagree, and the 12 MiB would be a number with no
    // derivation behind it.
    assert_eq!(limits::SUB_ITEM_TOTAL, 10 * 1024 * 1024);
    assert_eq!(limits::HEAP_INCREMENT, 12 * 1024 * 1024);
}

#[test]
fn the_observed_name_store_enforces_both_halves_of_its_budget() {
    // §12.5: 「5,000 个或 2 MiB，**先到者为准**」. V-F09 found this store enforcing only the
    // entry count, which means 5,000 names of 40 KiB would have held 200 MB inside a cache
    // budgeted at 2.
    //
    // The entry limit binds first for ordinary names.
    let mut s = ObservationStore::new(5_000);
    for i in 0..6_000 {
        s.record(Observation {
            name: format!("penguin:fixture:player:{i:012}:profile").into_bytes(),
            scope: scope(),
            origin: Origin::RetainedResult,
            parent_key: None,
        });
    }
    assert_eq!(s.len(), 5_000, "the entry limit did not bind");
    assert!(s.bytes() <= 2 * 1024 * 1024, "{} B", s.bytes());
    assert_eq!(s.evicted(), 1_000);

    // The byte limit binds first for long ones. 200 names of 40 KiB is 8 MB, well under 5,000
    // entries and well over 2 MiB.
    let mut s = ObservationStore::new(5_000);
    let long = vec![b'k'; 40 * 1024];
    for i in 0..200 {
        let mut name = long.clone();
        name.extend_from_slice(i.to_string().as_bytes());
        s.record(Observation {
            name,
            scope: scope(),
            origin: Origin::RetainedResult,
            parent_key: None,
        });
    }
    assert!(
        s.len() < 200,
        "the byte limit did not bind: {} held",
        s.len()
    );
    assert!(
        s.bytes() <= 2 * 1024 * 1024,
        "the store holds {} B, over its 2 MiB budget",
        s.bytes()
    );
    assert!(s.evicted() > 0);
}

#[test]
fn a_name_larger_than_the_whole_budget_is_refused_rather_than_emptying_the_store() {
    // 「超长名字受单项限制，仍可手输」. Making room for a 3 MiB name inside a 2 MiB cache would
    // mean discarding everything and still not fitting.
    let mut s = ObservationStore::new(5_000);
    for i in 0..100 {
        s.record(Observation {
            name: format!("keep:{i}").into_bytes(),
            scope: scope(),
            origin: Origin::RetainedResult,
            parent_key: None,
        });
    }
    let held = s.len();
    assert_eq!(held, 100);

    s.record(Observation {
        name: vec![b'x'; 3 * 1024 * 1024],
        scope: scope(),
        origin: Origin::RetainedResult,
        parent_key: None,
    });
    assert_eq!(s.len(), held, "an oversized name emptied the store");
    assert_eq!(s.evicted(), 1, "the refusal was not counted");
}

#[test]
fn the_field_cache_bounds_itself_by_bytes_across_all_keys() {
    // §12.5: 「field/member 名缓存 | 2 MiB 总预算 | 分 target/key LRU，**不按每 key 无限叠加**」.
    // A per-key budget would let a hundred keys hold a hundred times the budget.
    let mut ws = WorkingSet::new();
    for k in 0..100u32 {
        let owner = format!("key:{k}").into_bytes();
        for f in 0..1_000u32 {
            ws.fields
                .insert(&owner, format!("field:{f:010}").as_bytes());
        }
    }
    assert!(
        ws.fields.bytes() <= limits::FIELD_NAMES_BYTES,
        "the field cache holds {} B across 100 keys, over its {} B total",
        ws.fields.bytes(),
        limits::FIELD_NAMES_BYTES
    );
    assert!(
        ws.fields.evicted > 0,
        "nothing was evicted, so nothing was bounded"
    );
}
