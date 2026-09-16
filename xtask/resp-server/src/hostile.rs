//! Hostile / malformed corpus generator covering v2.1 §32.3 categories.
//!
//! Each category yields ≥ 20 parametric variants (V-A03 pass criterion). Every entry is a
//! **raw byte sequence** the client decoder must survive without panic, unbounded allocation,
//! infinite loop, terminal escape leakage, or framing loss.

use crate::encode::{Proto, to_vec};
use crate::frame::Frame;
use bytes::Bytes;

/// A corpus entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Category slug (directory name under `fixtures/protocol/`).
    pub category: &'static str,
    /// Unique name within the category.
    pub name: String,
    /// Raw bytes to send.
    pub bytes: Vec<u8>,
    /// What the decoder is expected to do.
    pub expectation: Expectation,
}

/// Expected decoder behaviour for a sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expectation {
    /// Must be rejected as a protocol error (connection isolated/closed), never panic.
    ProtocolError,
    /// Valid frame(s) the decoder must accept losslessly.
    Valid,
    /// Valid but exceeds a configured budget → must stop with a budget outcome, not OOM.
    BudgetExceeded,
    /// Incomplete: decoder must wait for more bytes (or time out), not misparse.
    Incomplete,
}

fn s(category: &'static str, name: impl Into<String>, bytes: Vec<u8>, expectation: Expectation) -> Sample {
    Sample { category, name: name.into(), bytes, expectation }
}

fn r3(f: &Frame) -> Vec<u8> {
    // Every frame we construct here is RESP3-encodable; if not, the fixture is a bug and we
    // emit an empty sample rather than panic (deny(panic) lint).
    to_vec(f, Proto::Resp3).unwrap_or_default()
}

/// `malformed-length`: non-numeric, negative (other than -1), huge, overflowing lengths.
fn malformed_length() -> Vec<Sample> {
    let c = "malformed-length";
    let mut v = Vec::new();
    let bad: [&[u8]; 8] = [b"abc", b"-5", b"1.5", b"+3", b" 3", b"3 ", b"", b"0x10"];
    for (i, b) in bad.iter().enumerate() {
        for (j, p) in b"$*%~!=".iter().enumerate() {
            let mut bytes = vec![*p];
            bytes.extend_from_slice(b);
            bytes.extend_from_slice(b"\r\n");
            v.push(s(c, format!("{}-{}-{}", p.escape_ascii(), i, j), bytes, Expectation::ProtocolError));
        }
    }
    for (i, big) in ["99999999999999999999", "18446744073709551616", "9223372036854775808"].iter().enumerate() {
        v.push(s(c, format!("overflow-{i}"), format!("${big}\r\n").into_bytes(), Expectation::ProtocolError));
        v.push(s(c, format!("overflow-int-{i}"), format!(":{big}\r\n").into_bytes(), Expectation::ProtocolError));
    }
    v
}

/// `oversized-declared`: declared length far beyond budget, little or nothing sent.
fn oversized_declared() -> Vec<Sample> {
    let c = "oversized-declared";
    let mut v = Vec::new();
    for (i, len) in [1u64 << 20, 1 << 24, 1 << 28, 1 << 30, 1 << 32, 1 << 40, 1 << 62].iter().enumerate() {
        v.push(s(c, format!("bulk-{i}"), format!("${len}\r\n").into_bytes(), Expectation::BudgetExceeded));
        v.push(s(c, format!("array-{i}"), format!("*{len}\r\n").into_bytes(), Expectation::BudgetExceeded));
        v.push(s(c, format!("map-{i}"), format!("%{len}\r\n").into_bytes(), Expectation::BudgetExceeded));
        // declared big, then a small body and close → truncated
        let mut b = format!("${len}\r\n").into_bytes();
        b.extend_from_slice(b"short");
        v.push(s(c, format!("bulk-truncated-{i}"), b, Expectation::Incomplete));
    }
    v
}

/// `deep-nesting`: arrays nested N deep.
fn deep_nesting() -> Vec<Sample> {
    let c = "deep-nesting";
    let mut v = Vec::new();
    for depth in [16usize, 32, 64, 128, 256, 512, 1024, 4096, 16384, 65536] {
        let mut b = Vec::with_capacity(depth * 4 + 4);
        for _ in 0..depth {
            b.extend_from_slice(b"*1\r\n");
        }
        b.extend_from_slice(b":1\r\n");
        let exp = if depth <= 64 { Expectation::Valid } else { Expectation::BudgetExceeded };
        v.push(s(c, format!("array-{depth}"), b, exp));
        let mut m = Vec::with_capacity(depth * 8);
        for _ in 0..depth {
            m.extend_from_slice(b"%1\r\n+k\r\n");
        }
        m.extend_from_slice(b":1\r\n");
        v.push(s(c, format!("map-{depth}"), m, exp));
    }
    v
}

/// `invalid-type`: unknown leading type bytes.
fn invalid_type() -> Vec<Sample> {
    let c = "invalid-type";
    let mut v = Vec::new();
    for &b in b"@&aZ0\0\x1b \n/\\'\"[{<?`\x7f\xff\xc3\xe2" {
        let mut bytes = vec![b];
        bytes.extend_from_slice(b"OK\r\n");
        v.push(s(c, format!("byte-{b:02x}"), bytes, Expectation::ProtocolError));
    }
    v
}

/// `truncated-crlf`: LF-only, CR-only, missing terminator, terminator inside length.
fn truncated_crlf() -> Vec<Sample> {
    let c = "truncated-crlf";
    let mut v = Vec::new();
    let cases: [(&str, &[u8], Expectation); 12] = [
        ("simple-lf", b"+OK\n", Expectation::ProtocolError),
        ("simple-cr", b"+OK\r", Expectation::Incomplete),
        ("simple-none", b"+OK", Expectation::Incomplete),
        ("int-lf", b":1\n", Expectation::ProtocolError),
        ("bulk-len-lf", b"$3\nfoo\r\n", Expectation::ProtocolError),
        ("bulk-body-lf", b"$3\r\nfoo\n", Expectation::ProtocolError),
        ("bulk-body-none", b"$3\r\nfoo", Expectation::Incomplete),
        ("bulk-body-cr", b"$3\r\nfoo\r", Expectation::Incomplete),
        ("bulk-body-wrong-len", b"$3\r\nfoobar\r\n", Expectation::ProtocolError),
        ("array-partial", b"*2\r\n:1\r\n", Expectation::Incomplete),
        ("map-odd", b"%1\r\n+k\r\n", Expectation::Incomplete),
        ("null-lf", b"_\n", Expectation::ProtocolError),
    ];
    for (n, b, e) in cases {
        v.push(s(c, n, b.to_vec(), e));
    }
    // CRLF inside a bulk body is legal (binary-safe) — must be Valid
    for i in 1..=8usize {
        let body = "\r\n".repeat(i);
        v.push(s(c, format!("crlf-in-body-{i}"), r3(&Frame::bulk(body.as_bytes())), Expectation::Valid));
    }
    v
}

/// `utf8-boundary`: multi-byte sequences (valid + invalid) in bulk strings; chunk splits are
/// applied by the server script (`SplitAt`), this corpus supplies the bodies.
fn utf8_boundary() -> Vec<Sample> {
    let c = "utf8-boundary";
    let mut v = Vec::new();
    let valid: [&[u8]; 8] = [
        "héllo".as_bytes(),
        "中文字段".as_bytes(),
        "🐧".as_bytes(),
        "👨\u{200d}👩\u{200d}👧".as_bytes(),
        "e\u{301}".as_bytes(),
        "\u{feff}bom".as_bytes(),
        "\u{202e}rtl".as_bytes(),
        "a\u{0}b".as_bytes(),
    ];
    for (i, b) in valid.iter().enumerate() {
        v.push(s(c, format!("valid-{i}"), r3(&Frame::bulk(b)), Expectation::Valid));
    }
    let invalid: [&[u8]; 12] = [
        b"\xff", b"\xc3", b"\xc3\x28", b"\xe2\x82", b"\xf0\x9f\x90", b"\xed\xa0\x80", // lone surrogate
        b"\xc0\xaf", // overlong
        b"\xf8\x88\x80\x80\x80", b"\x80", b"abc\xffdef", b"\xef\xbf\xbe", b"\xf4\x90\x80\x80",
    ];
    for (i, b) in invalid.iter().enumerate() {
        v.push(s(c, format!("invalid-{i}"), r3(&Frame::bulk(b)), Expectation::Valid)); // binary-safe → Valid frame
    }
    v
}

/// `terminal-escape`: control sequences inside bulk strings; frame is Valid, renderer must escape.
fn terminal_escape() -> Vec<Sample> {
    let c = "terminal-escape";
    let mut v = Vec::new();
    let payloads: [&[u8]; 20] = [
        b"\x1b]0;evil title\x07",
        b"\x1b]52;c;ZXZpbA==\x07",           // OSC52 clipboard
        b"\x1b]8;;http://evil\x07link\x1b]8;;\x07",
        b"\x1b[2J",
        b"\x1b[H",
        b"\x1b[?1049h",
        b"\x1b[31mred\x1b[0m",
        b"\x1bc",
        b"\x1bP+q\x1b\\",                     // DCS
        b"\x1b_APC\x1b\\",
        b"\x9b2J",                            // C1 CSI
        b"\x9d0;t\x9c",                       // C1 OSC/ST
        b"\r\x1b[Kfake prompt> ",
        b"\x07\x07\x07",
        b"\x08\x08\x08",
        b"\x0c",
        b"\x1b[6n",                           // cursor report request
        b"\x1b[>c",
        b"\x1b[?2004h",
        b"\x1b[200~pasted\x1b[201~",          // fake bracketed paste
    ];
    for (i, p) in payloads.iter().enumerate() {
        v.push(s(c, format!("bulk-{i:02}"), r3(&Frame::bulk(p)), Expectation::Valid));
    }
    // also as simple string / error / verbatim
    v.push(s(c, "simple", r3(&Frame::simple("\x1b]0;x\x07")), Expectation::Valid));
    v.push(s(c, "error", r3(&Frame::error("ERR \x1b[2J")), Expectation::Valid));
    v
}

/// `duplicate-attribute` & attribute edge cases.
fn attributes() -> Vec<Sample> {
    let c = "attribute";
    let mut v = Vec::new();
    for n in 0..=20usize {
        let attrs: Vec<(Frame, Frame)> = (0..n).map(|_| (Frame::simple("k"), Frame::Integer(1))).collect();
        v.push(s(c, format!("dup-{n:02}"), r3(&Frame::Attribute { attrs, value: Box::new(Frame::simple("OK")) }), Expectation::Valid));
    }
    // attribute wrapping attribute
    let inner = Frame::Attribute { attrs: vec![(Frame::simple("a"), Frame::Integer(1))], value: Box::new(Frame::Integer(7)) };
    v.push(s(c, "nested", r3(&Frame::Attribute { attrs: vec![], value: Box::new(inner) }), Expectation::Valid));
    // attribute with no value following (truncated)
    v.push(s(c, "no-value", b"|1\r\n+k\r\n:1\r\n".to_vec(), Expectation::Incomplete));
    v
}

/// `overlong-number`: numbers at/over i64 and doubles with odd lexemes.
fn overlong_number() -> Vec<Sample> {
    let c = "overlong-number";
    let mut v = Vec::new();
    let ints = ["9223372036854775807", "-9223372036854775808"];
    for (i, n) in ints.iter().enumerate() {
        v.push(s(c, format!("int-edge-{i}"), format!(":{n}\r\n").into_bytes(), Expectation::Valid));
    }
    let bad_ints = ["9223372036854775808", "-9223372036854775809", "1e3", "0x1", "١٢٣", "1_000", "+1", "--1", "1-", ""];
    for (i, n) in bad_ints.iter().enumerate() {
        v.push(s(c, format!("int-bad-{i}"), format!(":{n}\r\n").into_bytes(), Expectation::ProtocolError));
    }
    let doubles = ["1.2300", "-0", "0", "1e3", "1E-3", "inf", "-inf", "nan", "3.14159265358979323846264338327950288", "9007199254740993"];
    for (i, d) in doubles.iter().enumerate() {
        v.push(s(c, format!("double-{i}"), format!(",{d}\r\n").into_bytes(), Expectation::Valid));
    }
    for (i, d) in ["", "abc", "1..2", "--1", "1e", "0x10"].iter().enumerate() {
        v.push(s(c, format!("double-bad-{i}"), format!(",{d}\r\n").into_bytes(), Expectation::ProtocolError));
    }
    v.push(s(c, "bignum-huge", format!("({}\r\n", "9".repeat(4096)).into_bytes(), Expectation::Valid));
    v.push(s(c, "bignum-bad", b"(12a\r\n".to_vec(), Expectation::ProtocolError));
    v
}

/// `binary-nul`: NUL and full byte range inside bulk strings.
fn binary_nul() -> Vec<Sample> {
    let c = "binary-nul";
    let mut v = Vec::new();
    for i in 0..20usize {
        let mut body = vec![0u8; i + 1];
        for (j, b) in body.iter_mut().enumerate() {
            *b = u8::try_from((i * 13 + j * 7) % 256).unwrap_or(0);
        }
        body.push(0);
        v.push(s(c, format!("mixed-{i:02}"), r3(&Frame::bulk(&body)), Expectation::Valid));
    }
    let all: Vec<u8> = (0..=255u8).collect();
    v.push(s(c, "all-bytes", r3(&Frame::bulk(&all)), Expectation::Valid));
    v.push(s(c, "empty", r3(&Frame::bulk(b"")), Expectation::Valid));
    v
}

/// `streamed`: RESP3 streamed types incl. edge cases (R16).
fn streamed() -> Vec<Sample> {
    let c = "streamed";
    let mut v = Vec::new();
    for n in 0..=12usize {
        let chunks: Vec<Bytes> = (0..n).map(|i| Bytes::from(vec![b'a' + u8::try_from(i % 26).unwrap_or(0); i + 1])).collect();
        v.push(s(c, format!("bulk-{n:02}chunks"), r3(&Frame::StreamedBulk(chunks)), Expectation::Valid));
    }
    for n in [0i64, 1, 2, 5, 50] {
        v.push(s(c, format!("array-{n}"), r3(&Frame::StreamedArray((0..n).map(Frame::Integer).collect())), Expectation::Valid));
        v.push(s(c, format!("map-{n}"), r3(&Frame::StreamedMap((0..n).map(|i| (Frame::simple("k"), Frame::Integer(i))).collect())), Expectation::Valid));
        v.push(s(c, format!("set-{n}"), r3(&Frame::StreamedSet((0..n).map(Frame::Integer).collect())), Expectation::Valid));
    }
    // malformed streamed
    v.push(s(c, "bulk-no-terminator", b"$?\r\n;3\r\nabc\r\n".to_vec(), Expectation::Incomplete));
    v.push(s(c, "bulk-bad-chunk-len", b"$?\r\n;x\r\n".to_vec(), Expectation::ProtocolError));
    v.push(s(c, "array-no-terminator", b"*?\r\n:1\r\n".to_vec(), Expectation::Incomplete));
    v.push(s(c, "bad-terminator", b"*?\r\n:1\r\n,\r\n".to_vec(), Expectation::ProtocolError));
    v
}

/// `push`: push frames interleaved with replies (R17).
fn push() -> Vec<Sample> {
    let c = "push";
    let mut v = Vec::new();
    let reply = r3(&Frame::simple("OK"));
    for n in 0..=20usize {
        let mut b = Vec::new();
        for i in 0..n {
            b.extend_from_slice(&r3(&Frame::Push(vec![Frame::simple("message"), Frame::bulk(b"ch"), Frame::bulk(format!("m{i}").as_bytes())])));
        }
        b.extend_from_slice(&reply);
        v.push(s(c, format!("before-reply-{n:02}"), b, Expectation::Valid));
    }
    let mut after = reply.clone();
    after.extend_from_slice(&r3(&Frame::Push(vec![Frame::simple("invalidate"), Frame::Array(vec![Frame::bulk(b"k")])])));
    v.push(s(c, "after-reply", after, Expectation::Valid));
    v.push(s(c, "push-nested-in-array", b"*1\r\n>1\r\n+x\r\n".to_vec(), Expectation::ProtocolError));
    v
}

/// The whole corpus.
#[must_use]
pub fn corpus() -> Vec<Sample> {
    let mut all = Vec::new();
    all.extend(malformed_length());
    all.extend(oversized_declared());
    all.extend(deep_nesting());
    all.extend(invalid_type());
    all.extend(truncated_crlf());
    all.extend(utf8_boundary());
    all.extend(terminal_escape());
    all.extend(attributes());
    all.extend(overlong_number());
    all.extend(binary_nul());
    all.extend(streamed());
    all.extend(push());
    all
}

/// Category names in the corpus.
#[must_use]
pub fn categories() -> Vec<&'static str> {
    let mut c: Vec<&'static str> = corpus().iter().map(|s| s.category).collect();
    c.sort_unstable();
    c.dedup();
    c
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn every_category_has_at_least_twenty_samples() {
        let mut count: HashMap<&str, usize> = HashMap::new();
        for smp in corpus() {
            *count.entry(smp.category).or_default() += 1;
        }
        assert_eq!(count.len(), 12, "categories: {:?}", count.keys());
        for (cat, n) in &count {
            assert!(*n >= 20, "category {cat} has only {n} samples");
        }
    }

    #[test]
    fn names_are_unique_within_category() {
        let mut seen = std::collections::HashSet::new();
        for smp in corpus() {
            assert!(seen.insert((smp.category, smp.name.clone())), "dup {}/{}", smp.category, smp.name);
        }
    }

    #[test]
    fn no_sample_is_empty() {
        for smp in corpus() {
            assert!(!smp.bytes.is_empty(), "{}/{} empty", smp.category, smp.name);
        }
    }
}
