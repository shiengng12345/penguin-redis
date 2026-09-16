//! Penguin Redis — `pr-application` (v2.1 §31.3).
//!
//! The use cases every entry point shares. Nothing here talks to a terminal; the CLI, TUI,
//! recipes, MCP and `--pipe` all funnel through the same admission and policy (ADR-008).

pub mod pipe;

pub use pipe::{Admission, Admitted, PipeError, PipePolicy, admit};
