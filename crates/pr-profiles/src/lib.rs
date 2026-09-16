//! Penguin Redis — `pr-profiles` (v2.1 §31.3).
//!
//! Connection profiles and the credential references they point at. A profile never holds a
//! secret; it holds a `credential:<uuid>` that resolves in the OS store (§4.3, ADR-010).

pub mod credentials;
pub mod journal;
pub mod perms;
pub mod shared;

pub use credentials::{
    CredentialError, CredentialStore, HeadlessFallback, PlatformStore, SERVICE, SecretRef,
    StoreKind, UnavailableStore, headless_options,
};
pub use journal::{Entry, Journal, JournalError, Reconciliation, State};
