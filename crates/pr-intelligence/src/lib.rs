//! Penguin Redis — `pr-intelligence` (v2.1 §31.3).
//!
//! Local, deterministic command intelligence: the lenient analyser that describes a
//! half-typed line, and the suggestion machinery built on it. No network, no model
//! (ADR-015).

pub mod analyser;
pub mod broker;
pub mod scope;

pub use analyser::{Analysis, QuoteMode, Span, analyse, submittable};
pub use broker::{Accepted, Broker, Candidate, Dropped, Stamp};
pub use scope::{Observation, ObservationScope, ObservationStore, Origin};
