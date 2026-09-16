//! Penguin Redis — `pr-security` (v2.1 §31.3).
//!
//! Policy, approval and trust. Two review BLOCKERs live here:
//! [`ApprovalToken`] binds to exact argv bytes rather than a rendered digest (ADR-024),
//! and [`TrustIdentity`] gives "endpoint fingerprint" a real definition (ADR-010).

pub mod approval;
pub mod contract;
pub mod secrets;
pub mod trust;
pub mod uri;

pub use approval::{
    ApprovalRefusal, ApprovalToken, Epochs, ExecutionContext, Issuer, escape_bytes, plan_hash,
    preview, request_hash,
};
pub use secrets::{REDACTED, SecretSet, scrub, scrub_str};
pub use trust::{
    Advisory, AuthIdentity, DiscoveryBounds, Endpoint, KnownNodes, RebindVerdict, RedirectDecision,
    RotationVerdict, RotationWindow, ServerIdentity, TlsIdentity, TrustIdentity,
};
