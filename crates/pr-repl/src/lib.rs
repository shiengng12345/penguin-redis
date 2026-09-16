//! Penguin Redis — `pr-repl` (v2.1 §31.3).
//!
//! The authoritative request tokenizer ([`quoting`]) and the separate grammar for local
//! `:` commands ([`local`]). They are deliberately different languages: one must match
//! `redis-cli` byte for byte, the other is Penguin's own (ADR-002, R18).

pub mod buffer;
pub mod entries;
pub mod history;
pub mod local;
pub mod quoting;

pub use buffer::{EditError, LineBuffer, TextEdit};
pub use entries::{ENTRIES, KeyEntry, KeyMap, entry_for, text_entries};
pub use history::{
    Entry, History, HistoryError, RedactionLevel, SCHEMA_VERSION, Scope, has_placeholder, redact,
};
pub use local::{LocalCommand, LocalError, parse_local};
pub use quoting::{Token, TokenizeError, is_incomplete, quote, split_args};
