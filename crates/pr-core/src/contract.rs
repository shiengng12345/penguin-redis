//! The frozen interface skeleton this crate owns (v2.1 §11.11, §19.2, §31.4, V-J02).
//!
//! V-J02 freezes twelve types before Phase 1 begins, because the cost of changing one after
//! four crates depend on it is not the edit — it is discovering, months later, that two of
//! those crates disagreed about what a field meant.
//!
//! Frozen does not mean unchangeable. It means **versioned**: every contract carries a
//! [`SCHEMA_VERSION`], and `crates/pr-core/tests/contracts.rs` asserts that all twelve exist,
//! carry a version, and have a doc test that actually constructs one. A doc test rather than
//! prose because a type whose example does not compile is a type whose documentation has
//! already drifted from it.
//!
//! This crate owns five of the twelve: [`CommandRequest`], [`ExecutionOutcome`],
//! [`crate::SafeText`],
//! [`crate::TaskScope`] and [`ResultRecord`].

use crate::outcome::ExecutionOutcome;
use crate::request::CommandRequest;

/// The version of the contracts this crate owns.
///
/// Bumped when a frozen type changes shape. A consumer that stores a serialised contract —
/// history, a spool file, a diagnostic bundle — compares this and refuses what it cannot read,
/// rather than misreading it.
///
/// ```
/// assert_eq!(pr_core::contract::SCHEMA_VERSION, 1);
/// ```
pub const SCHEMA_VERSION: u32 = 1;

/// One executed command and everything retained about it (v2.1 §19.2, §24.3).
///
/// The unit `:inspect` and Result Context work over. It holds the **request**, the
/// **outcome** and what is still in memory — and says plainly when the reply is no longer
/// retained, because §12.4 forbids presenting "we dropped it" as "it does not exist".
///
/// ```
/// use bytes::Bytes;
/// use pr_core::contract::{ResultRecord, Retention};
/// use pr_core::{CommandRequest, Effects, RequestOrigin};
///
/// let req = CommandRequest::new(
///     vec![Bytes::from_static(b"GET"), Bytes::from_static(b"k")],
///     RequestOrigin::User,
///     Effects::read(),
/// );
/// let record = ResultRecord::new(17, req);
/// assert_eq!(record.id, 17);
/// // Nothing has been retained yet, and the record says so rather than looking empty.
/// assert_eq!(record.retention, Retention::NotRetained);
/// assert!(!record.retention.is_present());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRecord {
    /// Monotonic id within the session, as shown in `#17`.
    pub id: u64,
    /// The request exactly as it went out.
    pub request: CommandRequest,
    /// The four-dimensional outcome, once one exists.
    pub outcome: Option<ExecutionOutcome>,
    /// What is still held, and why not if nothing is.
    pub retention: Retention,
    /// Bytes the reply occupied, whether or not it is still retained.
    ///
    /// Kept after eviction on purpose: "the reply was 400 MiB and was not kept" is useful and
    /// "nothing here" is not.
    pub reply_bytes: usize,
}

impl ResultRecord {
    /// A record for a request that has gone out but not yet been answered.
    #[must_use]
    pub fn new(id: u64, request: CommandRequest) -> Self {
        Self {
            id,
            request,
            outcome: None,
            retention: Retention::NotRetained,
            reply_bytes: 0,
        }
    }
}

/// Whether a reply is still available locally, and why not (v2.1 §12.4, §24.3, §24.4).
///
/// ```
/// use pr_core::contract::Retention;
/// // Only `Held` means the data is there. Everything else is a reason, and none of them is
/// // "the key does not exist" -- that is a statement about the server, which a local budget
/// // is not entitled to make.
/// assert!(Retention::Held.is_present());
/// assert!(!Retention::EvictedForBudget.is_present());
/// assert!(Retention::Spooled.is_present());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    /// Held in memory and readable now.
    Held,
    /// Written to an authorised spool file; readable, at the cost of a disk read.
    Spooled,
    /// Dropped to stay inside §24.3's store budget.
    EvictedForBudget,
    /// Never retained: it was streamed straight out (§24.4).
    Streamed,
    /// Nothing has arrived yet.
    NotRetained,
}

impl Retention {
    /// Whether the reply can still be read locally.
    #[must_use]
    pub fn is_present(self) -> bool {
        matches!(self, Self::Held | Self::Spooled)
    }

    /// What to tell a user who asked to inspect it.
    ///
    /// Never says the data does not exist: a local eviction is a fact about this process.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Held => "retained in memory",
            Self::Spooled => "retained on disk in an authorised spool",
            Self::EvictedForBudget => {
                "not retained: dropped to stay inside the result-store budget. Re-run the \
                 command to see it again"
            }
            Self::Streamed => {
                "not retained: it was written straight out rather than kept. Re-run with a \
                 retaining output mode to inspect it"
            }
            Self::NotRetained => "no reply has arrived yet",
        }
    }
}

/// Everything this crate freezes, for the contract test to walk.
///
/// A list rather than a doc sentence, so "all twelve are frozen" is a thing a test can check
/// rather than a thing a reader has to count.
pub const OWNED: &[&str] = &[
    "CommandRequest",
    "ExecutionOutcome",
    "ResultRecord",
    "SafeText",
    "TaskScope",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn eviction_never_reads_as_absence() {
        // §12.4's rule, in the type that would otherwise be where it gets lost: a store that
        // dropped a reply must not let a caller render "not found".
        for r in [
            Retention::EvictedForBudget,
            Retention::Streamed,
            Retention::NotRetained,
        ] {
            assert!(!r.is_present());
            let m = r.message().to_lowercase();
            assert!(!m.contains("does not exist"), "{r:?}: {m}");
            assert!(!m.contains("not found"), "{r:?}: {m}");
            assert!(
                m.contains("not retained") || m.contains("no reply"),
                "{r:?}: {m}"
            );
        }
    }

    #[test]
    fn a_record_keeps_the_size_of_a_reply_it_no_longer_holds() {
        use crate::{Effects, RequestOrigin};
        use bytes::Bytes;
        let req = CommandRequest::new(
            vec![Bytes::from_static(b"GET"), Bytes::from_static(b"big")],
            RequestOrigin::User,
            Effects::read(),
        );
        let mut rec = ResultRecord::new(1, req);
        rec.reply_bytes = 400 * 1024 * 1024;
        rec.retention = Retention::EvictedForBudget;
        assert!(!rec.retention.is_present());
        assert_eq!(
            rec.reply_bytes,
            400 * 1024 * 1024,
            "the size outlives the data"
        );
    }
}
