//! Penguin Redis — `pr-routing` (v2.1 §31.3).
//!
//! Cluster slot mapping with hash tags, redirect parsing, and node admission against the
//! profile's discovery bounds (§21.2, §21.3).

pub mod slot;

pub use slot::{Redirect, SLOT_COUNT, hash_tag, parse_redirect, same_slot, slot_for};
