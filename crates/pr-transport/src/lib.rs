//! Penguin Redis — `pr-transport` (v2.1 §31.3).
//!
//! Phase 0 holds one thing: the [`oneshot`] passthrough the V-A02 differential harness runs
//! against. TCP/TLS/socket and the controlled SSH path arrive with V-G03/V-G04.

pub mod oneshot;

pub use oneshot::{CallError, Oneshot, encode_command};
