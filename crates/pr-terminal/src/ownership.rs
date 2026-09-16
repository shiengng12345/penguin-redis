//! Exclusive ownership of the terminal (v2.1 §14.4, ADR-022, V-C02).
//!
//! §14.4's rule is that a Tokio worker, the line editor and a TUI painter must never read
//! stdin or write stdout at the same time. The failure it prevents is not theoretical: two
//! writers interleaving escape sequences leaves the cursor somewhere neither of them thinks
//! it is, and the user's half-typed line is destroyed.
//!
//! A comment cannot prevent that. This module makes the rule a resource: there is one token,
//! taking it twice fails, and dropping it gives it back. Everything that paints or reads
//! needs the token, so a second reader is a compile-and-run failure rather than a race that
//! shows up once a week in someone's terminal.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set while a [`Ownership`] token exists.
static OWNED: AtomicBool = AtomicBool::new(false);

/// Why the terminal could not be claimed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OwnershipError {
    /// Something already holds the terminal.
    ///
    /// This is not a retryable condition: it means two parts of the program both believe they
    /// drive the terminal, which is a bug in their wiring rather than a busy resource.
    #[error("the terminal is already owned; only one coordinator may read stdin or paint")]
    AlreadyOwned,
}

/// Proof that the holder is the only thing reading or painting the terminal.
///
/// Not `Clone`, not `Copy`, and released on drop.
#[derive(Debug)]
pub struct Ownership {
    /// Prevent construction from outside this module.
    _private: (),
}

impl Ownership {
    /// Claim the terminal.
    ///
    /// # Errors
    /// [`OwnershipError::AlreadyOwned`] if a token is already outstanding.
    pub fn acquire() -> Result<Self, OwnershipError> {
        OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self { _private: () })
            .map_err(|_| OwnershipError::AlreadyOwned)
    }

    /// Whether the terminal is currently owned. For diagnostics and tests.
    #[must_use]
    pub fn is_owned() -> bool {
        OWNED.load(Ordering::Acquire)
    }
}

impl Drop for Ownership {
    fn drop(&mut self) {
        OWNED.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The token is process-global, so a test that claims it takes the shared test lock
    /// first. Without that, two tests would fail depending on scheduling rather than on the
    /// invariant they are checking.
    #[test]
    fn the_terminal_has_exactly_one_owner() {
        let _c = crate::testing::claim();
        assert!(!Ownership::is_owned(), "nothing should hold it at rest");

        let first = Ownership::acquire().expect("the first claim succeeds");
        assert!(Ownership::is_owned());

        // A second reader is refused rather than silently allowed to interleave.
        assert_eq!(
            Ownership::acquire().unwrap_err(),
            OwnershipError::AlreadyOwned
        );
        assert_eq!(
            Ownership::acquire().unwrap_err(),
            OwnershipError::AlreadyOwned,
            "and it stays refused"
        );

        drop(first);
        assert!(!Ownership::is_owned(), "dropping releases it");

        // Which means a TUI can hand the terminal back to the REPL and the REPL can claim it.
        let second = Ownership::acquire().expect("reclaimable after release");
        assert!(Ownership::is_owned());
        drop(second);
        assert!(!Ownership::is_owned());
    }

    #[test]
    fn a_panicking_holder_still_releases_it() {
        // Release is on drop, so unwinding past the holder does not strand the terminal.
        let _c = crate::testing::claim();
        let r = std::panic::catch_unwind(|| {
            let _t = Ownership::acquire().expect("claim");
            assert!(Ownership::is_owned());
            panic!("simulated failure while painting");
        });
        assert!(r.is_err());
        assert!(
            !Ownership::is_owned(),
            "a panic must not leave the terminal claimed forever"
        );
    }
}
