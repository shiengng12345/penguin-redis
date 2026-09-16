//! Penguin Redis — V-A03 synthetic RESP server.
//!
//! A programmable RESP2/RESP3 server used by Phase 0/1 tests: it encodes any frame (including
//! RESP3 streamed types, push and attribute), fragments output at arbitrary byte offsets,
//! streams a 1 GiB blob without materialising it, truncates, stalls, or closes on cue, and
//! ships a hostile corpus covering v2.1 §32.3.
//!
//! It is **not** the product decoder; see `pr-protocol` for that.

pub mod encode;
pub mod frame;
pub mod hostile;
pub mod inbound;
pub mod script;
pub mod server;

pub use encode::{EncodeError, Proto, encode, to_vec};
pub use frame::Frame;
pub use hostile::{Expectation, Sample, corpus};
pub use inbound::{Command, InboundError, parse_command};
pub use script::{Script, Step};
pub use server::{ServeError, SyntheticServer};
