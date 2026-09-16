//! The frozen security contracts (v2.1 §21.3, §23.2, §31.4, ADR-010, ADR-024, V-J02).
//!
//! Two of the twelve types V-J02 freezes live here: [`TrustIdentity`] and [`ApprovalToken`].
//! Both are defined in their own modules; this one pins the schema version and gives the V-J02
//! test one place to look.

pub use crate::approval::ApprovalToken;
pub use crate::trust::TrustIdentity;

/// The version of the contracts this crate owns.
///
/// ```
/// assert_eq!(pr_security::contract::SCHEMA_VERSION, 1);
/// ```
///
/// A stored approval token or a persisted trust identity carries this. A build that does not
/// recognise the version refuses the value rather than guessing at it — for a *credential
/// binding*, a wrong guess sends a password to the wrong server.
pub const SCHEMA_VERSION: u32 = 1;

/// The contracts this crate owns, for the V-J02 test to walk.
pub const OWNED: &[&str] = &["TrustIdentity", "ApprovalToken"];
