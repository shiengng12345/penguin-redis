//! SPIKE-002 / V-B01 — does `redis-rs`'s low-level API meet §31.2's bar? (v2.1 §31.2, §37.3)
//!
//! §31.2 lists what has to be true before `redis-rs` can be the kernel's protocol layer:
//! the RESP frame types we need are preserved, size and copy behaviour is bounded, push has a
//! budget, a cancelled read leaves the connection in a known state, and redirection and
//! internal retry are ours to control.
//!
//! This file answers that by running `redis-rs` 1.7.0's public low-level API over the same
//! V-A03 corpus `pr-protocol` is held to, and freezing the result. The freezing is the point:
//! when `redis-rs` changes, these numbers change and ADR-031 has to be looked at again, rather
//! than the decision quietly ageing into folklore.
//!
//! `redis-rs` is a **dev-dependency**. `ci/check-redis-rs-is-dev-only.sh` keeps it one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_protocol::decoder::{Decoder, Step};
use pr_protocol::value::{NullForm, Value};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("fixtures/protocol")
}

/// Decode with the product decoder.
fn ours(bytes: &[u8]) -> Result<Step, pr_protocol::decoder::DecodeError> {
    let mut d = Decoder::with_defaults();
    d.feed(bytes);
    d.decode()
}

/// Decode with the only low-level entry point `redis-rs` actually exports.
fn theirs(bytes: &[u8]) -> Result<redis::Value, redis::RedisError> {
    redis::parse_redis_value(bytes)
}

// ---------------------------------------------------------------------------------------------
// §31.2 line 1 — "是否完整保留需要的 RESP frame/type"
// ---------------------------------------------------------------------------------------------

#[test]
fn the_three_null_spellings_collapse_into_one_value() {
    // ADR-005: a view must not change the type. RESP2 `$-1`, RESP2 `*-1` and RESP3 `_` are
    // three different wire facts; `pr-protocol` records which arrived so the frame can be
    // re-encoded and so `--output resp` is faithful.
    for (bytes, form) in [
        (&b"$-1\r\n"[..], NullForm::Bulk),
        (&b"*-1\r\n"[..], NullForm::Array),
        (&b"_\r\n"[..], NullForm::Resp3),
    ] {
        assert_eq!(ours(bytes).unwrap(), Step::Value(Value::Null(form)));
        assert_eq!(theirs(bytes).unwrap(), redis::Value::Nil);
    }
    // And `~-1` too, which is not even legal RESP3 but is accepted as another Nil.
    assert_eq!(theirs(b"~-1\r\n").unwrap(), redis::Value::Nil);
}

#[test]
fn a_double_loses_the_lexeme_the_server_chose() {
    // §18.3: `1.2300` keeps its trailing zeros because they were the server's choice, and
    // §11.7 needs `-0` to stay `-0`. `redis-rs` parses to `f64` at the frame layer, so the
    // lexeme is gone before any caller can see it.
    let cases: &[(&[u8], &str)] = &[(b",1.2300\r\n", "1.2300"), (b",3e2\r\n", "3e2")];
    for (bytes, lexeme) in cases {
        let Step::Value(Value::Double(d)) = ours(bytes).unwrap() else {
            panic!("expected a double");
        };
        assert_eq!(&d[..], lexeme.as_bytes(), "we keep the lexeme");
        let redis::Value::Double(f) = theirs(bytes).unwrap() else {
            panic!("expected a double");
        };
        assert_ne!(
            format!("{f}"),
            *lexeme,
            "if this ever matches, redis-rs started keeping the lexeme and ADR-031 needs a look"
        );
    }
    // `-0` happens to survive, because Rust's `f64` Display prints `-0`. Recorded so the next
    // reader does not think this file is only collecting grievances.
    let redis::Value::Double(f) = theirs(b",-0\r\n").unwrap() else {
        panic!("expected a double");
    };
    assert_eq!(format!("{f}"), "-0");

    // A double too large for `f64` becomes `inf` and is returned as `Ok`. A client that shows
    // the user `inf` where the server said `1e999` has not reported an error — it has reported
    // a different number.
    let redis::Value::Double(f) = theirs(b",1e999\r\n").unwrap() else {
        panic!("expected a double");
    };
    assert!(f.is_infinite(), "1e999 silently became {f}");
    let Step::Value(Value::Double(d)) = ours(b",1e999\r\n").unwrap() else {
        panic!("expected a double");
    };
    assert_eq!(&d[..], b"1e999");

    // Worse than lossy: it is also lenient. `, 1.23 ` is not a legal RESP double, and the
    // `line.trim()` in redis-rs's parser accepts it, so a malformed frame is silently normal.
    assert!(theirs(b", 1.23 \r\n").is_ok());
    assert!(ours(b", 1.23 \r\n").is_err());
}

#[test]
fn a_verbatim_string_is_utf8_lossy_and_never_says_so() {
    // The most serious of the fidelity findings: redis-rs's verbatim path is
    // `String::from_utf8_lossy`, so bytes that are not UTF-8 become U+FFFD and the caller is
    // told nothing. ADR-005 makes the raw bytes authoritative; silently substituting a
    // replacement character is exactly the failure that rule exists to prevent.
    let payload = [0xff_u8, 0xfe]; // not valid UTF-8
    let body = {
        let mut b = b"txt:".to_vec();
        b.extend_from_slice(&payload);
        b
    };
    let frame = {
        let mut f = format!("={}\r\n", body.len()).into_bytes();
        f.extend_from_slice(&body);
        f.extend_from_slice(b"\r\n");
        f
    };

    let Step::Value(Value::Verbatim { format, data }) = ours(&frame).unwrap() else {
        panic!("expected a verbatim string");
    };
    assert_eq!(&format, b"txt");
    assert_eq!(&data[..], &payload, "the bytes arrive intact");

    let redis::Value::VerbatimString { text, .. } = theirs(&frame).unwrap() else {
        panic!("expected a verbatim string");
    };
    assert_eq!(
        text, "\u{fffd}\u{fffd}",
        "redis-rs replaced the payload with U+FFFD and returned Ok"
    );
}

#[test]
fn a_blob_error_is_utf8_lossy_too() {
    // §23.5 wants the server's error bytes shown exactly (inside `SafeText`). The `!` path
    // shares the lossy `blob()` helper, so a non-UTF-8 error message is silently rewritten.
    let mut body = b"ERR ".to_vec();
    body.push(0xff);
    let mut frame = format!("!{}\r\n", body.len()).into_bytes();
    frame.extend_from_slice(&body);
    frame.extend_from_slice(b"\r\n");

    let Step::Value(Value::BulkError(b)) = ours(&frame).unwrap() else {
        panic!("expected a blob error");
    };
    assert_eq!(&b[..], &body[..]);

    let v = theirs(&frame).unwrap();
    assert!(
        format!("{v:?}").contains('\u{fffd}'),
        "expected the lossy replacement, got {v:?}"
    );
}

#[test]
fn a_simple_string_that_is_not_utf8_kills_the_frame() {
    // A hostile or buggy server can put any byte in a `+` line. We keep the bytes; redis-rs
    // requires UTF-8 at the frame layer and fails the whole frame, which at the connection
    // layer means the connection.
    let frame = b"+\xffbad\r\n";
    let Step::Value(Value::Simple(b)) = ours(frame).unwrap() else {
        panic!("expected a simple string");
    };
    assert_eq!(&b[..], b"\xffbad");
    assert!(theirs(frame).is_err());
}

#[test]
fn plus_ok_stops_being_a_simple_string() {
    // `+OK` becomes `Value::Okay`, a variant of its own. Harmless on its own; it matters
    // because it means `redis-rs`'s frame layer is already interpreting, and a projection that
    // re-encodes has to know to special-case it.
    assert_eq!(theirs(b"+OK\r\n").unwrap(), redis::Value::Okay);
    assert_eq!(
        theirs(b"+PONG\r\n").unwrap(),
        redis::Value::SimpleString("PONG".into())
    );
    let Step::Value(Value::Simple(b)) = ours(b"+OK\r\n").unwrap() else {
        panic!("expected a simple string");
    };
    assert_eq!(&b[..], b"OK");
}

#[test]
fn an_empty_push_is_fabricated_rather_than_refused() {
    // `>0` and `>-1` are not valid push frames. redis-rs invents
    // `Push { kind: Other(""), data: [] }` for both, so a caller cannot tell a malformed push
    // from a real one — and a push is the one frame that must never be mistaken for a reply.
    for bytes in [&b">0\r\n"[..], &b">-1\r\n"[..]] {
        let v = theirs(bytes).unwrap();
        assert!(
            matches!(&v, redis::Value::Push { data, .. } if data.is_empty()),
            "got {v:?}"
        );
    }
}

#[test]
fn any_negative_length_becomes_a_null_not_an_error() {
    // The over-acceptance that matters. RESP defines exactly one negative length: `-1`.
    // redis-rs's parser tests `if size < 0` and produces `Nil`, so `$-5`, `*-5` and `~-5` —
    // none of which a conforming server can send — arrive at the application as an ordinary
    // null. A protocol violation that looks like a value is worse than one that looks like an
    // error, because nothing upstream ever finds out.
    for bytes in [&b"$-5\r\n"[..], &b"*-5\r\n"[..], &b"~-5\r\n"[..]] {
        assert!(
            matches!(
                ours(bytes),
                Err(pr_protocol::decoder::DecodeError::Protocol(_))
            ),
            "we reject {:?}",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(
            theirs(bytes).unwrap(),
            redis::Value::Nil,
            "redis-rs turned {:?} into a null",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn numbers_are_accepted_in_spellings_resp_does_not_define() {
    // `:+1` — RESP integers have no sign prefix; `line.trim().parse::<i64>()` takes one anyway.
    assert_eq!(theirs(b":+1\r\n").unwrap(), redis::Value::Int(1));
    assert!(ours(b":+1\r\n").is_err());

    // `(12a` — without the `num-bigint` feature the big-number path stores the line verbatim
    // with no validation, so a malformed big number is discovered by whichever caller first
    // tries to use it, arbitrarily far from the wire that produced it.
    assert_eq!(
        theirs(b"(12a\r\n").unwrap(),
        redis::Value::BigNumber(b"12a".to_vec())
    );
    assert!(ours(b"(12a\r\n").is_err());
}

#[test]
fn the_nesting_limit_is_redis_rs_s_and_not_ours() {
    // Not a defect — a difference in ownership. §24.3 makes the decode budget ours to set per
    // connection; redis-rs hard-codes `MAX_RECURSE_DEPTH = 100` with no way to change it, so a
    // deployment that wants 16 or 512 cannot have it.
    let deep = std::fs::read(fixtures().join("deep-nesting/array-64.resp")).unwrap();
    assert_eq!(
        ours(&deep),
        Err(pr_protocol::decoder::DecodeError::Budget("depth")),
        "our default is 64 and it is configurable"
    );
    assert!(theirs(&deep).is_ok(), "redis-rs's fixed limit is 100");
}

#[test]
fn resp3_streamed_types_are_not_implemented_at_all() {
    // R16: `$?`/`;N`/`;0` and `*?`/`%?`/`~?`/`.`. They are absent from redis-rs's dispatch
    // table, so every one of them is a parse error. This is the item §31.2 cares about most,
    // because a streamed type is the wire's own way of saying "this does not fit in memory".
    let streamed: &[&[u8]] = &[
        b"$?\r\n;5\r\nhello\r\n;0\r\n",
        b"*?\r\n:1\r\n.\r\n",
        b"%?\r\n+a\r\n:1\r\n.\r\n",
        b"~?\r\n:1\r\n.\r\n",
    ];
    for bytes in streamed {
        assert!(
            matches!(ours(bytes), Ok(Step::Value(_))),
            "we decode {:?}",
            String::from_utf8_lossy(bytes)
        );
        assert!(
            theirs(bytes).is_err(),
            "redis-rs decoded {:?} — ADR-031 needs revisiting",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn the_things_redis_rs_gets_right_are_recorded_too() {
    // A spike that only lists losses is not a comparison. These are preserved faithfully, and
    // recording them is what makes the conclusion about the rest credible.

    // Map order and duplicate keys survive — it is a `Vec<(Value, Value)>`, not a HashMap.
    let dup = b"%2\r\n+a\r\n:1\r\n+a\r\n:2\r\n";
    let redis::Value::Map(entries) = theirs(dup).unwrap() else {
        panic!("expected a map");
    };
    assert_eq!(entries.len(), 2, "both entries kept");

    // A big number stays as digits rather than becoming a float.
    assert_eq!(
        theirs(b"(3492890328409238509324850943850943825024385\r\n").unwrap(),
        redis::Value::BigNumber(b"3492890328409238509324850943850943825024385".to_vec())
    );

    // Attributes survive at this layer (the typed layer above discards them).
    let attr = b"|1\r\n+k\r\n+v\r\n:1\r\n";
    assert!(matches!(
        theirs(attr).unwrap(),
        redis::Value::Attribute { .. }
    ));

    // And bulk strings are binary safe, NUL included.
    assert_eq!(
        theirs(b"$3\r\na\x00b\r\n").unwrap(),
        redis::Value::BulkString(b"a\x00b".to_vec())
    );
}

// ---------------------------------------------------------------------------------------------
// §31.2 line 2 — "大小和复制行为"
// ---------------------------------------------------------------------------------------------

#[test]
fn every_bulk_string_is_copied_out_of_the_read_buffer() {
    // `Value::BulkString(Vec<u8>)` is an owned copy (`bs.to_vec()` in redis-rs's parser), so a
    // 512 MB value costs the read buffer *plus* the value, and every clone costs another copy.
    // `pr-protocol` hands out `Bytes`, a refcounted window on the buffer that already exists —
    // §24.7's budget is only reachable if the value does not double.
    let mut frame = b"$65536\r\n".to_vec();
    frame.extend(std::iter::repeat_n(b'x', 65536));
    frame.extend_from_slice(b"\r\n");

    let Step::Value(Value::Bulk(b)) = ours(&frame).unwrap() else {
        panic!("expected a bulk string");
    };
    let clone = b.clone();
    assert_eq!(
        b.as_ptr(),
        clone.as_ptr(),
        "our clone shares the allocation"
    );

    let redis::Value::BulkString(v) = theirs(&frame).unwrap() else {
        panic!("expected a bulk string");
    };
    let clone = v.clone();
    assert_ne!(
        v.as_ptr(),
        clone.as_ptr(),
        "redis-rs's clone copies 64 KiB — if this ever fails, redis-rs changed representation"
    );
}

#[test]
fn an_oversized_declared_length_cannot_be_refused_it_can_only_be_awaited() {
    // §24.4 draws the distinction that matters operationally: a *budget* error means the
    // stream is fine and we refuse to materialise it (close cleanly, tell the user a limit was
    // hit); a *protocol* error means the byte boundary is unknowable (kill the connection).
    //
    // redis-rs has neither a budget nor a way to express one. A server declaring a 1 GiB bulk
    // is indistinguishable from a 1 GiB bulk that has not finished arriving, so the only
    // available behaviour is to keep reading.
    let declared = b"$1073741824\r\nxxxx";
    assert_eq!(
        ours(declared),
        Err(pr_protocol::decoder::DecodeError::Budget("bulk")),
        "we refuse it by name"
    );
    let e = theirs(declared).unwrap_err();
    assert!(
        format!("{e:?}").to_lowercase().contains("eof")
            || format!("{e:?}").to_lowercase().contains("unexpected"),
        "redis-rs reports 'not enough bytes yet', not 'too large': {e:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// §31.2 line 3 — "push channel 预算"
// ---------------------------------------------------------------------------------------------

#[test]
fn resp2_pubsub_is_indistinguishable_from_a_reply_at_this_layer() {
    // In RESP2 a pub/sub delivery is an ordinary `*3` array. `pr-protocol` reports the frame
    // as it arrived and leaves routing to a layer that knows the connection's subscribe state;
    // redis-rs's low-level API does the same — so neither can demux RESP2 alone, and that is
    // fine. What matters is the RESP3 case below.
    let msg = b"*3\r\n$7\r\nmessage\r\n$2\r\nch\r\n$2\r\nhi\r\n";
    assert!(matches!(theirs(msg).unwrap(), redis::Value::Array(_)));
    assert!(matches!(ours(msg).unwrap(), Step::Value(Value::Array(_))));
}

#[test]
fn a_resp3_push_is_a_value_in_the_reply_slot_with_no_budget_attached() {
    // `parse_redis_value` returns the push as the frame it just parsed. There is no bound on
    // how many arrive between two replies and no public API to attach one: `ValueCodec`, the
    // type that would let a caller drive the decode and count, lives in a private module
    // (`redis-1.7.0/src/parser.rs`, `mod aio_support`) and `lib.rs` re-exports only
    // `{Parser, parse_redis_value}`. §24.3's push budget has nowhere to live.
    let push = b">2\r\n$7\r\nmessage\r\n$2\r\nhi\r\n";
    assert!(matches!(theirs(push).unwrap(), redis::Value::Push { .. }));
    let Step::Value(v) = ours(push).unwrap() else {
        panic!("expected a value");
    };
    assert!(v.is_push(), "we mark it out-of-band");
}

// ---------------------------------------------------------------------------------------------
// §31.2 lines 4 and 5 — "取消后读取行为" and "连接状态"
// ---------------------------------------------------------------------------------------------

/// A reader that hands out one chunk and then reports that more will come later — exactly what
/// a non-blocking socket does, and what a cancelled-then-resumed read has to cope with.
struct WouldBlockAfter<'a> {
    chunk: &'a [u8],
    done: bool,
}

impl std::io::Read for WouldBlockAfter<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.done {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "come back later",
            ));
        }
        self.done = true;
        let n = self.chunk.len().min(buf.len());
        buf[..n].copy_from_slice(&self.chunk[..n]);
        Ok(n)
    }
}

#[test]
fn a_reader_that_is_not_ready_yet_is_reported_as_a_failed_frame() {
    // `Parser::parse_value` takes a *blocking* `std::io::Read`. Handed a reader that is not
    // ready, it does not return "incomplete" — it returns an error, and the bytes it consumed
    // are inside the parser's private partial state. There is no way to ask how many bytes it
    // took from the socket, so the connection cannot be resumed and cannot be safely reused:
    // the next read would start mid-frame. That is precisely the state ADR-007 calls
    // `UnknownAfterSend`, and here it would be produced by an ordinary slow network.
    let mut parser = redis::Parser::new();
    let mut reader = WouldBlockAfter {
        chunk: b"$5\r\nhel",
        done: false,
    };
    let err = parser
        .parse_value(&mut reader)
        .expect_err("a partial frame cannot succeed");
    assert!(format!("{err:?}").len() > 1);

    // The same bytes, to the product decoder, are simply "not yet".
    let mut d = Decoder::with_defaults();
    d.feed(b"$5\r\nhel");
    assert_eq!(d.decode().unwrap(), Step::Incomplete);
    d.feed(b"lo\r\n");
    let Step::Value(Value::Bulk(b)) = d.decode().unwrap() else {
        panic!("expected a bulk string");
    };
    assert_eq!(&b[..], b"hello");
}

#[test]
fn truncation_is_distinguished_from_malformation_but_the_consumed_count_is_not_reported() {
    // Half a win, and the half that is missing is the expensive one.
    //
    // `redis-rs` *does* separate the two: a truncated frame surfaces as an IO-kind error and a
    // malformed one as a parse-kind error, so "read more" and "hang up" are distinguishable.
    // That is the thing this test originally expected it to get wrong, and it does not.
    assert_eq!(ours(b"$5\r\nhel").unwrap(), Step::Incomplete);
    assert!(matches!(
        ours(b"$x\r\n"),
        Err(pr_protocol::decoder::DecodeError::Protocol(_))
    ));
    assert_ne!(
        theirs(b"$5\r\nhel").unwrap_err().kind(),
        theirs(b"$x\r\n").unwrap_err().kind(),
        "if these ever converge, redis-rs lost the distinction and ADR-031 needs revisiting"
    );

    // What is missing is the byte count. `parse_redis_value` returns a value and nothing else,
    // so given a buffer holding two frames it hands back the first and leaves the caller with
    // no way to find where the second begins — short of re-implementing the framing it just
    // did. Pipelining, `--pipe`, MULTI/EXEC and a push arriving beside a reply all put more
    // than one frame in the buffer, so this is the normal case, not the exotic one.
    let two = b"+a\r\n+b\r\n";
    assert_eq!(
        theirs(two).unwrap(),
        redis::Value::SimpleString("a".into()),
        "the second frame is silently dropped on the floor"
    );

    let mut d = Decoder::with_defaults();
    d.feed(two);
    let before = d.buffered();
    assert!(matches!(d.decode(), Ok(Step::Value(_))));
    assert_eq!(
        d.buffered(),
        before - 4,
        "we consume exactly the first frame and keep the rest"
    );
    assert!(
        matches!(d.decode(), Ok(Step::Value(_))),
        "and then the second"
    );
}

// ---------------------------------------------------------------------------------------------
// The V-A03 corpus, end to end
// ---------------------------------------------------------------------------------------------

#[derive(Default, Debug, PartialEq, Eq)]
struct Tally {
    /// Both decoders reached the same verdict and nothing was lost.
    agree: usize,
    /// Both accepted it, but `redis-rs`'s value no longer carries what the wire said.
    lossy: usize,
    /// We decode it; `redis-rs` cannot. Every one of these is a RESP3 streamed type.
    not_implemented: usize,
    /// `redis-rs` accepts a frame we call a **protocol error**. The security-relevant column:
    /// a protocol violation arrives at the application looking like an ordinary value.
    over_accepted: usize,
    /// `redis-rs` accepts a frame we refuse on **budget** grounds. Not a defect in `redis-rs` —
    /// it simply has a different, hard-coded limit. Counted apart so it is not read as one.
    beyond_our_budget: usize,
}

#[test]
fn the_whole_v_a03_corpus_run_through_both_decoders() {
    // "对 V-A03 的全部帧类别用 redis-rs 低层 API 读取，比对保真" — the whole corpus, both
    // decoders, one table. The numbers are frozen: when `redis-rs` changes, this test fails
    // and ADR-031 has to be re-read rather than assumed.
    let root = fixtures();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let samples = manifest["samples"].as_array().unwrap();
    assert_eq!(
        samples.len(),
        316,
        "the corpus changed size; re-freeze the tally below"
    );

    let mut t = Tally::default();
    let mut detail: Vec<String> = Vec::new();
    for s in samples {
        let file = s["file"].as_str().unwrap();
        let bytes = std::fs::read(root.join(file)).unwrap();
        let us = ours(&bytes);
        let we_accept = matches!(us, Ok(Step::Value(_)));
        match (we_accept, theirs(&bytes)) {
            (true, Ok(v)) => {
                if lossless(&bytes, &v) {
                    t.agree += 1;
                } else {
                    t.lossy += 1;
                    detail.push(format!("lossy            {file}"));
                }
            }
            (true, Err(_)) => {
                t.not_implemented += 1;
                detail.push(format!("not-implemented  {file}"));
            }
            (false, Ok(_)) => {
                if matches!(us, Err(pr_protocol::decoder::DecodeError::Budget(_))) {
                    t.beyond_our_budget += 1;
                    detail.push(format!("beyond-budget    {file}"));
                } else {
                    t.over_accepted += 1;
                    detail.push(format!("over-accepted    {file}"));
                }
            }
            (false, Err(_)) => t.agree += 1,
        }
    }

    for d in &detail {
        eprintln!("  {d}");
    }
    eprintln!("SPIKE-002 corpus tally: {t:?}");

    assert_eq!(
        t,
        Tally {
            agree: 271,
            lossy: 10,
            not_implemented: 28,
            over_accepted: 5,
            beyond_our_budget: 2,
        },
        "the comparison moved; update docs/spikes/SPIKE-002.md and re-read ADR-031"
    );
    assert_eq!(
        t.agree + t.lossy + t.not_implemented + t.over_accepted + t.beyond_our_budget,
        samples.len()
    );

    // Every frame `redis-rs` cannot read is a RESP3 streamed type, and it is all of them.
    assert_eq!(
        t.not_implemented,
        detail
            .iter()
            .filter(|d| d.starts_with("not-implemented  streamed/"))
            .count(),
        "something other than a streamed type became unreadable"
    );
}

/// Whether `redis-rs`'s value still carries everything the wire said.
///
/// Deliberately narrow: it checks only the losses this spike found and names them, because a
/// projection that claims to check "everything" and does not is worse than one that says what
/// it covers. `Value::Okay` is **not** counted as a loss — `+OK` is recoverable from it; it is
/// a change of shape, recorded separately in `plus_ok_stops_being_a_simple_string`.
fn lossless(bytes: &[u8], v: &redis::Value) -> bool {
    match v {
        // `Nil`: which of `$-1` / `*-1` / `_` arrived is gone. `Double`: the lexeme is gone.
        redis::Value::Nil | redis::Value::Double(_) => false,
        redis::Value::VerbatimString { text, .. } => !text.contains('\u{fffd}'),
        // A `>1` push is a special case rather than a loss: redis-rs consumes the single
        // element as the push *kind*, so `data` is legitimately empty. Any other empty `data`
        // means redis-rs fabricated a push out of `>0` or `>-1`.
        redis::Value::Push { data, .. } => {
            !data.is_empty() || bytes.windows(4).any(|w| w == b">1\r\n")
        }
        redis::Value::Array(items) | redis::Value::Set(items) => {
            items.iter().all(|i| lossless(bytes, i))
        }
        redis::Value::Map(kv) => kv
            .iter()
            .all(|(k, val)| lossless(bytes, k) && lossless(bytes, val)),
        redis::Value::Attribute { data, attributes } => {
            lossless(bytes, data)
                && attributes
                    .iter()
                    .all(|(k, val)| lossless(bytes, k) && lossless(bytes, val))
        }
        redis::Value::ServerError(_) => !bytes.starts_with(b"!"),
        _ => true,
    }
}
