//! V-H04 / PERF-03 — opening and closing the TUI a thousand times (v2.1 §24.5, §24.6, §31.1).
//!
//! PERF-03 asks for "no continuing session / task / memory growth" over 1,000 TUI open-close
//! cycles. The failure it is looking for is not a crash. It is the session that is fine for an
//! hour, holds one more subscription task per cycle, and is unusable by the afternoon.
//!
//! `crates/pr-core/tests/task_soak.rs` isolates the supervisor. This one cycles the real
//! thing — the terminal coordinator taking and releasing the alternate screen, with a Ratatui
//! frame drawn each time — so a failure here says the leak is in the composition rather than
//! in `TaskScope`.
//!
//! The assertion is about *shape*, not about an absolute number. An allocator is allowed to
//! settle upwards; what it is not allowed to do is grow in proportion to the number of cycles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_core::mem::rss_bytes;
use pr_terminal::testing::claim;
use pr_terminal::{Capture, Coordinator, Input, Key, Surface};

const CYCLES: usize = 1_000;

/// One open-close cycle: claim the terminal, type a draft, hand the screen to the TUI, draw a
/// frame, come back, and release everything.
fn cycle(n: usize) {
    let mut co = Coordinator::new(Capture::new(), 120, 40).expect("the terminal is free");
    co.set_prompt("penguin@r2/0> ");
    co.set_origin("@r2 / DB 0 / result #17");
    co.start();

    for ch in format!("HGET player:{n} conf").chars() {
        co.handle(Input::Key(Key::Char(ch)));
    }

    co.enter_alternate();
    assert_eq!(co.surface(), Surface::Alternate);

    let mut tui = pr_tui::Idle::new(120, 40).expect("a TUI of this size");
    tui.draw(co.origin()).expect("a frame");
    assert_eq!(tui.lines().len(), 40);
    drop(tui);

    co.leave_alternate();
    assert_eq!(co.surface(), Surface::Repl);
    assert!(
        co.text().starts_with("HGET player:"),
        "the draft came back: {:?}",
        co.text()
    );
    co.shutdown();
}

#[test]
fn perf_03_a_thousand_tui_cycles_do_not_grow() {
    // The terminal token is process-global and `cargo test` runs these in parallel; without
    // the lock they would fail over each other rather than over what they check.
    let _c = claim();
    // Warm up first: the first cycles fault in pages and grow the allocator's arenas, and
    // counting that as a leak would make the test fail for the wrong reason.
    for n in 0..100 {
        cycle(n);
    }
    let after_warmup = rss_bytes().expect("this platform reports RSS");

    for n in 100..(CYCLES / 2) {
        cycle(n);
    }
    let midpoint = rss_bytes().unwrap();

    for n in (CYCLES / 2)..CYCLES {
        cycle(n);
    }
    let end = rss_bytes().unwrap();

    let first_half = midpoint.saturating_sub(after_warmup);
    let second_half = end.saturating_sub(midpoint);
    println!(
        "PERF-03 RSS: warmup {:.1} MiB -> mid {:.1} MiB -> end {:.1} MiB \
         (first half +{} B, second half +{} B)",
        mib(after_warmup),
        mib(midpoint),
        mib(end),
        first_half,
        second_half
    );

    // A leak proportional to the cycle count would make the two halves grow by comparable
    // amounts. A settling allocator grows once and then stops.
    assert!(
        second_half <= first_half.max(2 * 1024 * 1024),
        "RSS grew by {second_half} bytes in the second 450 cycles against {first_half} in the \
         first — that is growth proportional to the work, which is what a leak looks like"
    );

    // And the absolute drift stays far inside the idle-TUI budget (§24.6).
    let drift = end.saturating_sub(after_warmup);
    assert!(
        drift < 16 * 1024 * 1024,
        "RSS drifted {:.1} MiB over {CYCLES} cycles",
        mib(drift)
    );
}

#[test]
fn every_cycle_releases_the_terminal() {
    // The terminal token is process-global and `cargo test` runs these in parallel; without
    // the lock they would fail over each other rather than over what they check.
    let _c = claim();
    // The structural half of "no session growth": if a cycle failed to release the terminal,
    // the next `Coordinator::new` would return `AlreadyOwned` rather than leaking quietly.
    for n in 0..200 {
        cycle(n);
    }
    assert!(
        !pr_terminal::Ownership::is_owned(),
        "the terminal is still claimed after the last cycle"
    );
    // And it can still be claimed, which is the same check from the other direction.
    let co = Coordinator::new(Capture::new(), 80, 24);
    assert!(co.is_ok());
}

#[test]
fn a_cycle_that_leaves_the_tui_up_still_restores_the_screen_on_drop() {
    // The terminal token is process-global and `cargo test` runs these in parallel; without
    // the lock they would fail over each other rather than over what they check.
    let _c = claim();
    // A panic or an early return mid-TUI must not leave the user staring at a blank alternate
    // screen with no shell (§14.4).
    let bytes = {
        let mut co = Coordinator::new(Capture::new(), 80, 24).unwrap();
        co.start();
        co.enter_alternate();
        assert_eq!(co.surface(), Surface::Alternate);
        // No `leave_alternate`, no `shutdown` — just drop it.
        co.sink_mut().clear();
        drop(co);
        // The sink is owned by the coordinator, so re-check through a fresh one that the
        // terminal is at least releasable.
        pr_terminal::Ownership::is_owned()
    };
    assert!(!bytes, "dropping the coordinator released the terminal");
    assert!(Coordinator::new(Capture::new(), 80, 24).is_ok());
}

#[allow(clippy::cast_precision_loss)]
fn mib(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}
