//! Penguin Redis — `pr-results` (v2.1 §31.3).
//!
//! Bounded retention with provenance. Retention is not output: a reply too large to keep can
//! still be printed in full, it simply cannot be inspected afterwards (§24.3, ADR-009).

pub mod store;
pub mod subscription;

pub use store::{
    Budget, OutputContext, OversizeAction, ResultStore, Retained, Unavailable, oversize_action,
};
pub use subscription::{Dropped, MAX_BYTES, MAX_ENTRIES, Message, Subscription};
