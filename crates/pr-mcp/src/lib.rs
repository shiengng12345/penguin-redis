//! Penguin Redis — `pr-mcp`: the MCP stdio adapter (v2.1 §29.4, V-D08).
//!
//! Two things live here, and §29.4 gives each a pass criterion:
//!
//! - [`stdio`] — the bootstrap. `--mcp-stdio` is a **separate** start-up path: no REPL, no TUI,
//!   no terminal coordinator, no banner, no connection selector, no interactive credential
//!   prompt. **stdout carries JSON-RPC frames and nothing else**, which is the part an
//!   integration test asserts byte for byte, because one stray `println!` anywhere in the
//!   crate graph breaks an MCP host with a parse error that points nowhere near the cause.
//! - [`policy`] — the approval boundary. An agent may not approve its own requests, and a
//!   policy may only cover read-only work inside limits a person wrote down.

pub mod policy;
pub mod stdio;
pub mod tools;

pub use policy::{PolicyRefusal, PolicyRule};
pub use tools::{Authorisation, Tool, ToolRefusal};
