//! V-B06 — push frames and the one-shot output contract (v2.1 §18.1, §19.3, R17, R40).
//!
//! The requirement is short and the failure is silent: **stdout contains the reply and
//! nothing else**. A script that pipes `prc --json GET k` into `jq` has to keep working while
//! another client writes to a tracked key and the server sends an invalidation.
//!
//! So these tests do not hand the emitter values it constructed. They feed the real decoder
//! real wire bytes, with the push arriving before the reply, after it, and on its own — the
//! three orderings §18.1 calls out — and then assert on the exact stdout and stderr.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_application::output::{Arrival, Emitter, FlagError, OutputMode, PushRoute};
use pr_application::{encode_resp3, is_push, push_route, resolve_protocol, select};
use pr_protocol::{Decoder, Step, Value};

/// Decode a whole byte stream into values, keeping the bytes each one came from.
///
/// The wire slices are what `--trace-wire` records, so they have to be the real ones rather
/// than a re-encoding.
fn decode_all(wire: &[u8]) -> Vec<(Value, Vec<u8>)> {
    let mut d = Decoder::with_defaults();
    d.feed(wire);
    let mut out = Vec::new();
    let mut offset = 0usize;
    loop {
        let before = d.buffered();
        match d.decode().expect("the fixture is well formed") {
            Step::Value(v) => {
                let consumed = before - d.buffered();
                out.push((v, wire[offset..offset + consumed].to_vec()));
                offset += consumed;
            }
            Step::Incomplete => break,
        }
    }
    out
}

/// Run a stream through an emitter.
fn run(mode: OutputMode, show_pushes: bool, wire: &[u8]) -> Emitter {
    let frames = decode_all(wire);
    let mut e = Emitter::new(mode, show_pushes);
    for (v, raw) in &frames {
        e.accept(&Arrival {
            value: v,
            wire: raw,
        });
    }
    e.finish();
    e
}

// The three orderings. Each carries the same reply so the stdout assertion is identical.
const PUSH: &[u8] = b">3\r\n$10\r\ninvalidate\r\n$1\r\n1\r\n$7\r\nplayer:\r\n";
const REPLY: &[u8] = b"$5\r\nhello\r\n";

fn push_before() -> Vec<u8> {
    [PUSH, REPLY].concat()
}
fn push_after() -> Vec<u8> {
    [REPLY, PUSH].concat()
}
fn push_alone() -> Vec<u8> {
    PUSH.to_vec()
}
fn pushes_around() -> Vec<u8> {
    [PUSH, REPLY, PUSH, PUSH].concat()
}

// ================================================ stdout carries the reply only
#[test]
fn a_one_shot_reply_mode_never_lets_a_push_onto_stdout() {
    for mode in [
        OutputMode::Json,
        OutputMode::Raw,
        OutputMode::Csv,
        OutputMode::Pretty,
    ] {
        for (name, wire) in [
            ("before", push_before()),
            ("after", push_after()),
            ("around", pushes_around()),
        ] {
            let e = run(mode, false, &wire);
            let out = e.stdout_text();
            assert!(
                !out.contains("invalidate"),
                "{mode:?} with the push {name} leaked it onto stdout: {out:?}"
            );
            assert_eq!(e.replies(), 1, "{mode:?}/{name}: exactly one reply");
            assert!(
                out.contains("hello"),
                "{mode:?}/{name}: the reply is still there"
            );
            assert_eq!(
                out.lines().count(),
                1,
                "{mode:?}/{name}: a reply is one record, got {out:?}"
            );
        }
    }
}

#[test]
fn suppressed_pushes_are_counted_on_stderr() {
    // Dropping them silently would leave a user with no way to know their invalidations are
    // being thrown away.
    let e = run(OutputMode::Json, false, &push_before());
    assert_eq!(e.pushes(), 1);
    assert_eq!(e.stderr_text(), "1 push frame suppressed\n");

    let e = run(OutputMode::Json, false, &pushes_around());
    assert_eq!(e.pushes(), 3);
    assert_eq!(e.stderr_text(), "3 push frames suppressed\n");

    // And nothing is said when there were none: a clean run stays quiet.
    let e = run(OutputMode::Json, false, REPLY);
    assert_eq!(e.pushes(), 0);
    assert_eq!(e.stderr_text(), "");
}

#[test]
fn a_push_with_no_reply_at_all_produces_empty_stdout() {
    for mode in [OutputMode::Json, OutputMode::Raw, OutputMode::Csv] {
        let e = run(mode, false, &push_alone());
        assert_eq!(
            e.stdout_text(),
            "",
            "{mode:?}: a push alone must not produce a record"
        );
        assert_eq!(e.replies(), 0);
        assert_eq!(e.stderr_text(), "1 push frame suppressed\n");
    }
}

// ================================================ --show-pushes
#[test]
fn show_pushes_writes_ndjson_events_to_stderr_and_leaves_stdout_alone() {
    let e = run(OutputMode::Json, true, &pushes_around());
    let out = e.stdout_text();
    assert!(out.contains("hello"));
    assert!(!out.contains("invalidate"), "stdout is still reply-only");
    assert_eq!(out.lines().count(), 1);

    let err = e.stderr_text();
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 3, "one NDJSON line per push: {err:?}");
    for l in &lines {
        assert!(l.starts_with(r#"{"v":1,"kind":"push""#), "line was {l:?}");
        assert!(l.contains("invalidate"));
        assert!(l.ends_with('}'));
    }
    assert!(
        !err.contains("suppressed"),
        "they were shown, not suppressed"
    );
}

#[test]
fn the_ndjson_events_carry_arrival_order() {
    // A consumer correlating an invalidation with the reply it preceded needs the order, and
    // a sequence number is the only thing that survives the two streams being read
    // separately.
    let e = run(OutputMode::Json, true, &pushes_around());
    let err = e.stderr_text();
    let seqs: Vec<&str> = err
        .lines()
        .filter_map(|l| l.split(r#""seq":"#).nth(1))
        .filter_map(|r| r.split(',').next())
        .collect();
    assert_eq!(
        seqs,
        vec!["1", "3", "4"],
        "the push before the reply is 1 and the two after are 3 and 4: {err:?}"
    );
}

// ================================================ event formats
#[test]
fn ndjson_puts_pushes_and_replies_on_stdout_in_arrival_order() {
    let e = run(OutputMode::Ndjson, false, &pushes_around());
    let lines: Vec<&str> = e
        .stdout
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| std::str::from_utf8(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    let kinds: Vec<&str> = lines
        .iter()
        .map(|l| {
            if l.contains(r#""kind":"push""#) {
                "push"
            } else {
                "reply"
            }
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["push", "reply", "push", "push"],
        "arrival order is the contract"
    );
    assert_eq!(
        e.stderr_text(),
        "",
        "nothing is suppressed in an event stream"
    );

    // `kind` comes first so a consumer can dispatch a line before parsing all of it.
    for l in &lines {
        assert!(l.starts_with(r#"{"v":1,"kind":"#), "{l:?}");
    }
}

#[test]
fn show_pushes_does_not_duplicate_events_in_an_event_stream() {
    // Both routes firing would make a consumer that reads stdout and stderr see every push
    // twice.
    let e = run(OutputMode::Ndjson, true, &pushes_around());
    assert_eq!(e.stdout_text().lines().count(), 4);
    assert_eq!(
        e.stderr_text(),
        "",
        "pushes are already on stdout in this mode"
    );
    assert_eq!(push_route(OutputMode::Ndjson, true), PushRoute::StdoutEvent);
    assert_eq!(
        push_route(OutputMode::TypedJson, true),
        PushRoute::StdoutEvent
    );
}

#[test]
fn an_event_says_what_type_it_carries() {
    let wire = b">2\r\n$7\r\nmessage\r\n$3\r\nabc\r\n:42\r\n";
    let e = run(OutputMode::Ndjson, false, wire);
    let out = e.stdout_text();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains(r#""type":"push""#));
    assert!(lines[1].contains(r#""type":"integer""#));
    assert!(lines[1].contains(r#""value":42"#));
}

#[test]
fn binary_that_is_not_utf8_is_base64_not_lossy_text() {
    // A lossy string would silently replace bytes with U+FFFD, and the consumer would never
    // know its value changed.
    let wire = b"$3\r\n\xff\xfe\x80\r\n";
    let e = run(OutputMode::TypedJson, false, wire);
    let out = e.stdout_text();
    assert!(out.contains(r#""b64":"#), "{out:?}");
    assert!(!out.contains('\u{fffd}'), "no lossy replacement: {out:?}");
    assert!(out.contains("//6A"), "base64 of ff fe 80: {out:?}");
}

#[test]
fn a_double_keeps_its_lexeme_in_an_event() {
    // `1.2300` and `inf` are not JSON numbers, and rounding them would lose what the server
    // said.
    for (wire, want) in [
        (&b",1.2300\r\n"[..], "1.2300"),
        (b",inf\r\n", "inf"),
        (b",-0\r\n", "-0"),
    ] {
        let e = run(OutputMode::Ndjson, false, wire);
        assert!(
            e.stdout_text().contains(want),
            "{want} was not preserved: {}",
            e.stdout_text()
        );
    }
}

#[test]
fn a_map_becomes_pairs_not_an_object() {
    // Redis map keys can repeat and can be binary; an object would drop a duplicate silently
    // (ADR-005).
    let wire = b"%2\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n";
    let e = run(OutputMode::Ndjson, false, wire);
    let out = e.stdout_text();
    assert!(out.contains(r#"[["a",1],["a",2]]"#), "{out:?}");
}

// ================================================ --output resp
#[test]
fn output_resp_reencodes_normalised_resp3_and_drops_pushes() {
    let e = run(OutputMode::Resp, false, &pushes_around());
    assert_eq!(
        e.stdout, b"$5\r\nhello\r\n",
        "the reply and only the reply, re-encoded"
    );
    assert_eq!(e.stderr_text(), "3 push frames suppressed\n");
}

#[test]
fn resp_normalises_every_null_spelling_to_the_resp3_one() {
    // A consumer of `--output resp` should not have to handle three nulls; which one arrived
    // is still on the decoded value for anyone who cares.
    for wire in [&b"$-1\r\n"[..], b"*-1\r\n", b"_\r\n"] {
        let e = run(OutputMode::Resp, false, wire);
        assert_eq!(e.stdout, b"_\r\n", "from {wire:?}");
    }
}

#[test]
fn resp_preserves_bytes_exactly() {
    // Round-tripping is the whole promise of the mode.
    for wire in [
        &b",1.2300\r\n"[..],
        b",inf\r\n",
        b"(3492890328409238509324850943850943825024385\r\n",
        b"=15\r\ntxt:Some string\r\n",
        b"!21\r\nSYNTAX invalid syntax\r\n",
        b"#t\r\n",
        b"~2\r\n:1\r\n:2\r\n",
        b"%1\r\n$1\r\na\r\n:1\r\n",
    ] {
        let e = run(OutputMode::Resp, false, wire);
        assert_eq!(
            e.stdout,
            wire,
            "{} did not round-trip",
            String::from_utf8_lossy(wire)
        );
    }
}

#[test]
fn resp_writes_an_attribute_immediately_before_the_value_it_decorates() {
    // §18.1: the re-encoding uses a *defined* order rather than whatever the server happened
    // to do. An attribute belongs in front of its value.
    let wire = b"|1\r\n$3\r\nttl\r\n:100\r\n$5\r\nhello\r\n";
    let e = run(OutputMode::Resp, false, wire);
    let out = e.stdout.clone();
    let bar = out.iter().position(|b| *b == b'|').expect("attribute");
    let dollar = out
        .windows(2)
        .position(|w| w == b"$5")
        .expect("the decorated value");
    assert!(bar < dollar, "the attribute must come first: {out:?}");
    assert_eq!(out, wire, "and it round-trips");
}

#[test]
fn a_push_wearing_an_attribute_is_still_a_push() {
    // An attributed invalidation must not be mistaken for a reply and land on stdout.
    let wire = b"|1\r\n$3\r\nsrc\r\n$2\r\nr2\r\n>2\r\n$10\r\ninvalidate\r\n$1\r\nk\r\n";
    let frames = decode_all(wire);
    assert_eq!(frames.len(), 1);
    assert!(is_push(&frames[0].0), "the wrapper must be seen through");

    let e = run(OutputMode::Json, false, wire);
    assert_eq!(e.stdout_text(), "");
    assert_eq!(e.replies(), 0);
    assert_eq!(e.stderr_text(), "1 push frame suppressed\n");
}

// ================================================ --trace-wire
#[test]
fn trace_wire_records_the_real_bytes_including_the_pushes() {
    // `--output resp` normalises; somebody debugging a server needs what actually arrived,
    // RESP2 spellings and all.
    let wire = [PUSH, b"$-1\r\n", PUSH].concat();
    let frames = decode_all(&wire);
    let mut e = Emitter::new(OutputMode::Resp, false).with_wire_trace();
    for (v, raw) in &frames {
        e.accept(&Arrival {
            value: v,
            wire: raw,
        });
    }
    e.finish();

    assert_eq!(
        e.wire_trace().expect("a trace was requested"),
        wire.as_slice(),
        "the trace is the wire, byte for byte"
    );
    assert_eq!(
        e.stdout, b"_\r\n",
        "while stdout is the normalised re-encoding"
    );
    assert_ne!(
        e.wire_trace().unwrap(),
        e.stdout.as_slice(),
        "the two are different artefacts, which is the point of having both"
    );
}

#[test]
fn no_trace_is_kept_unless_it_was_asked_for() {
    let e = run(OutputMode::Resp, false, &push_before());
    assert!(e.wire_trace().is_none());
}

// ================================================ flag resolution
#[test]
fn conflicting_output_flags_are_an_error_not_last_one_wins() {
    // A stray `--json` in a wrapper script must not silently change what a pipeline gets.
    let err = select(&["--json", "--csv"]).unwrap_err();
    assert!(matches!(err, FlagError::Conflict(_)));
    assert!(err.to_string().contains("--json"));
    assert!(err.to_string().contains("--csv"));

    assert!(select(&["--raw", "--output", "ndjson"]).is_err());
    assert!(select(&["--output=json", "--bytes"]).is_err());
    assert!(
        select(&["--json", "--json"]).is_err(),
        "even the same twice"
    );
}

#[test]
fn a_single_flag_resolves_and_no_flag_is_pretty() {
    assert_eq!(select(&[]).unwrap(), OutputMode::Pretty);
    assert_eq!(select(&["--json"]).unwrap(), OutputMode::Json);
    assert_eq!(select(&["--raw"]).unwrap(), OutputMode::Raw);
    assert_eq!(select(&["--output", "ndjson"]).unwrap(), OutputMode::Ndjson);
    assert_eq!(select(&["--output=resp"]).unwrap(), OutputMode::Resp);
    assert_eq!(
        select(&["--output", "typed-json"]).unwrap(),
        OutputMode::TypedJson
    );
    // Unrelated flags are ignored rather than tripping the conflict check.
    assert_eq!(
        select(&["-3", "--no-colour", "--json"]).unwrap(),
        OutputMode::Json
    );
}

#[test]
fn an_unknown_format_is_named_in_the_error() {
    let err = select(&["--output", "yaml"]).unwrap_err();
    assert!(matches!(err, FlagError::UnknownFormat(ref f) if f == "yaml"));
    assert!(err.to_string().contains("yaml"));
}

#[test]
fn json_prefers_resp3_but_never_overrides_an_explicit_choice() {
    // §18.1: `--json` defaults to RESP3 so a map is a map. `-2` is the user saying they want
    // the RESP2 shape, and giving them something else is worse than giving them less.
    assert_eq!(resolve_protocol(OutputMode::Json, None), 3);
    assert_eq!(resolve_protocol(OutputMode::TypedJson, None), 3);
    assert_eq!(resolve_protocol(OutputMode::Resp, None), 3);
    assert_eq!(resolve_protocol(OutputMode::Raw, None), 2);

    assert_eq!(
        resolve_protocol(OutputMode::Json, Some(2)),
        2,
        "an explicit -2 wins over the mode's preference"
    );
    assert_eq!(resolve_protocol(OutputMode::Raw, Some(3)), 3);
}

// ================================================ --bytes
#[test]
fn bytes_emits_the_blob_with_no_delimiter_and_refuses_anything_else() {
    let e = run(OutputMode::Bytes, false, b"$5\r\nhello\r\n");
    assert_eq!(e.stdout, b"hello", "no trailing newline");

    let e = run(OutputMode::Bytes, false, b"*2\r\n:1\r\n:2\r\n");
    assert_eq!(e.stdout, b"", "an array is not a single blob");
    assert!(e.stderr_text().contains("requires a single blob"));
}

#[test]
fn bytes_mode_still_suppresses_pushes() {
    let e = run(OutputMode::Bytes, false, &push_before());
    assert_eq!(e.stdout, b"hello");
    assert_eq!(e.stderr_text(), "1 push frame suppressed\n");
}

// ================================================ the encoder on its own
#[test]
fn encode_resp3_is_total_over_every_value_shape() {
    // Every shape the decoder can produce has to have an encoding, or `--output resp` would
    // have a hole exactly where an unusual server reply lands.
    let shapes: &[&[u8]] = &[
        b"+OK\r\n",
        b"-ERR bad\r\n",
        b":1\r\n",
        b"$0\r\n\r\n",
        b"_\r\n",
        b"*0\r\n",
        b"#f\r\n",
        b",nan\r\n",
        b"(1\r\n",
        b"!3\r\nabc\r\n",
        b"=9\r\nmkd:hello\r\n",
        b"%0\r\n",
        b"~0\r\n",
        b">1\r\n:1\r\n",
        b"|1\r\n:1\r\n:2\r\n:3\r\n",
    ];
    for wire in shapes {
        let frames = decode_all(wire);
        assert_eq!(frames.len(), 1, "fixture {wire:?}");
        let mut out = Vec::new();
        encode_resp3(&frames[0].0, &mut out);
        assert_eq!(
            out,
            *wire,
            "{} did not round-trip",
            String::from_utf8_lossy(wire)
        );
    }
}
