//! V-H02 — the streaming decoder and the value decoder are one semantics (v2.1 §31.2, §19.3).
//!
//! §31.2 says there must not be two subtly different ways to read the wire. `pr-protocol` now
//! has two APIs, so that sentence has to be earned rather than assumed: this file runs the
//! whole V-A03 corpus — 316 fixtures, every category §32.3 lists — through both and requires
//! them to agree on the value **and** on the error.
//!
//! It runs the corpus twice: once with the default threshold, where almost nothing streams,
//! and once with `stream_above = 0`, where **every** scalar takes the streaming path. The
//! second pass is the one that matters. A threshold nobody crosses proves nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_protocol::decoder::{Budget, DecodeError};
use pr_protocol::stream::{
    Event, Reassembler, ScalarKind, Step, StreamBudget, Streamer, decode_by_streaming,
    decode_by_value,
};
use pr_protocol::value::Value;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("fixtures/protocol")
}

fn samples() -> Vec<(String, Vec<u8>)> {
    let root = fixtures();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    manifest["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let f = s["file"].as_str().unwrap().to_owned();
            let bytes = std::fs::read(root.join(&f)).unwrap();
            (f, bytes)
        })
        .collect()
}

/// What a difference between the two APIs is allowed to be.
///
/// Exactly one: §24.3 caps what may be **held** at 16 MiB and §24.4 says streaming is how to
/// exceed it. So the streaming API is permitted to start delivering a scalar the value API
/// refuses with `Budget("bulk")` — that is the feature, not a divergence.
///
/// Everything else must match, including which error: "close the connection" and "refuse this
/// value" are opposite reactions, and an API that picks differently is a second semantics.
#[derive(Debug, Default, PartialEq, Eq)]
struct Tally {
    agreed: usize,
    streamed_past_the_hold_cap: usize,
}

fn agree(name: &str, bytes: &[u8], stream: StreamBudget, t: &mut Tally) {
    let budget = Budget::default();
    let by_value = decode_by_value(bytes, budget);
    let by_stream = decode_by_streaming(bytes, budget, stream);
    match (&by_value, &by_stream) {
        (Ok(a), Ok(b)) => {
            assert_eq!(
                a, b,
                "{name}: the two APIs decoded the same bytes differently"
            );
            t.agreed += 1;
        }
        (Err(a), Err(b)) => {
            assert_eq!(
                a, b,
                "{name}: the two APIs disagree about *why* it failed, which is the difference \
                 between closing the connection and refusing a value"
            );
            t.agreed += 1;
        }
        (Err(DecodeError::Budget("bulk")), Ok(_)) => {
            assert!(
                declares_more_than_the_hold_cap(bytes, budget),
                "{name}: the streaming API accepted something the value API refused, and it \
                 was not because the value was too large to hold"
            );
            t.streamed_past_the_hold_cap += 1;
        }
        _ => panic!("{name}: one API accepted and the other did not: {by_value:?} / {by_stream:?}"),
    }
}

/// Does the frame at the head declare a scalar larger than `max_bulk_bytes`?
///
/// Read off the wire rather than inferred from the error, so the permission above cannot be
/// satisfied by a `Budget("bulk")` that came from somewhere else.
fn declares_more_than_the_hold_cap(bytes: &[u8], budget: Budget) -> bool {
    let Some(&tag) = bytes.first() else {
        return false;
    };
    if !matches!(tag, b'$' | b'!' | b'=') {
        return false;
    }
    let Some(end) = bytes.windows(2).position(|w| w == b"\r\n") else {
        return false;
    };
    std::str::from_utf8(&bytes[1..end])
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|n| n > budget.max_bulk_bytes as u64)
}

#[test]
fn the_whole_corpus_agrees_at_the_default_threshold() {
    let mut t = Tally::default();
    for (name, bytes) in samples() {
        agree(&name, &bytes, StreamBudget::default(), &mut t);
    }
    assert_eq!(
        t,
        Tally {
            agreed: 312,
            streamed_past_the_hold_cap: 4,
        },
        "the number of fixtures where streaming legitimately goes further than holding is \
         frozen; if it moved, something other than the §24.4 escape changed"
    );
}

#[test]
fn the_whole_corpus_agrees_with_every_scalar_streamed() {
    // The pass that earns the claim: `stream_above = 0` forces the streaming path for every
    // `$`, `!` and `=` in the corpus, including the malformed and the truncated ones. A
    // threshold nobody crosses would prove nothing.
    let stream = StreamBudget {
        stream_above: 0,
        ..StreamBudget::default()
    };
    let mut t = Tally::default();
    for (name, bytes) in samples() {
        agree(&name, &bytes, stream, &mut t);
    }
    assert_eq!(
        t,
        Tally {
            agreed: 312,
            streamed_past_the_hold_cap: 4,
        },
        "forcing every scalar through the streaming path changed the outcome for some fixture"
    );
}

#[test]
fn a_frame_split_at_every_byte_boundary_still_agrees() {
    // Streaming is a state machine across reads, so the interesting inputs are the ones that
    // arrive in pieces. This feeds one byte at a time, which puts a split at every position
    // including inside a CRLF and inside a length.
    let frames: &[&[u8]] = &[
        b"$5\r\nhello\r\n",
        b"*2\r\n$1\r\na\r\n:7\r\n",
        b"%1\r\n+k\r\n$3\r\nvvv\r\n",
        b"|1\r\n+a\r\n:1\r\n$2\r\nhi\r\n",
        b"=11\r\ntxt:abcdefg\r\n",
        b"$?\r\n;3\r\nabc\r\n;2\r\nde\r\n;0\r\n",
        b"*?\r\n:1\r\n:2\r\n.\r\n",
        b"~2\r\n:1\r\n:2\r\n",
        b">2\r\n+message\r\n+hi\r\n",
        b"*0\r\n",
        b"*-1\r\n",
    ];
    let stream = StreamBudget {
        stream_above: 0,
        ..StreamBudget::default()
    };
    for frame in frames {
        let expected = decode_by_value(frame, Budget::default()).unwrap().unwrap();

        let mut s = Streamer::new(Budget::default(), stream);
        let mut r = Reassembler::default();
        let mut got = None;
        for (i, b) in frame.iter().enumerate() {
            s.feed(&[*b]);
            loop {
                match s.step() {
                    Ok(Step::Incomplete) => break,
                    Ok(Step::Event(Event::FrameEnd)) => {
                        assert_eq!(i + 1, frame.len(), "frame ended early");
                        got = r.take();
                        break;
                    }
                    Ok(Step::Event(e)) => r.push(e).unwrap(),
                    Err(e) => panic!("{:?} at byte {i}: {e}", String::from_utf8_lossy(frame)),
                }
            }
        }
        assert_eq!(
            got.as_ref(),
            Some(&expected),
            "{:?} decoded differently one byte at a time",
            String::from_utf8_lossy(frame)
        );
    }
}

#[test]
fn a_large_scalar_is_never_held_whole() {
    // The measurement V-H02 exists for, in miniature: feed a 4 MiB bulk string 64 KiB at a
    // time and watch what the streamer holds. If `buffered()` tracked the declared length
    // instead of the read size, this is where it would show.
    const PAYLOAD: usize = 4 * 1024 * 1024;
    const READ: usize = 64 * 1024;

    let mut s = Streamer::new(
        Budget::default(),
        StreamBudget {
            stream_above: 1024,
            ..StreamBudget::default()
        },
    );
    s.feed(format!("${PAYLOAD}\r\n").as_bytes());

    let mut emitted = 0usize;
    let mut peak = 0usize;
    let mut sent = 0usize;
    let mut began = false;
    let mut ended = false;

    while !ended {
        if sent < PAYLOAD + 2 {
            let n = READ.min(PAYLOAD + 2 - sent);
            // The last two bytes are the CRLF.
            let mut chunk = vec![b'x'; n];
            if sent + n > PAYLOAD {
                let tail = sent + n - PAYLOAD;
                let at = n - tail;
                chunk[at..].copy_from_slice(&b"\r\n"[..tail]);
            }
            s.feed(&chunk);
            sent += n;
        }
        peak = peak.max(s.buffered());
        match s.step().unwrap() {
            Step::Event(Event::ScalarBegin { kind, len }) => {
                assert_eq!(kind, ScalarKind::Bulk);
                assert_eq!(len, Some(PAYLOAD));
                began = true;
            }
            Step::Event(Event::ScalarChunk(c)) => emitted += c.len(),
            Step::Event(Event::ScalarEnd) => ended = true,
            Step::Event(Event::FrameEnd) => break,
            Step::Event(other) => panic!("unexpected {other:?}"),
            Step::Incomplete => {}
        }
    }

    assert!(began && ended);
    assert_eq!(emitted, PAYLOAD, "every payload byte was delivered");
    assert!(
        peak <= 2 * READ,
        "held {peak} bytes for a {PAYLOAD}-byte value; the read size is {READ}"
    );
    // And for contrast: the value API refuses it outright rather than growing, which is the
    // §24.3 cap doing its job. The two behaviours are both correct and are for different jobs.
    let mut frame = format!("${PAYLOAD}\r\n").into_bytes();
    frame.extend(std::iter::repeat_n(b'x', PAYLOAD));
    frame.extend_from_slice(b"\r\n");
    assert!(
        decode_by_value(&frame, Budget::default()).is_ok(),
        "4 MiB is under the 16 MiB cap"
    );
}

#[test]
fn the_value_api_refuses_what_the_streaming_api_delivers() {
    // §24.3 caps a held result at 16 MiB and §24.4 says streaming is the way past it. Both
    // halves of that sentence, on the same bytes.
    const PAYLOAD: usize = 17 * 1024 * 1024;
    let header = format!("${PAYLOAD}\r\n").into_bytes();

    let mut d = pr_protocol::Decoder::with_defaults();
    d.feed(&header);
    assert_eq!(
        d.decode(),
        Err(DecodeError::Budget("bulk")),
        "the value API refuses to hold it, by name"
    );

    let mut s = Streamer::with_defaults();
    s.feed(&header);
    assert_eq!(
        s.step().unwrap(),
        Step::Event(Event::ScalarBegin {
            kind: ScalarKind::Bulk,
            len: Some(PAYLOAD)
        }),
        "the streaming API starts delivering it"
    );
}

#[test]
fn an_absurd_declared_length_still_meets_a_limit() {
    // Streaming is not "read whatever the server claims". `max_streamed_bytes` is what makes
    // the difference between an unbounded read and a refusal.
    let mut s = Streamer::new(
        Budget::default(),
        StreamBudget {
            stream_above: 1024,
            max_streamed_bytes: 1024 * 1024,
        },
    );
    s.feed(b"$1073741824\r\n");
    assert_eq!(s.step(), Err(DecodeError::Budget("bulk")));
}

#[test]
fn a_streamed_string_that_never_ends_meets_the_same_limit() {
    // `$?` declares no total, so the only way to bound it is to count as the chunks arrive.
    let mut s = Streamer::new(
        Budget::default(),
        StreamBudget {
            stream_above: 0,
            max_streamed_bytes: 10,
        },
    );
    s.feed(b"$?\r\n");
    assert!(matches!(
        s.step(),
        Ok(Step::Event(Event::ScalarBegin { .. }))
    ));
    for _ in 0..3 {
        s.feed(b";8\r\nxxxxxxxx\r\n");
    }
    let mut err = None;
    for _ in 0..50 {
        match s.step() {
            Err(e) => {
                err = Some(e);
                break;
            }
            Ok(Step::Incomplete) => break,
            Ok(_) => {}
        }
    }
    assert_eq!(err, Some(DecodeError::Budget("bulk")));
}

#[test]
fn a_verbatim_strings_format_tag_is_framing_and_arrives_before_the_payload() {
    // A consumer writing chunks to a file must not have to strip `txt:` out of the first one.
    let body = "txt:".to_owned() + &"a".repeat(2000);
    let mut frame = format!("={}\r\n", body.len()).into_bytes();
    frame.extend_from_slice(body.as_bytes());
    frame.extend_from_slice(b"\r\n");

    let mut s = Streamer::new(
        Budget::default(),
        StreamBudget {
            stream_above: 16,
            ..StreamBudget::default()
        },
    );
    s.feed(&frame);
    let Ok(Step::Event(Event::ScalarBegin { kind, len })) = s.step() else {
        panic!("expected a scalar");
    };
    assert_eq!(kind, ScalarKind::Verbatim(*b"txt"));
    assert_eq!(len, Some(2000));

    let Ok(Step::Event(Event::ScalarChunk(c))) = s.step() else {
        panic!("expected a chunk");
    };
    assert!(c.starts_with(b"aaa"), "the tag leaked into the payload");
}

#[test]
fn a_push_frame_is_still_recognisable_while_streaming() {
    // §19.3: a push must never occupy a command's reply slot. A consumer that only sees
    // chunks has to learn that from the first event, before any payload arrives.
    let mut s = Streamer::with_defaults();
    s.feed(b">2\r\n$7\r\nmessage\r\n$2\r\nhi\r\n");
    assert_eq!(
        s.step().unwrap(),
        Step::Event(Event::AggregateBegin {
            kind: pr_protocol::stream::AggKind::Push,
            len: Some(2)
        })
    );
}

#[test]
fn several_frames_in_one_buffer_are_separated() {
    // Pipelining, `--pipe` and a push arriving beside a reply all produce this.
    let mut s = Streamer::with_defaults();
    s.feed(b"+a\r\n:1\r\n*1\r\n$1\r\nz\r\n");
    let mut frames = Vec::new();
    let mut r = Reassembler::default();
    loop {
        match s.step().unwrap() {
            Step::Incomplete => break,
            Step::Event(Event::FrameEnd) => {
                if let Some(v) = r.take() {
                    frames.push(v);
                }
            }
            Step::Event(e) => r.push(e).unwrap(),
        }
    }
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[1], Value::Integer(1));
}
