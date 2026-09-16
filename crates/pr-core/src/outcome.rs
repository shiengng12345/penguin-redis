//! The single execution-result model (v2.1 §19.2 / §19.6, ADR-021, R30).
//!
//! v2.0 carried a flat `ExecutionOutcome` enum that conflated four independent questions.
//! v2.1 replaced it with four orthogonal dimensions, because a server error does **not**
//! imply "no side effects", and a render failure does **not** imply "not executed".

use std::fmt;

/// Did the request reach the server?
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Never left the client.
    NotSent,
    /// Cancelled before any byte was written.
    CancelledBeforeSend,
    /// Refused by local policy (v2.1 §23.1).
    DeniedByPolicy,
    /// All bytes were written.
    Sent,
    /// Bytes may or may not have arrived; the connection broke around transmission.
    /// This is a first-class outcome, never collapsed into a failure (ADR-007).
    UnknownAfterSend,
}

/// What came back?
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Nothing (yet, or by design).
    None,
    /// A reply was received and retained; the handle identifies it in the result store.
    Ok(ResponseHandle),
    /// The server returned an error reply.
    Error(ServerError),
    /// Queued inside `MULTI` — not applied (v2.1 §22.4).
    Queued,
    /// The connection is in `CLIENT REPLY OFF`/`SKIP`, so no reply is expected (v2.1 §22.6).
    SuppressedByReplyMode,
    /// Composite: `EXEC`, `--pipe`, batch and script results, each with its own four dimensions.
    Partial(Vec<ExecutionOutcome>),
}

/// What do we know about side effects?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectsCertainty {
    /// Nothing was sent, so nothing happened.
    NoCommandSent,
    /// A reply was observed that determines the effect.
    ReplyObserved,
    /// A conditional write did not apply (`SET .. NX` returning nil) — not a failure.
    ConditionNotApplied,
    /// Effects may have occurred; the reply does not settle it (e.g. error inside a
    /// composite operation, or `UnknownAfterSend`).
    EffectsPossible,
    /// Some sub-operations are known to have applied and others not.
    KnownPartial,
    /// Deliberately unacknowledged because of the reply mode.
    UnacknowledgedByMode,
}

/// What did the local presentation layer manage to do?
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderStatus {
    /// Not rendered (yet, or not applicable).
    NotRendered,
    /// Fully rendered.
    Rendered,
    /// Rendering failed locally; this never changes the other three dimensions.
    RenderFailed(DiagnosticId),
    /// Rendered, but only `retained` of the response is still available (v2.1 §24.3).
    Truncated {
        /// Byte range of the response that is still retained.
        retained: std::ops::Range<u64>,
    },
}

/// Opaque handle into the bounded result store.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResponseHandle(pub u64);

/// Local diagnostic identifier (never a server string).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticId(pub String);

/// A server error reply, split into code and message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerError {
    /// Leading token, e.g. `WRONGTYPE`, `NOPERM`, `MOVED`, `ERR`.
    pub code: String,
    /// Remainder of the error text. Untrusted — render through `SafeText`.
    pub message: crate::SafeText,
}

impl ServerError {
    /// Split a raw error reply into code + message.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Self {
        let text = String::from_utf8_lossy(raw);
        let (code, rest) = text.split_once(' ').unwrap_or((text.as_ref(), ""));
        Self {
            code: code.to_owned(),
            message: crate::SafeText::from_bytes(bytes::Bytes::copy_from_slice(rest.as_bytes())),
        }
    }
}

/// The complete outcome of one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionOutcome {
    /// Transport dimension.
    pub delivery: Delivery,
    /// Server reply dimension.
    pub reply: Reply,
    /// Side-effect certainty dimension.
    pub effects: EffectsCertainty,
    /// Local rendering dimension.
    pub render: RenderStatus,
}

/// Process exit codes (v2.1 §18.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ExitCode {
    /// Everything confirmed.
    Success = 0,
    /// Usage or configuration error.
    Usage = 2,
    /// Connection / TLS / authentication failure.
    Connection = 3,
    /// Redis returned a command error.
    CommandError = 4,
    /// Refused by policy.
    PolicyDenied = 5,
    /// Result unknown — the command may have been applied.
    ResultUnknown = 6,
    /// Batch completed with some failures.
    PartialBatch = 7,
    /// Local resource / output failure.
    LocalFailure = 8,
    /// Sent without acknowledgement (explicit reply-suppressed contract).
    SentUnacknowledged = 9,
    /// Cancelled by the user with no unknown side effects.
    Cancelled = 130,
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl ExecutionOutcome {
    /// Nothing was sent.
    #[must_use]
    pub fn not_sent() -> Self {
        Self {
            delivery: Delivery::NotSent,
            reply: Reply::None,
            effects: EffectsCertainty::NoCommandSent,
            render: RenderStatus::NotRendered,
        }
    }

    /// A confirmed, rendered reply.
    #[must_use]
    pub fn confirmed(handle: ResponseHandle) -> Self {
        Self {
            delivery: Delivery::Sent,
            reply: Reply::Ok(handle),
            effects: EffectsCertainty::ReplyObserved,
            render: RenderStatus::Rendered,
        }
    }

    /// True when the command may have taken effect without confirmation.
    #[must_use]
    pub fn is_uncertain(&self) -> bool {
        matches!(self.delivery, Delivery::UnknownAfterSend)
            || matches!(
                self.effects,
                EffectsCertainty::EffectsPossible | EffectsCertainty::KnownPartial
            )
    }

    /// Deterministic mapping to a process exit code (v2.1 §18.6, §22.3, R42).
    ///
    /// Precedence is by **effect certainty**, not by event order: any request that may have
    /// taken effect without confirmation outranks a user cancellation, so `130` reliably
    /// means "cancelled, and nothing is in doubt".
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        // 1. Uncertainty wins over everything (ADR-007).
        if matches!(self.delivery, Delivery::UnknownAfterSend) {
            return ExitCode::ResultUnknown;
        }
        if let Reply::Partial(children) = &self.reply {
            if children
                .iter()
                .any(|c| c.exit_code() == ExitCode::ResultUnknown)
            {
                return ExitCode::ResultUnknown;
            }
            if children.iter().any(|c| c.exit_code() != ExitCode::Success) {
                return ExitCode::PartialBatch;
            }
            return ExitCode::Success;
        }
        if matches!(
            self.effects,
            EffectsCertainty::EffectsPossible | EffectsCertainty::KnownPartial
        ) {
            return ExitCode::ResultUnknown;
        }
        // 2. Policy refusal.
        if matches!(self.delivery, Delivery::DeniedByPolicy) {
            return ExitCode::PolicyDenied;
        }
        // 3. Server error.
        if matches!(self.reply, Reply::Error(_)) {
            return ExitCode::CommandError;
        }
        // 4. Deliberately unacknowledged.
        if matches!(self.reply, Reply::SuppressedByReplyMode)
            || matches!(self.effects, EffectsCertainty::UnacknowledgedByMode)
        {
            return ExitCode::SentUnacknowledged;
        }
        // 5. Local render failure does not change delivery/reply/effects, but the process
        //    did fail to produce its output.
        if matches!(self.render, RenderStatus::RenderFailed(_)) {
            return ExitCode::LocalFailure;
        }
        // 6. Cancelled with nothing in doubt.
        if matches!(self.delivery, Delivery::CancelledBeforeSend) {
            return ExitCode::Cancelled;
        }
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn o(d: Delivery, r: Reply, e: EffectsCertainty) -> ExecutionOutcome {
        ExecutionOutcome {
            delivery: d,
            reply: r,
            effects: e,
            render: RenderStatus::Rendered,
        }
    }

    #[test]
    fn unknown_after_send_outranks_cancellation() {
        // STATE-01 / R42: 130 must never hide a possibly-applied write.
        let out = o(
            Delivery::UnknownAfterSend,
            Reply::None,
            EffectsCertainty::EffectsPossible,
        );
        assert_eq!(out.exit_code(), ExitCode::ResultUnknown);
        assert!(out.is_uncertain());
    }

    #[test]
    fn cancelled_before_send_is_130() {
        let out = o(
            Delivery::CancelledBeforeSend,
            Reply::None,
            EffectsCertainty::NoCommandSent,
        );
        assert_eq!(out.exit_code(), ExitCode::Cancelled);
        assert!(!out.is_uncertain());
    }

    #[test]
    fn server_error_does_not_imply_no_effects() {
        // A composite op can fail after partially applying; effects must be able to say so.
        let out = o(
            Delivery::Sent,
            Reply::Error(ServerError::parse(b"ERR partial")),
            EffectsCertainty::EffectsPossible,
        );
        assert_eq!(
            out.exit_code(),
            ExitCode::ResultUnknown,
            "effects uncertainty outranks the error code"
        );
        let clean = o(
            Delivery::Sent,
            Reply::Error(ServerError::parse(b"WRONGTYPE Operation against a key")),
            EffectsCertainty::ReplyObserved,
        );
        assert_eq!(clean.exit_code(), ExitCode::CommandError);
    }

    #[test]
    fn render_failure_never_changes_delivery_or_effects() {
        let mut out = ExecutionOutcome::confirmed(ResponseHandle(1));
        out.render = RenderStatus::RenderFailed(DiagnosticId("r-1".into()));
        assert_eq!(out.delivery, Delivery::Sent);
        assert_eq!(out.effects, EffectsCertainty::ReplyObserved);
        assert_eq!(out.exit_code(), ExitCode::LocalFailure);
    }

    #[test]
    fn policy_denial_is_5_and_sends_nothing() {
        let out = o(
            Delivery::DeniedByPolicy,
            Reply::None,
            EffectsCertainty::NoCommandSent,
        );
        assert_eq!(out.exit_code(), ExitCode::PolicyDenied);
    }

    #[test]
    fn queued_is_not_applied() {
        let out = o(
            Delivery::Sent,
            Reply::Queued,
            EffectsCertainty::EffectsPossible,
        );
        // STATE-03: QUEUED must never render as "applied".
        assert!(out.is_uncertain());
        assert_ne!(out.exit_code(), ExitCode::Success);
    }

    #[test]
    fn reply_suppressed_mode_is_9() {
        let out = o(
            Delivery::Sent,
            Reply::SuppressedByReplyMode,
            EffectsCertainty::UnacknowledgedByMode,
        );
        assert_eq!(out.exit_code(), ExitCode::SentUnacknowledged);
    }

    #[test]
    fn partial_batch_rolls_up_children() {
        let ok = ExecutionOutcome::confirmed(ResponseHandle(1));
        let err = o(
            Delivery::Sent,
            Reply::Error(ServerError::parse(b"ERR x")),
            EffectsCertainty::ReplyObserved,
        );
        let unknown = o(
            Delivery::UnknownAfterSend,
            Reply::None,
            EffectsCertainty::EffectsPossible,
        );

        let all_ok = ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Partial(vec![ok.clone(), ok.clone()]),
            effects: EffectsCertainty::ReplyObserved,
            render: RenderStatus::Rendered,
        };
        assert_eq!(all_ok.exit_code(), ExitCode::Success);

        let some_err = ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Partial(vec![ok.clone(), err]),
            effects: EffectsCertainty::KnownPartial,
            render: RenderStatus::Rendered,
        };
        assert_eq!(some_err.exit_code(), ExitCode::PartialBatch);

        let some_unknown = ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Partial(vec![ok, unknown]),
            effects: EffectsCertainty::KnownPartial,
            render: RenderStatus::Rendered,
        };
        assert_eq!(
            some_unknown.exit_code(),
            ExitCode::ResultUnknown,
            "one unknown child poisons the batch"
        );
    }

    #[test]
    fn conditional_write_not_applied_is_success_not_failure() {
        // `SET k v NX` returning nil is not an error (v2.1 §13.7 / CMD-05 family).
        let out = o(
            Delivery::Sent,
            Reply::Ok(ResponseHandle(7)),
            EffectsCertainty::ConditionNotApplied,
        );
        assert_eq!(out.exit_code(), ExitCode::Success);
        assert!(!out.is_uncertain());
    }

    #[test]
    fn server_error_parse_splits_code_and_escapes_message() {
        let e = ServerError::parse(b"WRONGTYPE Operation \x1b]0;x\x07against");
        assert_eq!(e.code, "WRONGTYPE");
        assert!(
            !e.message.as_display().contains('\x1b'),
            "error text must be escaped"
        );
        let bare = ServerError::parse(b"ERR");
        assert_eq!(bare.code, "ERR");
        assert!(bare.message.is_empty());
    }

    #[test]
    fn exit_code_values_match_the_spec_table() {
        assert_eq!(ExitCode::Success as u8, 0);
        assert_eq!(ExitCode::Usage as u8, 2);
        assert_eq!(ExitCode::Connection as u8, 3);
        assert_eq!(ExitCode::CommandError as u8, 4);
        assert_eq!(ExitCode::PolicyDenied as u8, 5);
        assert_eq!(ExitCode::ResultUnknown as u8, 6);
        assert_eq!(ExitCode::PartialBatch as u8, 7);
        assert_eq!(ExitCode::LocalFailure as u8, 8);
        assert_eq!(ExitCode::SentUnacknowledged as u8, 9);
        assert_eq!(ExitCode::Cancelled as u8, 130);
    }
}
