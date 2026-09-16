//! Penguin Redis — `pr-application` (v2.1 §31.3).
//!
//! The use cases every entry point shares. Nothing here talks to a terminal; the CLI, TUI,
//! recipes, MCP and `--pipe` all funnel through the same admission and policy (ADR-008).

pub mod json_projection;
pub mod output;
pub mod pipe;

pub use json_projection::{Projection, Protocol, Strictness, is_error, project, project_many};
pub use output::{
    Arrival, Emitter, FlagError, OutputMode, PushRoute, encode_resp3, is_push, push_route,
    resolve_protocol, select,
};
pub use pipe::{Admission, Admitted, PipeError, PipePolicy, admit};
