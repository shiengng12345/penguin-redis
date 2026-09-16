//! V-B04 — the outcome / exit-code matrix, exercised end to end.
//!
//! §19.2 defines four orthogonal dimensions and §18.6 a mapping to exit codes. A table that
//! is only asserted against hand-written values proves nothing about the decoder, so these
//! tests drive real bytes from the V-A03 synthetic server through the real decoder and only
//! then build the outcome.
//!
//! The cases that matter are the ones where a naive client gets it wrong: a truncated reply
//! after the bytes were sent, an error inside a composite operation, and a cancellation that
//! must not mask an uncertain write.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_core::{
    Delivery, DiagnosticId, EffectsCertainty, ExecutionOutcome, ExitCode, RenderStatus, Reply,
    ResponseHandle, ServerError,
};
use pr_protocol::{DecodeError, Decoder, Step, Value};
use resp_server::{Frame, Proto, Script, Step as ServerStep, SyntheticServer};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Drive one scripted exchange and return whatever bytes arrived.
async fn exchange(script: Script, send: &[u8], read_for: Duration) -> Vec<u8> {
    let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
    let addr = srv.addr().unwrap();
    let server = tokio::spawn(async move { srv.run_once(&script).await });

    let mut c = TcpStream::connect(addr).await.unwrap();
    c.write_all(send).await.unwrap();
    let mut out = Vec::new();
    let mut buf = vec![0u8; 8192];
    let deadline = tokio::time::Instant::now() + read_for;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, c.read(&mut buf)).await {
            Ok(Ok(0) | Err(_)) | Err(_) => break,
            Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
        }
    }
    let _ = server.await;
    out
}

fn decode_one(bytes: &[u8]) -> Result<Step, DecodeError> {
    let mut d = Decoder::with_defaults();
    d.feed(bytes);
    d.decode()
}

/// Build the outcome the kernel would record for a completed exchange.
fn outcome_for(bytes: &[u8]) -> ExecutionOutcome {
    match decode_one(bytes) {
        Ok(Step::Value(Value::Error(e))) => ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Error(ServerError::parse(&e)),
            effects: EffectsCertainty::ReplyObserved,
            render: RenderStatus::Rendered,
        },
        Ok(Step::Value(_)) => ExecutionOutcome::confirmed(ResponseHandle(1)),
        // Nothing usable arrived although the request went out — either an incomplete
        // stream or a framing error. Both mean the server may well have applied it, which
        // is the STATE-01 case (ADR-007).
        Ok(Step::Incomplete) | Err(_) => ExecutionOutcome {
            delivery: Delivery::UnknownAfterSend,
            reply: Reply::None,
            effects: EffectsCertainty::EffectsPossible,
            render: RenderStatus::NotRendered,
        },
    }
}

const PING: &[u8] = b"*1\r\n$4\r\nPING\r\n";

#[tokio::test]
async fn a_complete_reply_is_confirmed_and_exits_zero() {
    let script = Script::new().reply(Frame::simple("OK"));
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;
    let o = outcome_for(&bytes);
    assert_eq!(o.delivery, Delivery::Sent);
    assert_eq!(o.exit_code(), ExitCode::Success);
    assert!(!o.is_uncertain());
}

#[tokio::test]
async fn a_server_error_reply_exits_four_and_keeps_its_code() {
    let script = Script::new().reply(Frame::error("WRONGTYPE Operation against a key"));
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;
    let o = outcome_for(&bytes);
    match &o.reply {
        Reply::Error(e) => assert_eq!(e.code, "WRONGTYPE"),
        other => panic!("expected an error reply, got {other:?}"),
    }
    assert_eq!(o.exit_code(), ExitCode::CommandError);
}

#[tokio::test]
async fn a_reply_cut_mid_stream_is_unknown_not_a_failure() {
    // STATE-01: the command reached the server; the reply did not come back. Reporting this
    // as a plain failure is what makes a client retry a write it already performed.
    let script = Script::new()
        .then(ServerStep::ReadCommand)
        .then(ServerStep::TruncatedBulk {
            declared: 1_000_000,
            actual: 8,
        });
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;
    assert!(!bytes.is_empty(), "a prefix did arrive");
    assert_eq!(decode_one(&bytes).unwrap(), Step::Incomplete);

    let o = outcome_for(&bytes);
    assert_eq!(o.delivery, Delivery::UnknownAfterSend);
    assert_eq!(o.exit_code(), ExitCode::ResultUnknown);
    assert!(o.is_uncertain());
}

#[tokio::test]
async fn a_connection_closed_before_any_reply_is_also_unknown() {
    let script = Script::new()
        .then(ServerStep::ReadCommand)
        .then(ServerStep::Close);
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;
    assert!(bytes.is_empty());
    let o = outcome_for(&bytes);
    assert_eq!(o.exit_code(), ExitCode::ResultUnknown);
}

#[tokio::test]
async fn a_protocol_error_is_unknown_after_send_not_success() {
    // Framing is lost, so the next byte boundary is unknowable and the command's fate is too.
    let script = Script::new()
        .then(ServerStep::ReadCommand)
        .then(ServerStep::SendRaw(b"@garbage\r\n".to_vec()));
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;
    assert!(matches!(decode_one(&bytes), Err(DecodeError::Protocol(_))));
    assert_eq!(outcome_for(&bytes).exit_code(), ExitCode::ResultUnknown);
}

#[tokio::test]
async fn a_fragmented_reply_still_confirms() {
    // Chunking is a transport detail and must not change the outcome.
    let whole = resp_server::to_vec(&Frame::bulk(b"Hello world"), Proto::Resp3).unwrap();
    let script = Script::new()
        .then(ServerStep::ReadCommand)
        .then(ServerStep::SendChunked {
            bytes: whole,
            chunk: 1,
            delay_ms: 0,
        });
    let bytes = exchange(script, PING, Duration::from_millis(800)).await;
    let o = outcome_for(&bytes);
    assert_eq!(o.exit_code(), ExitCode::Success);
}

#[tokio::test]
async fn push_frames_do_not_answer_the_command() {
    // §19.3: a push must not occupy the reply slot. Decoding it first leaves the real reply
    // still to come, so the command is not yet confirmed by it.
    let mut wire = resp_server::to_vec(
        &Frame::Push(vec![
            Frame::simple("message"),
            Frame::bulk(b"ch"),
            Frame::bulk(b"hi"),
        ]),
        Proto::Resp3,
    )
    .unwrap();
    wire.extend_from_slice(&resp_server::to_vec(&Frame::simple("OK"), Proto::Resp3).unwrap());
    let script = Script::new()
        .then(ServerStep::ReadCommand)
        .then(ServerStep::SendRaw(wire));
    let bytes = exchange(script, PING, Duration::from_millis(400)).await;

    let mut d = Decoder::with_defaults();
    d.feed(&bytes);
    let first = d.decode().unwrap();
    match first {
        Step::Value(v) => assert!(
            v.is_push(),
            "the push must decode first and be identifiable"
        ),
        Step::Incomplete => panic!("expected a push, got Incomplete"),
    }
    let second = d.decode().unwrap();
    assert_eq!(
        second,
        Step::Value(Value::Simple(bytes::Bytes::from_static(b"OK")))
    );
}

// ---------------------------------------------------------------- pure mapping matrix

#[test]
fn the_full_dimension_matrix_maps_deterministically() {
    // Every combination the kernel can produce, checked against §18.6 in one place.
    let cases: Vec<(Delivery, Reply, EffectsCertainty, ExitCode, &str)> = vec![
        (
            Delivery::NotSent,
            Reply::None,
            EffectsCertainty::NoCommandSent,
            ExitCode::Success,
            "nothing happened",
        ),
        (
            Delivery::CancelledBeforeSend,
            Reply::None,
            EffectsCertainty::NoCommandSent,
            ExitCode::Cancelled,
            "130 only when nothing is in doubt",
        ),
        (
            Delivery::DeniedByPolicy,
            Reply::None,
            EffectsCertainty::NoCommandSent,
            ExitCode::PolicyDenied,
            "policy refusal",
        ),
        (
            Delivery::Sent,
            Reply::Ok(ResponseHandle(1)),
            EffectsCertainty::ReplyObserved,
            ExitCode::Success,
            "ordinary success",
        ),
        (
            Delivery::Sent,
            Reply::Ok(ResponseHandle(1)),
            EffectsCertainty::ConditionNotApplied,
            ExitCode::Success,
            "SET NX returning nil is not a failure",
        ),
        (
            Delivery::Sent,
            Reply::Error(ServerError::parse(b"ERR x")),
            EffectsCertainty::ReplyObserved,
            ExitCode::CommandError,
            "clean server error",
        ),
        (
            Delivery::Sent,
            Reply::Error(ServerError::parse(b"ERR partial")),
            EffectsCertainty::EffectsPossible,
            ExitCode::ResultUnknown,
            "an error inside a composite op may still have applied",
        ),
        (
            Delivery::Sent,
            Reply::Queued,
            EffectsCertainty::EffectsPossible,
            ExitCode::ResultUnknown,
            "QUEUED is not applied",
        ),
        (
            Delivery::Sent,
            Reply::SuppressedByReplyMode,
            EffectsCertainty::UnacknowledgedByMode,
            ExitCode::SentUnacknowledged,
            "CLIENT REPLY OFF",
        ),
        (
            Delivery::UnknownAfterSend,
            Reply::None,
            EffectsCertainty::EffectsPossible,
            ExitCode::ResultUnknown,
            "STATE-01",
        ),
    ];

    for (delivery, reply, effects, expected, why) in cases {
        let o = ExecutionOutcome {
            delivery,
            reply,
            effects,
            render: RenderStatus::Rendered,
        };
        assert_eq!(o.exit_code(), expected, "{why}");
    }
}

#[test]
fn uncertainty_outranks_cancellation_in_every_combination() {
    // R42: 130 must never hide a write that may have landed.
    for effects in [
        EffectsCertainty::EffectsPossible,
        EffectsCertainty::KnownPartial,
    ] {
        let o = ExecutionOutcome {
            delivery: Delivery::CancelledBeforeSend,
            reply: Reply::None,
            effects,
            render: RenderStatus::Rendered,
        };
        assert_eq!(
            o.exit_code(),
            ExitCode::ResultUnknown,
            "{effects:?} must outrank 130"
        );
    }
}

#[test]
fn a_render_failure_never_rewrites_the_other_dimensions() {
    let mut o = ExecutionOutcome::confirmed(ResponseHandle(9));
    o.render = RenderStatus::RenderFailed(DiagnosticId("r-1".into()));
    assert_eq!(o.delivery, Delivery::Sent);
    assert_eq!(o.effects, EffectsCertainty::ReplyObserved);
    assert_eq!(o.exit_code(), ExitCode::LocalFailure);
}

#[test]
fn a_partial_batch_is_governed_by_its_worst_child() {
    let ok = ExecutionOutcome::confirmed(ResponseHandle(1));
    let err = ExecutionOutcome {
        delivery: Delivery::Sent,
        reply: Reply::Error(ServerError::parse(b"ERR x")),
        effects: EffectsCertainty::ReplyObserved,
        render: RenderStatus::Rendered,
    };
    let unknown = ExecutionOutcome {
        delivery: Delivery::UnknownAfterSend,
        reply: Reply::None,
        effects: EffectsCertainty::EffectsPossible,
        render: RenderStatus::Rendered,
    };

    let batch = |children: Vec<ExecutionOutcome>| ExecutionOutcome {
        delivery: Delivery::Sent,
        reply: Reply::Partial(children),
        effects: EffectsCertainty::KnownPartial,
        render: RenderStatus::Rendered,
    };

    assert_eq!(
        batch(vec![ok.clone(), ok.clone()]).exit_code(),
        ExitCode::Success
    );
    assert_eq!(
        batch(vec![ok.clone(), err]).exit_code(),
        ExitCode::PartialBatch
    );
    assert_eq!(
        batch(vec![ok, unknown]).exit_code(),
        ExitCode::ResultUnknown,
        "one uncertain child poisons the whole batch"
    );
}

#[test]
fn every_exit_code_is_reachable_from_some_outcome() {
    // A code nothing can produce is a spec bug; this keeps §18.6 honest.
    use std::collections::HashSet;
    let mut seen: HashSet<u8> = HashSet::new();
    let samples = [
        ExecutionOutcome::not_sent(),
        ExecutionOutcome::confirmed(ResponseHandle(1)),
        ExecutionOutcome {
            delivery: Delivery::CancelledBeforeSend,
            reply: Reply::None,
            effects: EffectsCertainty::NoCommandSent,
            render: RenderStatus::Rendered,
        },
        ExecutionOutcome {
            delivery: Delivery::DeniedByPolicy,
            reply: Reply::None,
            effects: EffectsCertainty::NoCommandSent,
            render: RenderStatus::Rendered,
        },
        ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Error(ServerError::parse(b"ERR x")),
            effects: EffectsCertainty::ReplyObserved,
            render: RenderStatus::Rendered,
        },
        ExecutionOutcome {
            delivery: Delivery::UnknownAfterSend,
            reply: Reply::None,
            effects: EffectsCertainty::EffectsPossible,
            render: RenderStatus::Rendered,
        },
        ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::SuppressedByReplyMode,
            effects: EffectsCertainty::UnacknowledgedByMode,
            render: RenderStatus::Rendered,
        },
        ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Partial(vec![
                ExecutionOutcome::confirmed(ResponseHandle(1)),
                ExecutionOutcome {
                    delivery: Delivery::Sent,
                    reply: Reply::Error(ServerError::parse(b"ERR x")),
                    effects: EffectsCertainty::ReplyObserved,
                    render: RenderStatus::Rendered,
                },
            ]),
            effects: EffectsCertainty::KnownPartial,
            render: RenderStatus::Rendered,
        },
        {
            let mut o = ExecutionOutcome::confirmed(ResponseHandle(1));
            o.render = RenderStatus::RenderFailed(DiagnosticId("d".into()));
            o
        },
    ];
    for o in samples {
        seen.insert(o.exit_code() as u8);
    }
    for expected in [0u8, 4, 5, 6, 7, 8, 9, 130] {
        assert!(
            seen.contains(&expected),
            "exit code {expected} is unreachable"
        );
    }
}
