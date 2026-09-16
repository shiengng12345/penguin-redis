//! Penguin Redis — `pr-repl` (v2.1 §31.3).
//!
//! The authoritative request tokenizer ([`quoting`]) and the separate grammar for local
//! `:` commands ([`local`]). They are deliberately different languages: one must match
//! `redis-cli` byte for byte, the other is Penguin's own (ADR-002, R18).

pub mod local;
pub mod quoting;

pub use local::{LocalCommand, LocalError, parse_local};
pub use quoting::{Token, TokenizeError, is_incomplete, quote, split_args};
