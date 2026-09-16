//! Penguin Redis — `pr-catalog` (v2.1 §31.3).
//!
//! Command metadata. The signed local catalog is the only authority for effects, danger and
//! key extraction; remote introspection may report availability and nothing more (ADR-030).

pub mod compile;
pub mod precedence;
pub mod snapshot;
pub mod spec;

pub use compile::{
    DELIBERATELY_UNKNOWN, Merged, OVERRIDES, Override, compile, compile_command, merge,
};
pub use precedence::{Availability, LocalSpec, RemoteReport, Resolved, resolve};
pub use snapshot::{LoadError, RawArg, RawCommand, Snapshot, load};
pub use spec::{
    CommandSpec, ExclusiveGroup, Family, Node, Role, Unit, eval_role_at, resolve_roles,
};
