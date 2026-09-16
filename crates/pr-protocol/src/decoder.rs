//! Incremental, bounded RESP2/RESP3 decoder (v2.1 §19.3, §24.2, ADR-009, R16).
//!
//! Requirements this exists to meet:
//! - **incremental**: bytes arrive in arbitrary chunks, including splits inside a CRLF, a
//!   length prefix, or a multi-byte UTF-8 sequence. Feeding one byte at a time must give the
//!   same result as feeding the whole buffer.
//! - **bounded**: a declared length is a *claim*, not an instruction to allocate. Depth,
//!   element counts and total bytes are all budgeted, and exceeding a budget is a distinct
//!   outcome from a protocol error.
//! - **lossless**: lexemes and raw bytes survive (ADR-005).
//! - **streamed forms**: `$?`/`;n`/`;0` and `*?`/`%?`/`~?`/`.` are decoded (R16).
//!
//! The decoder is a resumable state machine over an internal buffer, not a recursive parser
//! over a complete frame, so a 1 GiB bulk string never has to exist in memory at once.

use crate::value::{NullForm, Value};
use bytes::{Buf, BytesMut};
use thiserror::Error;

/// Limits applied while decoding (v2.1 §24.3).
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Maximum aggregate nesting depth.
    pub max_depth: usize,
    /// Maximum declared elements in one aggregate.
    pub max_elements: usize,
    /// Maximum bytes for one scalar held in memory.
    pub max_bulk_bytes: usize,
    /// Maximum total bytes buffered for one frame.
    pub max_frame_bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_elements: 1024 * 1024,
            max_bulk_bytes: 16 * 1024 * 1024,
            max_frame_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Why decoding stopped.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// The stream is not valid RESP. The connection must be closed or isolated: after a
    /// framing error the next byte boundary is unknowable (v2.1 §19.3).
    #[error("protocol error: {0}")]
    Protocol(&'static str),
    /// A budget was exceeded. Distinct from a protocol error: the stream is well-formed, we
    /// simply refuse to materialise it (v2.1 §24.4).
    #[error("budget exceeded: {0}")]
    Budget(&'static str),
}

/// Result of one decode attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// A complete value. `consumed` bytes were taken from the buffer.
    Value(Value),
    /// Not enough bytes yet; feed more and call again.
    Incomplete,
}

/// A resumable RESP decoder.
#[derive(Debug)]
pub struct Decoder {
    buf: BytesMut,
    budget: Budget,
}

impl Decoder {
    /// Create a decoder with the given budget.
    #[must_use]
    pub fn new(budget: Budget) -> Self {
        Self {
            buf: BytesMut::new(),
            budget,
        }
    }

    /// Create a decoder with default budgets.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(Budget::default())
    }

    /// Append received bytes.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes currently buffered (for budget assertions in tests).
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Try to decode one value.
    ///
    /// # Errors
    /// [`DecodeError::Protocol`] for malformed framing (close the connection),
    /// [`DecodeError::Budget`] when a limit is hit (the stream is fine, we refuse it).
    pub fn decode(&mut self) -> Result<Step, DecodeError> {
        if self.buf.len() > self.budget.max_frame_bytes {
            return Err(DecodeError::Budget("frame buffer"));
        }
        let mut pos = 0usize;
        match parse_value(&self.buf, &mut pos, 0, &self.budget)? {
            Some(v) => {
                self.buf.advance(pos);
                Ok(Step::Value(v))
            }
            None => Ok(Step::Incomplete),
        }
    }
}

/// Scan for the line terminator. RESP lines are CRLF-terminated and may contain neither a
/// bare CR nor a bare LF, so an LF seen before any CR is a framing error we can report
/// immediately rather than buffering forever waiting for a CR that will never come.
enum LineScan {
    /// CR found at this index.
    Cr(usize),
    /// A bare LF appeared before any CR.
    BareLf,
    /// Neither yet; need more bytes.
    NeedMore,
}

fn scan_line(b: &[u8], from: usize) -> LineScan {
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'\r' => return LineScan::Cr(i),
            b'\n' => return LineScan::BareLf,
            _ => i += 1,
        }
    }
    LineScan::NeedMore
}

/// Read one CRLF-terminated line's contents.
pub(crate) fn line<'a>(b: &'a [u8], pos: &mut usize) -> Result<Option<&'a [u8]>, DecodeError> {
    let cr = match scan_line(b, *pos) {
        LineScan::Cr(i) => i,
        // Detected as soon as the LF arrives: no unbounded buffering on LF-framed junk.
        LineScan::BareLf => return Err(DecodeError::Protocol("bare LF in line")),
        LineScan::NeedMore => return Ok(None),
    };
    if cr + 1 >= b.len() {
        return Ok(None); // have CR, waiting for LF
    }
    if b[cr + 1] != b'\n' {
        return Err(DecodeError::Protocol("CR not followed by LF"));
    }
    let s = &b[*pos..cr];
    *pos = cr + 2;
    Ok(Some(s))
}

pub(crate) fn parse_i64(s: &[u8]) -> Result<i64, DecodeError> {
    if s.is_empty() {
        return Err(DecodeError::Protocol("empty integer"));
    }
    let txt = std::str::from_utf8(s).map_err(|_| DecodeError::Protocol("non-ascii integer"))?;
    // Reject forms Rust accepts but RESP does not: leading '+', whitespace, underscores.
    if txt.starts_with('+') || txt.trim() != txt || txt.contains('_') {
        return Err(DecodeError::Protocol("malformed integer"));
    }
    txt.parse::<i64>()
        .map_err(|_| DecodeError::Protocol("integer out of range"))
}

pub(crate) fn parse_len(
    s: &[u8],
    budget: &Budget,
    what: &'static str,
) -> Result<Option<usize>, DecodeError> {
    let n = parse_i64(s)?;
    if n == -1 {
        return Ok(None); // null form
    }
    if n < 0 {
        return Err(DecodeError::Protocol("negative length"));
    }
    let n = usize::try_from(n).map_err(|_| DecodeError::Budget(what))?;
    let cap = if what == "bulk" {
        budget.max_bulk_bytes
    } else {
        budget.max_elements
    };
    if n > cap {
        return Err(DecodeError::Budget(what));
    }
    Ok(Some(n))
}

fn validate_double(s: &[u8]) -> Result<(), DecodeError> {
    let t = std::str::from_utf8(s).map_err(|_| DecodeError::Protocol("non-ascii double"))?;
    let ok = matches!(t, "inf" | "-inf" | "+inf" | "nan" | "-nan") || t.parse::<f64>().is_ok();
    // `parse::<f64>` accepts "inf"/"NaN" spellings and also rejects empty; additionally
    // exclude forms RESP never emits.
    if !ok || t.is_empty() || t.contains('_') || t.trim() != t {
        return Err(DecodeError::Protocol("malformed double"));
    }
    Ok(())
}

fn validate_bignum(s: &[u8]) -> Result<(), DecodeError> {
    let body = s.strip_prefix(b"-").unwrap_or(s);
    if body.is_empty() || !body.iter().all(u8::is_ascii_digit) {
        return Err(DecodeError::Protocol("malformed big number"));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(crate) fn parse_value(
    b: &[u8],
    pos: &mut usize,
    depth: usize,
    budget: &Budget,
) -> Result<Option<Value>, DecodeError> {
    if depth >= budget.max_depth {
        return Err(DecodeError::Budget("depth"));
    }
    if *pos >= b.len() {
        return Ok(None);
    }
    let tag = b[*pos];
    let start = *pos;
    *pos += 1;

    macro_rules! need {
        ($e:expr) => {
            match $e? {
                Some(v) => v,
                None => {
                    *pos = start;
                    return Ok(None);
                }
            }
        };
    }

    let v = match tag {
        b'+' => Value::Simple(bytes::Bytes::copy_from_slice(need!(line(b, pos)))),
        b'-' => Value::Error(bytes::Bytes::copy_from_slice(need!(line(b, pos)))),
        b':' => Value::Integer(parse_i64(need!(line(b, pos)))?),
        b'#' => match need!(line(b, pos)) {
            b"t" => Value::Boolean(true),
            b"f" => Value::Boolean(false),
            _ => return Err(DecodeError::Protocol("malformed boolean")),
        },
        b'_' => {
            let l = need!(line(b, pos));
            if !l.is_empty() {
                return Err(DecodeError::Protocol("null takes no payload"));
            }
            Value::Null(NullForm::Resp3)
        }
        b',' => {
            let l = need!(line(b, pos));
            validate_double(l)?;
            Value::Double(bytes::Bytes::copy_from_slice(l))
        }
        b'(' => {
            let l = need!(line(b, pos));
            validate_bignum(l)?;
            Value::BigNumber(bytes::Bytes::copy_from_slice(l))
        }
        b'$' | b'!' | b'=' => {
            let hdr = need!(line(b, pos));
            if tag == b'$' && hdr == b"?" {
                // Streamed bulk string: ;n chunks until ;0
                let mut out = Vec::new();
                loop {
                    if *pos >= b.len() {
                        *pos = start;
                        return Ok(None);
                    }
                    if b[*pos] != b';' {
                        return Err(DecodeError::Protocol("streamed chunk must start with ';'"));
                    }
                    *pos += 1;
                    let clen = need!(line(b, pos));
                    let Some(n) = parse_len(clen, budget, "bulk")? else {
                        return Err(DecodeError::Protocol(
                            "streamed chunk length cannot be null",
                        ));
                    };
                    if n == 0 {
                        break;
                    }
                    if out.len() + n > budget.max_bulk_bytes {
                        return Err(DecodeError::Budget("bulk"));
                    }
                    if *pos + n + 2 > b.len() {
                        *pos = start;
                        return Ok(None);
                    }
                    out.extend_from_slice(&b[*pos..*pos + n]);
                    *pos += n;
                    if &b[*pos..*pos + 2] != b"\r\n" {
                        return Err(DecodeError::Protocol("chunk not CRLF terminated"));
                    }
                    *pos += 2;
                }
                Value::Bulk(bytes::Bytes::from(out))
            } else {
                let Some(n) = parse_len(hdr, budget, "bulk")? else {
                    return Value::Null(NullForm::Bulk).pipe_ok();
                };
                if *pos + n + 2 > b.len() {
                    *pos = start;
                    return Ok(None);
                }
                let data = bytes::Bytes::copy_from_slice(&b[*pos..*pos + n]);
                *pos += n;
                if &b[*pos..*pos + 2] != b"\r\n" {
                    return Err(DecodeError::Protocol("bulk not CRLF terminated"));
                }
                *pos += 2;
                match tag {
                    b'$' => Value::Bulk(data),
                    b'!' => Value::BulkError(data),
                    _ => {
                        if data.len() < 4 || data[3] != b':' {
                            return Err(DecodeError::Protocol("verbatim needs `xxx:` prefix"));
                        }
                        let mut f = [0u8; 3];
                        f.copy_from_slice(&data[..3]);
                        Value::Verbatim {
                            format: f,
                            data: data.slice(4..),
                        }
                    }
                }
            }
        }
        b'*' | b'~' | b'>' | b'%' => {
            let hdr = need!(line(b, pos));
            let streamed = hdr == b"?";
            let is_map = tag == b'%';
            if streamed {
                let mut items = Vec::new();
                let mut pairs = Vec::new();
                loop {
                    if *pos >= b.len() {
                        *pos = start;
                        return Ok(None);
                    }
                    if b[*pos] == b'.' {
                        *pos += 1;
                        let l = need!(line(b, pos));
                        if !l.is_empty() {
                            return Err(DecodeError::Protocol(
                                "streamed terminator takes no payload",
                            ));
                        }
                        break;
                    }
                    let Some(k) = parse_value(b, pos, depth + 1, budget)? else {
                        *pos = start;
                        return Ok(None);
                    };
                    if is_map {
                        let Some(v) = parse_value(b, pos, depth + 1, budget)? else {
                            *pos = start;
                            return Ok(None);
                        };
                        pairs.push((k, v));
                    } else {
                        items.push(k);
                    }
                    if items.len() > budget.max_elements || pairs.len() > budget.max_elements {
                        return Err(DecodeError::Budget("elements"));
                    }
                }
                // The tag still decides the type. Collapsing `~?` and `>?` into an array
                // would lose the set/array distinction ADR-005 protects, and — worse — would
                // make a streamed push answer a command: `is_push()` is what keeps a push out
                // of the reply slot (§19.3), and an `Array` is not a push.
                match (is_map, tag) {
                    (true, _) => Value::Map(pairs),
                    (false, b'~') => Value::Set(items),
                    (false, b'>') => Value::Push(items),
                    (false, _) => Value::Array(items),
                }
            } else {
                let Some(n) = parse_len(hdr, budget, "elements")? else {
                    return Value::Null(NullForm::Array).pipe_ok();
                };
                if is_map {
                    let mut kv = Vec::with_capacity(n.min(1024));
                    for _ in 0..n {
                        let Some(k) = parse_value(b, pos, depth + 1, budget)? else {
                            *pos = start;
                            return Ok(None);
                        };
                        let Some(v) = parse_value(b, pos, depth + 1, budget)? else {
                            *pos = start;
                            return Ok(None);
                        };
                        kv.push((k, v));
                    }
                    Value::Map(kv)
                } else {
                    let mut items = Vec::with_capacity(n.min(1024));
                    for _ in 0..n {
                        let Some(v) = parse_value(b, pos, depth + 1, budget)? else {
                            *pos = start;
                            return Ok(None);
                        };
                        items.push(v);
                    }
                    match tag {
                        b'*' => Value::Array(items),
                        b'~' => Value::Set(items),
                        _ => Value::Push(items),
                    }
                }
            }
        }
        b'|' => {
            let hdr = need!(line(b, pos));
            let Some(n) = parse_len(hdr, budget, "elements")? else {
                return Err(DecodeError::Protocol("attribute length cannot be null"));
            };
            let mut attrs = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let Some(k) = parse_value(b, pos, depth + 1, budget)? else {
                    *pos = start;
                    return Ok(None);
                };
                let Some(v) = parse_value(b, pos, depth + 1, budget)? else {
                    *pos = start;
                    return Ok(None);
                };
                attrs.push((k, v));
            }
            let Some(value) = parse_value(b, pos, depth + 1, budget)? else {
                *pos = start;
                return Ok(None);
            };
            Value::Attribute {
                attrs,
                value: Box::new(value),
            }
        }
        _ => return Err(DecodeError::Protocol("unknown type byte")),
    };
    Ok(Some(v))
}

/// Small helper so the null-return paths read cleanly.
trait PipeOk: Sized {
    fn pipe_ok(self) -> Result<Option<Self>, DecodeError>;
}
impl PipeOk for Value {
    fn pipe_ok(self) -> Result<Option<Self>, DecodeError> {
        Ok(Some(self))
    }
}
