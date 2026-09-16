//! Test support for the process-global terminal token.
//!
//! [`Ownership`](crate::Ownership) is deliberately process-wide, which means two tests that
//! both claim it would fail depending on scheduling. This module hands out the token under a
//! lock so a test can say "I need the terminal" without either serialising the whole suite
//! with `--test-threads=1` or weakening the invariant it is there to check.
//!
//! It is compiled unconditionally so integration tests can use it. Nothing in the product
//! calls it.

use std::sync::{Mutex, MutexGuard, OnceLock};

static GATE: OnceLock<Mutex<()>> = OnceLock::new();

fn gate() -> &'static Mutex<()> {
    GATE.get_or_init(|| Mutex::new(()))
}

/// Holds the test lock for as long as it is alive.
///
/// Take this *before* constructing anything that claims the terminal, and keep it for the
/// whole test.
pub struct Claim {
    _guard: MutexGuard<'static, ()>,
}

/// Wait for exclusive use of the terminal token.
///
/// # Panics
/// Never on a poisoned lock: a panicking test leaves the mutex poisoned, and refusing to run
/// every later test because of it would hide the real failure. The guard is recovered.
#[must_use]
pub fn claim() -> Claim {
    let guard = gate()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Claim { _guard: guard }
}
