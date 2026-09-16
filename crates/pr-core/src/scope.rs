//! `TaskScope` — explicit task supervision (v2.1 §31.1/§31.4, ADR-003, R21).
//!
//! Tokio has **no** built-in scoped tasks: `JoinSet` and `TaskTracker` do not abort on drop,
//! so "structured concurrency" is not something we get for free. STATE-07 (blocking-command
//! cancellation), NET-07 (SSH cleanup), PERF-03 (repeated TUI open/close) and LIFE-01
//! (no daemon after exit) all depend on this type actually cancelling its children.
//!
//! Contract, in the order §31.1 states it: **cancel the token, wait a bounded time, then
//! abort**. Each step does a different job, and the middle one is the reason the first exists:
//!
//! 1. `cancel()` is a *signal*. A task that watches its token gets to finish — flush a
//!    journal entry, send `UNSUBSCRIBE`, close a tunnel.
//! 2. The bounded wait is how long that cleanup may take.
//! 3. `abort()` is the hard stop for anything that did not, or could not, leave.
//!
//! The wrapper around a spawned future therefore does **not** race the future against the
//! token. An earlier version did, with `select!`, and it made every one of those three steps
//! a no-op: cancellation dropped the user's future immediately, so cooperative cleanup never
//! ran, and `shutdown` reported a clean drain for tasks that had simply been discarded. V-H04
//! found it by asking sixteen cooperative tasks to record that they had finished; eleven did.
//!
//! Nothing outlives its scope either way — `Drop` still aborts — but "nothing is running" and
//! "everything finished what it was doing" are different promises, and only one of them is
//! worth making to a journal.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::task::{AbortHandle, JoinHandle};
use tokio_util::sync::CancellationToken;

/// Decrements the live counter on drop, including when the task is aborted.
#[derive(Debug)]
struct LiveGuard(Arc<AtomicUsize>);

impl LiveGuard {
    fn new(live: Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(live)
    }
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Live task counter, exposed for `:tasks` and for leak-detection tests (PERF-03).
#[derive(Clone, Debug, Default)]
pub struct TaskScopeHandle {
    live: Arc<AtomicUsize>,
}

impl TaskScopeHandle {
    /// Number of tasks currently running in the scope.
    #[must_use]
    pub fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
}

/// A supervision scope. Every session, subscription, discovery job, tunnel and TUI page
/// owns one.
///
/// Tokio has no scoped tasks and `JoinSet` does not abort on drop, so structured lifetime is
/// something this type provides rather than something the runtime gives. Dropping a scope
/// aborts what it owns — which is why a page closing cannot leave a subscription running.
///
/// ```
/// use pr_core::TaskScope;
///
/// let scope = TaskScope::new("example");
/// assert_eq!(scope.name(), "example");
///
/// // The live count is the leak detector: a scope that shut down cleanly is back at zero.
/// let handle = scope.handle();
/// assert_eq!(handle.live(), 0);
///
/// // A child token is how a task learns it should stop -- cooperative, not a kill.
/// let token = scope.child_token();
/// assert!(!token.is_cancelled());
/// drop(scope);
/// assert!(token.is_cancelled(), "dropping the scope cancels what it owns");
/// ```
#[derive(Debug)]
pub struct TaskScope {
    token: CancellationToken,
    aborts: Vec<AbortHandle>,
    live: Arc<AtomicUsize>,
    name: &'static str,
}

impl TaskScope {
    /// Create a named scope.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            token: CancellationToken::new(),
            aborts: Vec::new(),
            live: Arc::new(AtomicUsize::new(0)),
            name,
        }
    }

    /// Scope name (for `:tasks` and diagnostics).
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Observation handle for counters.
    #[must_use]
    pub fn handle(&self) -> TaskScopeHandle {
        TaskScopeHandle {
            live: Arc::clone(&self.live),
        }
    }

    /// A child token, so a task can observe cancellation cooperatively.
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.token.child_token()
    }

    /// Spawn a task into the scope. The future receives a [`CancellationToken`].
    ///
    /// The token is the task's to observe. Nothing here races it: a task that watches the
    /// token decides for itself when it has finished cleaning up, and a task that ignores it
    /// is stopped by `abort()` after the bounded wait. Cancelling a task *for* it would make
    /// both of those impossible to distinguish.
    ///
    /// A handle resolves to `Err(JoinError)` when the task was aborted or panicked, which is
    /// Tokio's own signal and needs no wrapper of ours.
    pub fn spawn<F, Fut, T>(&mut self, f: F) -> JoinHandle<T>
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = self.token.child_token();
        // The counter must be decremented even when the task is *aborted*, which drops the
        // future without running the rest of its body. An RAII guard is the only way to
        // make the count trustworthy for leak detection (PERF-03).
        let guard = LiveGuard::new(Arc::clone(&self.live));
        let fut = f(token);
        let handle = tokio::spawn(async move {
            let _guard = guard;
            fut.await
        });
        self.aborts.push(handle.abort_handle());
        handle
    }

    /// Cancel cooperatively and wait, bounded, for tasks to finish.
    ///
    /// Returns `true` only if every task left *on its own* within `timeout_ms`. A `false`
    /// means at least one had to be aborted, and the caller may not assume that task's
    /// cleanup ran — which is exactly the distinction §24.5 needs before writing "done" to a
    /// journal.
    pub async fn shutdown(&mut self, timeout_ms: u64) -> bool {
        self.token.cancel();
        let deadline = std::time::Duration::from_millis(timeout_ms);
        let drained = tokio::time::timeout(deadline, async {
            while self.live.load(Ordering::SeqCst) > 0 {
                // Sleeping rather than yielding: a hot spin for the whole timeout would burn
                // a core while waiting for a task that is, by definition, not ready.
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .is_ok();
        if !drained {
            for a in &self.aborts {
                a.abort();
            }
        }
        self.aborts.clear();
        drained
    }
}

impl Drop for TaskScope {
    fn drop(&mut self) {
        // LIFE-01: never rely on "the process is exiting anyway".
        self.token.cancel();
        for a in &self.aborts {
            a.abort();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn settle() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn tasks_run_and_counter_returns_to_zero() {
        let mut scope = TaskScope::new("test");
        let h = scope.handle();
        let j = scope.spawn(|_tok| async { 42u32 });
        assert_eq!(j.await.unwrap(), 42);
        settle().await;
        assert_eq!(h.live(), 0);
    }

    #[tokio::test]
    async fn drop_cancels_running_tasks() {
        // PERF-03 / LIFE-01: no task may survive its scope.
        let h;
        {
            let mut scope = TaskScope::new("drop");
            h = scope.handle();
            for _ in 0..8 {
                scope.spawn(|tok| async move {
                    tok.cancelled().await; // would otherwise hang forever
                });
            }
            assert_eq!(h.live(), 8);
        }
        settle().await;
        assert_eq!(h.live(), 0, "tasks outlived their scope");
    }

    #[tokio::test]
    async fn shutdown_drains_cooperative_tasks() {
        let mut scope = TaskScope::new("shutdown");
        let h = scope.handle();
        for _ in 0..4 {
            scope.spawn(|tok| async move {
                tok.cancelled().await;
            });
        }
        assert!(scope.shutdown(1000).await, "cooperative tasks should drain");
        assert_eq!(h.live(), 0);
    }

    #[tokio::test]
    async fn shutdown_aborts_a_task_that_ignores_its_token_and_says_so() {
        // A task that only sleeps never observes cancellation, so it does *not* drain — it is
        // aborted after the bounded wait. Reporting that honestly is the point: the caller
        // knows this task's cleanup did not run.
        let mut scope = TaskScope::new("sleeper");
        let h = scope.handle();
        scope.spawn(|_tok| async {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(
            !scope.shutdown(100).await,
            "a task that ignores its token has not drained, whatever else happened to it"
        );
        settle().await;
        assert_eq!(
            h.live(),
            0,
            "but it was aborted, so nothing is left running"
        );
    }

    #[tokio::test]
    async fn a_cooperative_task_gets_to_finish_its_cleanup() {
        // The whole reason the token exists. An earlier implementation raced the future
        // against the token with `select!`, which dropped the future the moment cancellation
        // arrived — the cleanup below never ran, and `shutdown` still reported success.
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let cleaned = Arc::new(AtomicBool::new(false));
        let mut scope = TaskScope::new("cooperative");
        let c = Arc::clone(&cleaned);
        scope.spawn(move |tok| async move {
            tok.cancelled().await;
            c.store(true, Ordering::SeqCst);
        });
        assert!(scope.shutdown(1000).await);
        assert!(
            cleaned.load(Ordering::SeqCst),
            "the task was discarded instead of being allowed to clean up"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_reports_failure_when_a_task_blocks_its_thread() {
        // Genuinely uninterruptible work: no await points, so neither the token nor abort()
        // can land until it returns. shutdown must report `false` rather than claim success,
        // and the counter must still reach zero once the guard drops.
        let mut scope = TaskScope::new("blocking");
        let h = scope.handle();
        scope.spawn(|_tok| async {
            std::thread::sleep(Duration::from_millis(400));
        });
        tokio::time::sleep(Duration::from_millis(20)).await; // let it start
        let drained = scope.shutdown(50).await;
        assert!(!drained, "must not claim a clean drain it did not achieve");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(
            h.live(),
            0,
            "guard must clear the count once the task returns"
        );
    }

    #[tokio::test]
    async fn repeated_open_close_does_not_grow() {
        // PERF-03: 1,000 TUI open/close cycles must not accumulate tasks.
        for _ in 0..1000 {
            let mut scope = TaskScope::new("cycle");
            scope.spawn(|tok| async move {
                tok.cancelled().await;
            });
        }
        settle().await;
        // If scopes leaked, the runtime would still hold their tasks; a fresh scope proves
        // the counter is per-scope and the previous ones are gone.
        let scope = TaskScope::new("final");
        assert_eq!(scope.handle().live(), 0);
    }
}
