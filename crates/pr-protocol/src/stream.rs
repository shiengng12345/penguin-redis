//! Streamed decoding for values too large to hold (v2.1 §19.3, §24.3, §24.4, V-H02).
//!
//! §24.3 caps a single retained result at 16 MiB and §24.4 says what happens past it: **keep
//! streaming the output**, do not silently truncate, do not grow without bound. [`Decoder`]
//! implements the cap; this module implements the escape.
//!
//! A 512 MB Redis string — the documented maximum [R52] — cannot be answered by a decoder that
//! only ever hands back a finished [`Value`]. Neither can the 1 GiB synthetic blob PERF-01
//! measures, which exists precisely because no real server will send one.
//!
//! ## One semantics, not two
//!
//! §31.2 is blunt that there must not be two subtly different decoders. So this is not a
//! second parser: every scalar small enough to hold is handed to the same
//! [`decoder::parse_value`] the value API uses, and aggregate headers go through the same
//! [`decoder::parse_len`]. The only new code is the part that *cannot* exist in the value API
//! — walking a frame without holding it.
//!
//! `crates/pr-protocol/tests/stream_equivalence.rs` runs the whole V-A03 corpus through both
//! and requires the streamer's reassembled value to equal the decoder's, and their errors to
//! match. That test is what makes the claim above checkable rather than asserted.

use crate::decoder::{Budget, DecodeError, Step as ValueStep, parse_i64, parse_len, parse_value};
use crate::value::{NullForm, Value};
use bytes::{Buf, Bytes, BytesMut};
use std::collections::VecDeque;

/// Which aggregate opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggKind {
    /// `*`
    Array,
    /// `~`
    Set,
    /// `>`
    Push,
    /// `%`
    Map,
    /// `|`
    Attribute,
}

/// Which scalar is being streamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    /// `$`
    Bulk,
    /// `!`
    BulkError,
    /// `=`, with its three-byte format tag already read.
    Verbatim([u8; 3]),
}

/// One step of a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A value small enough to hold, decoded whole by the same code the value API uses.
    Whole(Value),
    /// A scalar too large to hold is starting. `len` is `None` for a RESP3 streamed string
    /// (`$?`), whose total is unknowable until the `;0` terminator arrives.
    ScalarBegin {
        /// Which scalar.
        kind: ScalarKind,
        /// Declared payload length, when the wire declared one.
        len: Option<usize>,
    },
    /// Part of the payload. Never larger than what was already buffered.
    ScalarChunk(Bytes),
    /// The scalar's payload is complete.
    ScalarEnd,
    /// An aggregate is starting. `len` is `None` for a streamed aggregate (`*?`).
    AggregateBegin {
        /// Which aggregate.
        kind: AggKind,
        /// Declared element count (entries, for a map), when declared.
        len: Option<usize>,
    },
    /// The aggregate is complete.
    AggregateEnd,
    /// One top-level frame is complete.
    FrameEnd,
}

/// Result of one [`Streamer::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// An event.
    Event(Event),
    /// Not enough bytes yet.
    Incomplete,
}

/// How much of a large scalar may be held at once, and how large is "large".
#[derive(Clone, Copy, Debug)]
pub struct StreamBudget {
    /// A scalar whose declared length reaches this is streamed rather than held.
    ///
    /// Below it, the value API decodes it whole — cheaper, and it keeps the common case on
    /// exactly one code path.
    pub stream_above: usize,
    /// The largest payload the streamer will accept at all.
    ///
    /// This is not `max_bulk_bytes`: that one bounds what is held in memory, and streaming is
    /// how §24.4 says to exceed it. This one bounds what will be *read*, so a server claiming
    /// an absurd length still meets a limit rather than an open-ended read.
    pub max_streamed_bytes: usize,
}

impl Default for StreamBudget {
    fn default() -> Self {
        Self {
            // Below 1 MiB there is nothing to gain and a whole `Value` is easier to work with.
            stream_above: 1024 * 1024,
            // 2 GiB: comfortably past the 1 GiB PERF-01 blob and past Redis's 512 MB string
            // limit [R52], while still being a number rather than "whatever arrives".
            max_streamed_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
enum State {
    /// Expecting the next value.
    Value,
    /// Emitting a declared-length scalar's payload.
    Payload { remaining: usize },
    /// Emitting a `$?` streamed string: expecting the next `;n` header.
    ChunkHeader,
    /// Emitting a `$?` streamed string's current chunk.
    Chunk { remaining: usize },
}

#[derive(Debug)]
struct Pending {
    /// Kept for diagnostics: a stack dump that says only "three deep" is not a diagnostic.
    #[allow(dead_code)]
    kind: AggKind,
    /// Remaining child *values* (a map of n entries has 2n; an attribute of n has 2n + 1).
    /// `None` means a streamed aggregate, ended by `.`.
    remaining: Option<usize>,
}

/// A resumable decoder that walks a frame without holding it.
#[derive(Debug)]
pub struct Streamer {
    buf: BytesMut,
    budget: Budget,
    stream: StreamBudget,
    stack: Vec<Pending>,
    state: State,
    queued: VecDeque<Event>,
    /// Total payload bytes emitted for the scalar in progress, for the streamed-string cap.
    streamed_so_far: usize,
}

impl Streamer {
    /// Create one.
    #[must_use]
    pub fn new(budget: Budget, stream: StreamBudget) -> Self {
        Self {
            buf: BytesMut::new(),
            budget,
            stream,
            stack: Vec::new(),
            state: State::Value,
            queued: VecDeque::new(),
            streamed_so_far: 0,
        }
    }

    /// Create one with default budgets.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(Budget::default(), StreamBudget::default())
    }

    /// Append received bytes.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes currently held.
    ///
    /// This is the number V-H02 measures: if it tracks the transport's read size rather than
    /// the frame's declared size, the frame is genuinely streaming.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Produce the next event.
    ///
    /// # Errors
    /// [`DecodeError::Protocol`] for malformed framing, [`DecodeError::Budget`] for a limit.
    pub fn step(&mut self) -> Result<Step, DecodeError> {
        if let Some(e) = self.queued.pop_front() {
            return Ok(Step::Event(e));
        }
        match self.state {
            State::Payload { .. } | State::Chunk { .. } => self.continue_payload(),
            State::ChunkHeader => self.chunk_header(),
            State::Value => self.next_value(),
        }
    }

    /// Emit whatever payload is already buffered.
    fn continue_payload(&mut self) -> Result<Step, DecodeError> {
        let (remaining, streamed) = match self.state {
            State::Payload { remaining } => (remaining, false),
            State::Chunk { remaining } => (remaining, true),
            State::Value | State::ChunkHeader => unreachable!("checked by the caller"),
        };

        if remaining == 0 {
            // The CRLF that closes the payload.
            if self.buf.len() < 2 {
                return Ok(Step::Incomplete);
            }
            if &self.buf[..2] != b"\r\n" {
                return Err(DecodeError::Protocol("bulk not CRLF terminated"));
            }
            self.buf.advance(2);
            if streamed {
                self.state = State::ChunkHeader;
                return self.step();
            }
            self.state = State::Value;
            self.finish_value(Event::ScalarEnd);
            return self.step();
        }

        if self.buf.is_empty() {
            return Ok(Step::Incomplete);
        }
        let take = remaining.min(self.buf.len());
        let chunk = self.buf.split_to(take).freeze();
        let left = remaining - take;
        self.state = if streamed {
            State::Chunk { remaining: left }
        } else {
            State::Payload { remaining: left }
        };
        Ok(Step::Event(Event::ScalarChunk(chunk)))
    }

    /// The `;n` header inside a `$?` streamed string.
    fn chunk_header(&mut self) -> Result<Step, DecodeError> {
        if self.buf.is_empty() {
            return Ok(Step::Incomplete);
        }
        if self.buf[0] != b';' {
            return Err(DecodeError::Protocol("streamed chunk must start with ';'"));
        }
        let mut pos = 1usize;
        let Some(hdr) = crate::decoder::line(&self.buf, &mut pos)? else {
            return Ok(Step::Incomplete);
        };
        let n = parse_i64(hdr)?;
        if n < 0 {
            return Err(DecodeError::Protocol(
                "streamed chunk length cannot be null",
            ));
        }
        let n = usize::try_from(n).map_err(|_| DecodeError::Budget("bulk"))?;
        self.streamed_so_far = self.streamed_so_far.saturating_add(n);
        if self.streamed_so_far > self.stream.max_streamed_bytes {
            return Err(DecodeError::Budget("bulk"));
        }
        self.buf.advance(pos);
        if n == 0 {
            self.state = State::Value;
            self.finish_value(Event::ScalarEnd);
            return self.step();
        }
        self.state = State::Chunk { remaining: n };
        self.step()
    }

    #[allow(clippy::too_many_lines)]
    fn next_value(&mut self) -> Result<Step, DecodeError> {
        if self.buf.is_empty() {
            return Ok(Step::Incomplete);
        }

        // The `.` that closes a streamed aggregate, checked before anything else because `.`
        // is not a value tag.
        if self.buf[0] == b'.' && self.stack.last().is_some_and(|p| p.remaining.is_none()) {
            let mut pos = 1usize;
            let Some(rest) = crate::decoder::line(&self.buf, &mut pos)? else {
                return Ok(Step::Incomplete);
            };
            if !rest.is_empty() {
                return Err(DecodeError::Protocol(
                    "streamed terminator takes no payload",
                ));
            }
            self.buf.advance(pos);
            self.stack.pop();
            self.queued.push_back(Event::AggregateEnd);
            self.close_completed();
            return self.step();
        }

        let tag = self.buf[0];
        match tag {
            b'*' | b'~' | b'>' | b'%' | b'|' => self.open_aggregate(tag),
            b'$' | b'!' | b'=' => self.open_scalar(tag),
            _ => {
                // Everything else is a scalar the value API can decode whole. Reusing it is
                // what keeps the two APIs from drifting.
                let mut pos = 0usize;
                match parse_value(&self.buf, &mut pos, self.stack.len(), &self.budget)? {
                    None => Ok(Step::Incomplete),
                    Some(v) => {
                        self.buf.advance(pos);
                        self.finish_value(Event::Whole(v));
                        self.step()
                    }
                }
            }
        }
    }

    fn open_aggregate(&mut self, tag: u8) -> Result<Step, DecodeError> {
        if self.stack.len() >= self.budget.max_depth {
            return Err(DecodeError::Budget("depth"));
        }
        let mut pos = 1usize;
        let Some(hdr) = crate::decoder::line(&self.buf, &mut pos)? else {
            return Ok(Step::Incomplete);
        };
        let kind = match tag {
            b'*' => AggKind::Array,
            b'~' => AggKind::Set,
            b'>' => AggKind::Push,
            b'%' => AggKind::Map,
            _ => AggKind::Attribute,
        };

        if hdr == b"?" {
            if kind == AggKind::Attribute {
                return Err(DecodeError::Protocol("unknown type byte"));
            }
            self.buf.advance(pos);
            self.stack.push(Pending {
                kind,
                remaining: None,
            });
            return Ok(Step::Event(Event::AggregateBegin { kind, len: None }));
        }

        let Some(n) = parse_len(hdr, &self.budget, "elements")? else {
            // `*-1` and friends: a null, not an empty aggregate.
            if kind == AggKind::Attribute {
                return Err(DecodeError::Protocol("attribute length cannot be null"));
            }
            self.buf.advance(pos);
            self.finish_value(Event::Whole(Value::Null(NullForm::Array)));
            return self.step();
        };
        self.buf.advance(pos);

        let children = match kind {
            AggKind::Map => n.checked_mul(2),
            // `|n` is n key/value pairs *plus* the value it decorates.
            AggKind::Attribute => n.checked_mul(2).and_then(|c| c.checked_add(1)),
            _ => Some(n),
        }
        .ok_or(DecodeError::Budget("elements"))?;

        self.queued
            .push_back(Event::AggregateBegin { kind, len: Some(n) });
        if children == 0 {
            self.queued.push_back(Event::AggregateEnd);
            // An empty aggregate is itself a completed value for its parent.
            self.close_completed();
        } else {
            self.stack.push(Pending {
                kind,
                remaining: Some(children),
            });
        }
        self.step()
    }

    fn open_scalar(&mut self, tag: u8) -> Result<Step, DecodeError> {
        let mut pos = 1usize;
        let Some(hdr) = crate::decoder::line(&self.buf, &mut pos)? else {
            return Ok(Step::Incomplete);
        };

        if tag == b'$' && hdr == b"?" {
            self.buf.advance(pos);
            self.streamed_so_far = 0;
            self.state = State::ChunkHeader;
            return Ok(Step::Event(Event::ScalarBegin {
                kind: ScalarKind::Bulk,
                len: None,
            }));
        }

        let n = parse_i64(hdr)?;
        if n == -1 {
            self.buf.advance(pos);
            self.finish_value(Event::Whole(Value::Null(NullForm::Bulk)));
            return self.step();
        }
        if n < 0 {
            return Err(DecodeError::Protocol("negative length"));
        }
        let n = usize::try_from(n).map_err(|_| DecodeError::Budget("bulk"))?;
        if n > self.stream.max_streamed_bytes {
            return Err(DecodeError::Budget("bulk"));
        }

        if n < self.stream.stream_above {
            // Small enough to hold: the value API owns it, including every validation it does.
            let mut whole = 0usize;
            return match parse_value(&self.buf, &mut whole, self.stack.len(), &self.budget)? {
                None => Ok(Step::Incomplete),
                Some(v) => {
                    self.buf.advance(whole);
                    self.finish_value(Event::Whole(v));
                    self.step()
                }
            };
        }

        let kind = match tag {
            b'$' => ScalarKind::Bulk,
            b'!' => ScalarKind::BulkError,
            _ => {
                // A verbatim string's `xxx:` prefix is framing, not payload, so it has to be
                // read before the first chunk goes out or the consumer would have to strip it.
                if n < 4 {
                    return Err(DecodeError::Protocol("verbatim needs `xxx:` prefix"));
                }
                if self.buf.len() < pos + 4 {
                    return Ok(Step::Incomplete);
                }
                if self.buf[pos + 3] != b':' {
                    return Err(DecodeError::Protocol("verbatim needs `xxx:` prefix"));
                }
                let mut f = [0u8; 3];
                f.copy_from_slice(&self.buf[pos..pos + 3]);
                ScalarKind::Verbatim(f)
            }
        };
        let payload = if let ScalarKind::Verbatim(_) = kind {
            pos += 4;
            n - 4
        } else {
            n
        };
        self.buf.advance(pos);
        self.state = State::Payload { remaining: payload };
        Ok(Step::Event(Event::ScalarBegin {
            kind,
            len: Some(payload),
        }))
    }

    /// Record that one value finished, queue `e`, and close any aggregates it completed.
    fn finish_value(&mut self, e: Event) {
        self.queued.push_back(e);
        self.close_completed();
    }

    fn close_completed(&mut self) {
        loop {
            match self.stack.last_mut() {
                None => {
                    self.queued.push_back(Event::FrameEnd);
                    return;
                }
                Some(p) => match p.remaining.as_mut() {
                    // A streamed aggregate ends at `.`, not at a count.
                    None => return,
                    Some(left) => {
                        *left -= 1;
                        if *left > 0 {
                            return;
                        }
                        self.stack.pop();
                        self.queued.push_back(Event::AggregateEnd);
                    }
                },
            }
        }
    }
}

/// Rebuild a [`Value`] from a stream of events.
///
/// Used by the equivalence test and by callers that turn out not to need streaming after all.
/// It defeats the purpose for a large payload — which is exactly why the test that uses it
/// also measures [`Streamer::buffered`].
#[derive(Debug, Default)]
pub struct Reassembler {
    stack: Vec<(AggKind, Vec<Value>)>,
    scalar: Option<(ScalarKind, Vec<u8>)>,
    done: Option<Value>,
}

impl Reassembler {
    /// Feed one event.
    ///
    /// # Errors
    /// [`DecodeError::Protocol`] if the events do not form a frame, which would mean a bug in
    /// the streamer rather than in the wire.
    #[allow(clippy::too_many_lines)]
    pub fn push(&mut self, e: Event) -> Result<(), DecodeError> {
        match e {
            Event::Whole(v) => {
                self.value(v);
                Ok(())
            }
            Event::ScalarBegin { kind, .. } => {
                self.scalar = Some((kind, Vec::new()));
                Ok(())
            }
            Event::ScalarChunk(b) => {
                let Some((_, buf)) = self.scalar.as_mut() else {
                    return Err(DecodeError::Protocol("chunk outside a scalar"));
                };
                buf.extend_from_slice(&b);
                Ok(())
            }
            Event::ScalarEnd => {
                let Some((kind, buf)) = self.scalar.take() else {
                    return Err(DecodeError::Protocol("scalar end outside a scalar"));
                };
                let data = Bytes::from(buf);
                self.value(match kind {
                    ScalarKind::Bulk => Value::Bulk(data),
                    ScalarKind::BulkError => Value::BulkError(data),
                    ScalarKind::Verbatim(format) => Value::Verbatim { format, data },
                });
                Ok(())
            }
            Event::AggregateBegin { kind, .. } => {
                self.stack.push((kind, Vec::new()));
                Ok(())
            }
            Event::AggregateEnd => {
                let Some((kind, items)) = self.stack.pop() else {
                    return Err(DecodeError::Protocol("aggregate end with none open"));
                };
                self.value(finish_aggregate(kind, items)?);
                Ok(())
            }
            Event::FrameEnd => Ok(()),
        }
    }

    /// The finished frame, if one completed.
    #[must_use]
    pub fn take(&mut self) -> Option<Value> {
        self.done.take()
    }

    fn value(&mut self, v: Value) {
        match self.stack.last_mut() {
            Some((_, items)) => items.push(v),
            None => self.done = Some(v),
        }
    }
}

fn finish_aggregate(kind: AggKind, items: Vec<Value>) -> Result<Value, DecodeError> {
    Ok(match kind {
        AggKind::Array => Value::Array(items),
        AggKind::Set => Value::Set(items),
        AggKind::Push => Value::Push(items),
        AggKind::Map => Value::Map(pairs(items)?),
        AggKind::Attribute => {
            let mut items = items;
            let Some(value) = items.pop() else {
                return Err(DecodeError::Protocol("attribute with no value"));
            };
            Value::Attribute {
                attrs: pairs(items)?,
                value: Box::new(value),
            }
        }
    })
}

fn pairs(items: Vec<Value>) -> Result<Vec<(Value, Value)>, DecodeError> {
    if !items.len().is_multiple_of(2) {
        return Err(DecodeError::Protocol("odd number of map entries"));
    }
    let mut out = Vec::with_capacity(items.len() / 2);
    let mut it = items.into_iter();
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        out.push((k, v));
    }
    Ok(out)
}

/// Decode a complete frame from `bytes` by streaming it, for tests and small callers.
///
/// # Errors
/// Whatever the streamer reports.
pub fn decode_by_streaming(
    bytes: &[u8],
    budget: Budget,
    stream: StreamBudget,
) -> Result<Option<Value>, DecodeError> {
    let mut s = Streamer::new(budget, stream);
    s.feed(bytes);
    let mut r = Reassembler::default();
    loop {
        match s.step()? {
            Step::Incomplete => return Ok(None),
            Step::Event(Event::FrameEnd) => {
                r.push(Event::FrameEnd)?;
                return Ok(r.take());
            }
            Step::Event(e) => r.push(e)?,
        }
    }
}

/// The value API's verdict on the same bytes, for comparison.
///
/// # Errors
/// Whatever the decoder reports.
pub fn decode_by_value(bytes: &[u8], budget: Budget) -> Result<Option<Value>, DecodeError> {
    let mut d = crate::decoder::Decoder::new(budget);
    d.feed(bytes);
    match d.decode()? {
        ValueStep::Value(v) => Ok(Some(v)),
        ValueStep::Incomplete => Ok(None),
    }
}
