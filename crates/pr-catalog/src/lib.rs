//! Penguin Redis — `pr-catalog` (v2.1 §31.3).
//!
//! Command metadata. The signed local catalog is the only authority for effects, danger and
//! key extraction; remote introspection may report availability and nothing more (ADR-030).

pub mod precedence;

pub use precedence::{Availability, LocalSpec, RemoteReport, Resolved, resolve};
