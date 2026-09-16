//! Wire encoders: RESP3 (full) and RESP2 (with defined down-conversion or refusal).

use crate::frame::Frame;
use bytes::Bytes;
use thiserror::Error;

/// Encoding protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    /// RESP2 — RESP3-only types are down-converted where Redis itself does so; otherwise refused.
    Resp2,
    /// RESP3 — every variant encodes natively.
    Resp3,
}

/// Errors from encoding.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    /// A RESP3-only frame that has no RESP2 wire form (attribute, streamed types).
    #[error("frame {0} has no RESP2 representation")]
    NoResp2Form(&'static str),
    /// A frame that cannot be expressed on the wire at all, in any protocol.
    #[error("frame cannot be encoded: {0}")]
    UnrepresentableFrame(&'static str),
}

const CRLF: &[u8] = b"\r\n";

fn put_len(out: &mut Vec<u8>, prefix: u8, len: usize) {
    out.push(prefix);
    out.extend_from_slice(len.to_string().as_bytes());
    out.extend_from_slice(CRLF);
}

fn put_blob(out: &mut Vec<u8>, prefix: u8, b: &Bytes) {
    put_len(out, prefix, b.len());
    out.extend_from_slice(b);
    out.extend_from_slice(CRLF);
}

fn encode_scalar(f: &Frame, proto: Proto, out: &mut Vec<u8>) -> bool {
    match f {
        Frame::Simple(b) => {
            out.push(b'+');
            out.extend_from_slice(b);
            out.extend_from_slice(CRLF);
        }
        Frame::Error(b) => {
            out.push(b'-');
            out.extend_from_slice(b);
            out.extend_from_slice(CRLF);
        }
        Frame::Integer(i) => {
            out.push(b':');
            out.extend_from_slice(i.to_string().as_bytes());
            out.extend_from_slice(CRLF);
        }
        Frame::Bulk(b) => put_blob(out, b'$', b),
        Frame::NullBulk => out.extend_from_slice(b"$-1\r\n"),
        Frame::NullArray => out.extend_from_slice(b"*-1\r\n"),
        Frame::Null => match proto {
            Proto::Resp3 => out.extend_from_slice(b"_\r\n"),
            Proto::Resp2 => out.extend_from_slice(b"$-1\r\n"),
        },
        Frame::Boolean(b) => match proto {
            Proto::Resp3 => out.extend_from_slice(if *b { b"#t\r\n" } else { b"#f\r\n" }),
            Proto::Resp2 => out.extend_from_slice(if *b { b":1\r\n" } else { b":0\r\n" }),
        },
        Frame::Double(lex) => match proto {
            Proto::Resp3 => {
                out.push(b',');
                out.extend_from_slice(lex);
                out.extend_from_slice(CRLF);
            }
            Proto::Resp2 => put_blob(out, b'$', lex),
        },
        Frame::BigNumber(d) => match proto {
            Proto::Resp3 => {
                out.push(b'(');
                out.extend_from_slice(d);
                out.extend_from_slice(CRLF);
            }
            Proto::Resp2 => put_blob(out, b'$', d),
        },
        Frame::BulkError(b) => match proto {
            Proto::Resp3 => put_blob(out, b'!', b),
            Proto::Resp2 => {
                out.push(b'-');
                out.extend_from_slice(b);
                out.extend_from_slice(CRLF);
            }
        },
        Frame::Verbatim { format, data } => match proto {
            Proto::Resp3 => {
                put_len(out, b'=', data.len() + 4);
                out.extend_from_slice(format);
                out.push(b':');
                out.extend_from_slice(data);
                out.extend_from_slice(CRLF);
            }
            Proto::Resp2 => put_blob(out, b'$', data),
        },
        _ => return false,
    }
    true
}

fn encode_streamed(f: &Frame, proto: Proto, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if proto == Proto::Resp2 {
        return Err(EncodeError::NoResp2Form(match f {
            Frame::StreamedBulk(_) => "streamed-bulk",
            Frame::StreamedMap(_) => "streamed-map",
            _ => "streamed-aggregate",
        }));
    }
    match f {
        Frame::StreamedBulk(chunks) => {
            out.extend_from_slice(b"$?\r\n");
            for c in chunks {
                if c.is_empty() {
                    // `;0` is the terminator, so an empty chunk has no representation.
                    // Refuse loudly: silently dropping it would be undetectable data loss.
                    return Err(EncodeError::UnrepresentableFrame("empty streamed chunk"));
                }
                put_len(out, b';', c.len());
                out.extend_from_slice(c);
                out.extend_from_slice(CRLF);
            }
            out.extend_from_slice(b";0\r\n");
        }
        Frame::StreamedArray(v) | Frame::StreamedSet(v) => {
            out.extend_from_slice(if matches!(f, Frame::StreamedArray(_)) {
                b"*?\r\n"
            } else {
                b"~?\r\n"
            });
            for x in v {
                encode(x, proto, out)?;
            }
            out.extend_from_slice(b".\r\n");
        }
        Frame::StreamedMap(kv) => {
            out.extend_from_slice(b"%?\r\n");
            for (k, v) in kv {
                encode(k, proto, out)?;
                encode(v, proto, out)?;
            }
            out.extend_from_slice(b".\r\n");
        }
        _ => return Ok(()),
    }
    Ok(())
}

/// Encode `f` into `out` using `proto`.
///
/// RESP2 down-conversion follows what Redis does when a RESP2 client is connected:
/// `Null → $-1`, `Boolean → :1/:0`, `Double → $<lexeme>`, `BigNumber → $<digits>`,
/// `BulkError → -<text>`, `Verbatim → $<data>`, `Map → *2n flat`, `Set → *n`.
/// Push / Attribute / Streamed forms are refused because a RESP2 client can never receive them.
///
/// # Errors
/// Returns [`EncodeError::NoResp2Form`] when a RESP3-only aggregate is encoded as RESP2.
pub fn encode(f: &Frame, proto: Proto, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if encode_scalar(f, proto, out) {
        return Ok(());
    }
    match f {
        Frame::Array(v) => {
            put_len(out, b'*', v.len());
            for x in v {
                encode(x, proto, out)?;
            }
        }
        Frame::Map(kv) => match proto {
            Proto::Resp3 => {
                put_len(out, b'%', kv.len());
                for (k, v) in kv {
                    encode(k, proto, out)?;
                    encode(v, proto, out)?;
                }
            }
            Proto::Resp2 => {
                put_len(out, b'*', kv.len() * 2);
                for (k, v) in kv {
                    encode(k, proto, out)?;
                    encode(v, proto, out)?;
                }
            }
        },
        Frame::Set(v) => {
            put_len(
                out,
                if proto == Proto::Resp3 { b'~' } else { b'*' },
                v.len(),
            );
            for x in v {
                encode(x, proto, out)?;
            }
        }
        Frame::Attribute { attrs, value } => match proto {
            Proto::Resp3 => {
                put_len(out, b'|', attrs.len());
                for (k, v) in attrs {
                    encode(k, proto, out)?;
                    encode(v, proto, out)?;
                }
                encode(value, proto, out)?;
            }
            Proto::Resp2 => return Err(EncodeError::NoResp2Form("attribute")),
        },
        // every remaining variant is handled below or by encode_scalar
        Frame::Simple(_)
        | Frame::Error(_)
        | Frame::Integer(_)
        | Frame::Bulk(_)
        | Frame::NullBulk
        | Frame::NullArray
        | Frame::Null
        | Frame::Boolean(_)
        | Frame::Double(_)
        | Frame::BigNumber(_)
        | Frame::BulkError(_)
        | Frame::Verbatim { .. } => unreachable!("handled by encode_scalar"),
        // RESP2 has no push type, but a RESP2 subscriber genuinely receives pub/sub
        // messages as plain arrays — so this down-converts rather than refusing, otherwise
        // RESP2 pub/sub fixtures could not be produced at all.
        Frame::Push(v) => match proto {
            Proto::Resp3 | Proto::Resp2 => {
                put_len(
                    out,
                    if proto == Proto::Resp3 { b'>' } else { b'*' },
                    v.len(),
                );
                for x in v {
                    encode(x, proto, out)?;
                }
            }
        },
        Frame::StreamedBulk(_)
        | Frame::StreamedArray(_)
        | Frame::StreamedSet(_)
        | Frame::StreamedMap(_) => {
            encode_streamed(f, proto, out)?;
        }
    }
    Ok(())
}

/// Encode to a fresh `Vec<u8>`.
///
/// # Errors
/// Returns [`EncodeError::NoResp2Form`] when a RESP3-only aggregate is encoded as RESP2.
pub fn to_vec(f: &Frame, proto: Proto) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    encode(f, proto, &mut out)?;
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn r3(f: &Frame) -> Vec<u8> {
        to_vec(f, Proto::Resp3).unwrap()
    }
    fn r2(f: &Frame) -> Vec<u8> {
        to_vec(f, Proto::Resp2).unwrap()
    }

    #[test]
    fn scalars_match_spec_bytes() {
        assert_eq!(r3(&Frame::simple("OK")), b"+OK\r\n");
        assert_eq!(r3(&Frame::error("ERR x")), b"-ERR x\r\n");
        assert_eq!(r3(&Frame::Integer(-5)), b":-5\r\n");
        assert_eq!(r3(&Frame::bulk(b"a\0b")), b"$3\r\na\0b\r\n");
        assert_eq!(r3(&Frame::NullBulk), b"$-1\r\n");
        assert_eq!(r3(&Frame::Null), b"_\r\n");
        assert_eq!(r2(&Frame::Null), b"$-1\r\n");
        assert_eq!(r3(&Frame::Boolean(true)), b"#t\r\n");
        assert_eq!(r2(&Frame::Boolean(false)), b":0\r\n");
        assert_eq!(r3(&Frame::double("1.2300")), b",1.2300\r\n");
        assert_eq!(r3(&Frame::double("-0")), b",-0\r\n");
        assert_eq!(r3(&Frame::double("inf")), b",inf\r\n");
        assert_eq!(
            r3(&Frame::BigNumber(Bytes::from_static(b"9007199254740993"))),
            b"(9007199254740993\r\n"
        );
        assert_eq!(
            r3(&Frame::BulkError(Bytes::from_static(b"SYNTAX bad"))),
            b"!10\r\nSYNTAX bad\r\n"
        );
        assert_eq!(
            r3(&Frame::Verbatim {
                format: *b"txt",
                data: Bytes::from_static(b"hi")
            }),
            b"=6\r\ntxt:hi\r\n"
        );
    }

    #[test]
    fn aggregates_and_down_conversion() {
        let m = Frame::map([
            (Frame::bulk(b"a"), Frame::Integer(1)),
            (Frame::bulk(b"a"), Frame::Integer(2)),
        ]);
        assert_eq!(r3(&m), b"%2\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n");
        // duplicates preserved, flattened for RESP2
        assert_eq!(r2(&m), b"*4\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n");
        assert_eq!(r3(&Frame::Set(vec![Frame::Integer(1)])), b"~1\r\n:1\r\n");
        assert_eq!(r2(&Frame::Set(vec![Frame::Integer(1)])), b"*1\r\n:1\r\n");
        let attr = Frame::Attribute {
            attrs: vec![(Frame::simple("ttl"), Frame::Integer(3))],
            value: Box::new(Frame::simple("OK")),
        };
        assert_eq!(r3(&attr), b"|1\r\n+ttl\r\n:3\r\n+OK\r\n");
        assert_eq!(
            to_vec(&attr, Proto::Resp2),
            Err(EncodeError::NoResp2Form("attribute"))
        );
        let push = Frame::Push(vec![Frame::simple("message")]);
        assert_eq!(r3(&push), b">1\r\n+message\r\n");
        // RESP2 subscribers receive pub/sub messages as arrays, so this down-converts.
        assert_eq!(r2(&push), b"*1\r\n+message\r\n");
    }

    #[test]
    fn streamed_forms() {
        let s = Frame::StreamedBulk(vec![Bytes::from_static(b"Hel"), Bytes::from_static(b"lo")]);
        assert_eq!(r3(&s), b"$?\r\n;3\r\nHel\r\n;2\r\nlo\r\n;0\r\n");
        let a = Frame::StreamedArray(vec![Frame::Integer(1), Frame::Integer(2)]);
        assert_eq!(r3(&a), b"*?\r\n:1\r\n:2\r\n.\r\n");
        let m = Frame::StreamedMap(vec![(Frame::simple("k"), Frame::Integer(1))]);
        assert_eq!(r3(&m), b"%?\r\n+k\r\n:1\r\n.\r\n");
        assert_eq!(
            to_vec(&s, Proto::Resp2),
            Err(EncodeError::NoResp2Form("streamed-bulk"))
        );
    }

    #[test]
    fn empty_streamed_chunk_is_refused_not_dropped() {
        // `;0` is the terminator; an empty chunk has no wire form. Silently skipping it would
        // be undetectable data loss, which is the one thing this server must never do.
        let s = Frame::StreamedBulk(vec![Bytes::from_static(b"a"), Bytes::new()]);
        assert_eq!(
            to_vec(&s, Proto::Resp3),
            Err(EncodeError::UnrepresentableFrame("empty streamed chunk"))
        );
    }

    #[test]
    fn literal_null_forms_are_emitted_verbatim_in_both_protocols() {
        assert_eq!(r3(&Frame::NullBulk), b"$-1\r\n");
        assert_eq!(r2(&Frame::NullBulk), b"$-1\r\n");
        assert_eq!(r3(&Frame::NullArray), b"*-1\r\n");
        assert_eq!(r2(&Frame::NullArray), b"*-1\r\n");
    }

    #[test]
    fn depth_counts_nesting() {
        let f = Frame::Array(vec![Frame::Array(vec![Frame::Array(vec![
            Frame::Integer(1),
        ])])]);
        assert_eq!(f.depth(), 4);
        assert_eq!(Frame::Integer(1).depth(), 1);
    }
}
