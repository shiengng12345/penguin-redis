//! V-H04 — task supervision leak detection (v2.1 §31.1, §24.5, ADR-003, R21, PERF-03).
//!
//! Tokio has no scoped tasks. `JoinSet` and `TaskTracker` do not abort on drop, so
//! "structured concurrency" is not something this project gets for free — it is something
//! [`TaskScope`] has to provide, and something a test has to prove.
//!
//! The load-bearing claim is narrow and checkable: **after a scope is gone, its task count is
//! zero.** Everything STATE-07, NET-07, PERF-03 and LIFE-01 promise rests on that, and the
//! failure it prevents is the quiet one — a long session that accumulates a subscription task
//! per reconnect and is fine until the eighth hour.
//!
//! These tests deliberately include the awkward cases: a task that ignores its cancellation
//! token, a task that panics, a task blocked forever, and a scope dropped while tasks are
//! mid-flight. A counter that only returns to zero for well-behaved tasks proves nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_core::TaskScope;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Wait until `f` holds, or give up. Tokio needs a moment to finish aborting.
async fn settle(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..2000 {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    f()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropping_a_scope_returns_the_count_to_zero() {
    let handle = {
        let mut scope = TaskScope::new("session");
        let handle = scope.handle();
        for _ in 0..64 {
            scope.spawn(|token| async move {
                token.cancelled().await;
            });
        }
        assert!(
            settle(|| handle.live() == 64).await,
            "expected 64 live, got {}",
            handle.live()
        );
        handle
    };
    assert!(
        settle(|| handle.live() == 0).await,
        "the scope is gone and {} task(s) are still live",
        handle.live()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_that_ignores_its_token_is_still_reclaimed() {
    // The cooperative path is the easy one. A task that never looks at its token is the one
    // that leaks, and `Drop` aborting is the only thing that saves it.
    let handle = {
        let mut scope = TaskScope::new("stubborn");
        let handle = scope.handle();
        for _ in 0..32 {
            scope.spawn(|_token| async move {
                loop {
                    tokio::time::sleep(Duration::from_hours(1)).await;
                }
            });
        }
        assert!(settle(|| handle.live() == 32).await);
        handle
    };
    assert!(
        settle(|| handle.live() == 0).await,
        "{} stubborn task(s) survived their scope",
        handle.live()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_panicking_task_does_not_strand_the_count() {
    // A guard that only decrements on the happy path leaves the counter permanently wrong,
    // and `:tasks` then lies for the rest of the session.
    let mut scope = TaskScope::new("panics");
    let handle = scope.handle();
    for _ in 0..8 {
        scope.spawn(|_t| async move {
            panic!("simulated failure inside a supervised task");
        });
    }
    assert!(
        settle(|| handle.live() == 0).await,
        "{} panicked task(s) left counted",
        handle.live()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_drains_cooperatively_and_reports_that_it_did() {
    let mut scope = TaskScope::new("graceful");
    let handle = scope.handle();
    let finished = Arc::new(AtomicUsize::new(0));
    for _ in 0..16 {
        let f = Arc::clone(&finished);
        scope.spawn(move |token| async move {
            token.cancelled().await;
            f.fetch_add(1, Ordering::SeqCst);
        });
    }
    assert!(settle(|| handle.live() == 16).await);

    let drained = scope.shutdown(5_000).await;
    assert!(drained, "cooperative tasks should drain inside the timeout");
    assert_eq!(handle.live(), 0);
    assert_eq!(
        finished.load(Ordering::SeqCst),
        16,
        "every task ran its cleanup rather than being aborted mid-way"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_reports_failure_and_aborts_when_a_task_will_not_leave() {
    // The bounded wait must not become an unbounded one, and the caller has to be able to
    // tell the difference between "drained" and "had to be killed" (§24.5).
    let mut scope = TaskScope::new("stuck");
    let handle = scope.handle();
    scope.spawn(|_t| async move {
        loop {
            tokio::time::sleep(Duration::from_hours(1)).await;
        }
    });
    assert!(settle(|| handle.live() == 1).await);

    let started = std::time::Instant::now();
    let drained = scope.shutdown(150).await;
    let waited = started.elapsed();

    assert!(
        !drained,
        "a task that never leaves must be reported as such"
    );
    assert!(
        waited < Duration::from_secs(2),
        "the wait is bounded; it took {waited:?}"
    );
    assert!(
        settle(|| handle.live() == 0).await,
        "it was aborted after the bounded wait, {} still live",
        handle.live()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nested_scopes_each_clean_up_their_own() {
    // A TUI page inside a session inside a connection: dropping the inner one must not take
    // the outer one's tasks with it, and dropping the outer one must take everything.
    let mut outer = TaskScope::new("session");
    let outer_handle = outer.handle();
    for _ in 0..4 {
        outer.spawn(|t| async move { t.cancelled().await });
    }

    let inner_handle = {
        let mut inner = TaskScope::new("page");
        let h = inner.handle();
        for _ in 0..6 {
            inner.spawn(|t| async move { t.cancelled().await });
        }
        assert!(settle(|| h.live() == 6).await);
        h
    };

    assert!(settle(|| inner_handle.live() == 0).await);
    assert_eq!(
        outer_handle.live(),
        4,
        "closing a page must not cancel the session's own tasks"
    );

    drop(outer);
    assert!(settle(|| outer_handle.live() == 0).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_scope_dropped_mid_flight_leaves_nothing_running() {
    // The realistic shape of a cancelled operation: work is in progress when the user hits
    // Ctrl-C and the scope goes away underneath it.
    let ticks = Arc::new(AtomicUsize::new(0));
    let handle = {
        let mut scope = TaskScope::new("in-flight");
        let h = scope.handle();
        for _ in 0..16 {
            let t = Arc::clone(&ticks);
            scope.spawn(move |_token| async move {
                loop {
                    t.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            });
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(ticks.load(Ordering::Relaxed) > 0, "the tasks really ran");
        h
    };

    assert!(settle(|| handle.live() == 0).await);
    let after_drop = ticks.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        ticks.load(Ordering::Relaxed),
        after_drop,
        "a task kept running after its scope was dropped"
    );
}

// ================================================ PERF-03
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_thousand_open_close_cycles_leave_no_residue() {
    // PERF-03 in the form this crate can check: 1,000 scopes created, populated and dropped.
    // The `prc` side of PERF-03 cycles a real TUI; this one isolates the supervisor, so a
    // failure says which of the two is leaking.
    let observed = Arc::new(AtomicUsize::new(0));
    let mut peak = 0usize;

    for cycle in 0..1_000 {
        let handle = {
            let mut scope = TaskScope::new("page");
            let h = scope.handle();
            for _ in 0..4 {
                let o = Arc::clone(&observed);
                scope.spawn(move |token| async move {
                    o.fetch_add(1, Ordering::Relaxed);
                    token.cancelled().await;
                });
            }
            h
        };
        peak = peak.max(handle.live());
        assert!(
            settle(|| handle.live() == 0).await,
            "cycle {cycle}: {} task(s) survived",
            handle.live()
        );
    }

    assert!(
        observed.load(Ordering::Relaxed) > 0,
        "the cycles really spawned work"
    );
    assert!(peak <= 4, "a cycle held more than its own tasks: {peak}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_count_is_per_scope_not_global() {
    // Two sessions running at once must each report their own tasks, or `:tasks` is useless
    // for finding which one is leaking.
    let mut a = TaskScope::new("a");
    let mut b = TaskScope::new("b");
    let (ha, hb) = (a.handle(), b.handle());
    for _ in 0..3 {
        a.spawn(|t| async move { t.cancelled().await });
    }
    for _ in 0..7 {
        b.spawn(|t| async move { t.cancelled().await });
    }
    assert!(settle(|| ha.live() == 3 && hb.live() == 7).await);
    assert_eq!(a.name(), "a");
    assert_eq!(b.name(), "b");

    drop(a);
    assert!(settle(|| ha.live() == 0).await);
    assert_eq!(
        hb.live(),
        7,
        "dropping one scope must not disturb the other"
    );
    drop(b);
    assert!(settle(|| hb.live() == 0).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_completed_task_is_uncounted_without_waiting_for_the_scope() {
    // Otherwise a long-lived session's count only ever grows, and the number stops meaning
    // "work in progress".
    let mut scope = TaskScope::new("short");
    let handle = scope.handle();
    for _ in 0..20 {
        scope.spawn(|_t| async move { 42u8 });
    }
    assert!(
        settle(|| handle.live() == 0).await,
        "{} finished task(s) still counted",
        handle.live()
    );
    // And the scope is still usable afterwards.
    scope.spawn(|t| async move { t.cancelled().await });
    assert!(settle(|| handle.live() == 1).await);
}
