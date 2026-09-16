//! Penguin Redis — `pr-intelligence` (v2.1 §31.3).
//!
//! Local, deterministic command intelligence: the lenient analyser that describes a
//! half-typed line, and the suggestion machinery built on it. No network, no model
//! (ADR-015).

pub mod analyser;
pub mod broker;
pub mod discovery;
pub mod find;
pub mod scope;
pub mod vocabulary;
pub mod working_set;

pub use analyser::{Analysis, QuoteMode, Span, analyse, submittable};
pub use broker::{Accepted, Broker, Candidate, Dropped, Stamp};
pub use discovery::{Budget, Completeness, Cooldown, Discovery, MatchMode, Outcome, Step};
pub use find::{Finder, Hit, Purpose, Term};
pub use scope::{Observation, ObservationScope, ObservationStore, Origin};
pub use working_set::{BoundedNames, WorkingSet, limits};
