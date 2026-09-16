//! Penguin Redis — `pr-core` (v2.1 §31.3).
//!
//! Foundation types shared by every layer: the display trust boundary ([`SafeText`]), the
//! four-dimensional execution result ([`ExecutionOutcome`]), the request that reaches the
//! kernel ([`CommandRequest`]), and task supervision ([`TaskScope`]).
//!
//! This crate depends on no terminal, renderer, network or AI code — the dependency arrow
//! points *into* it (v2.1 §31.4).

pub mod outcome;
pub mod request;
pub mod safetext;
pub mod scope;
pub mod session;

pub use outcome::{
    Delivery, DiagnosticId, EffectsCertainty, ExecutionOutcome, ExitCode, RenderStatus, Reply,
    ResponseHandle, ServerError,
};
pub use request::{CommandRequest, Effects, RequestBudget, RequestOrigin};
pub use safetext::SafeText;
pub use scope::{TaskScope, TaskScopeHandle};
pub use session::{Protocol, ReconnectLosses, ReplyMode, SessionRefusal, SessionState, TxState};
