//! The frozen JSON contract (v2.1 §8.3, §31.4, ADR-028, V-J02).
//!
//! One of the twelve types V-J02 freezes lives here: [`JsonNode`], defined in [`crate::dom`].
//! This module pins the schema version and gives the V-J02 test one place to look.

pub use crate::dom::JsonNode;

/// The version of the contract this crate owns.
///
/// ```
/// assert_eq!(pr_json::contract::SCHEMA_VERSION, 1);
/// ```
///
/// ADR-028 makes this the authoritative model for the server's business JSON, with duplicate
/// members addressed by occurrence index. A consumer that stored a path against one version of
/// that addressing must not silently reinterpret it under another.
pub const SCHEMA_VERSION: u32 = 1;

/// The contracts this crate owns, for the V-J02 test to walk.
pub const OWNED: &[&str] = &["JsonNode"];
