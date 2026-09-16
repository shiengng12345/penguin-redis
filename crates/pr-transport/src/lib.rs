//! Penguin Redis — `pr-transport` (v2.1 §31.3).
//!
//! Phase 0 holds two things: the [`oneshot`] passthrough the V-A02 differential harness runs
//! against, and the [`tls`] layer V-G04 measures. The controlled SSH path arrives with V-G03.

pub mod oneshot;
pub mod tls;

pub use oneshot::{CallError, Oneshot, connect_tcp, encode_command};
pub use tls::{TlsConfig, TlsError, TlsStream};
