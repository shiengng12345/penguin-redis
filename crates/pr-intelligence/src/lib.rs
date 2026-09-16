//! Penguin Redis — `pr-intelligence` (v2.1 §31.3).
//!
//! Local, deterministic command intelligence: the lenient analyser that describes a
//! half-typed line, and the suggestion machinery built on it. No network, no model
//! (ADR-015).

pub mod analyser;

pub use analyser::{Analysis, QuoteMode, Span, analyse, submittable};
