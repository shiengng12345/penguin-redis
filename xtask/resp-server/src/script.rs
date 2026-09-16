//! Scripted server behaviour: what to send, how to fragment it, when to stall or drop.

use crate::frame::Frame;

/// One step in a server script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Read one client command; if `expect` is non-empty, assert argv equality (bytes).
    Expect(Vec<Vec<u8>>),
    /// Read and discard one client command.
    ReadCommand,
    /// Encode and send a frame in one write.
    Send(Frame),
    /// Send raw bytes verbatim (for malformed corpora).
    SendRaw(Vec<u8>),
    /// Send bytes split into fixed-size chunks with a delay between them (half-packets).
    SendChunked {
        /// Bytes to send.
        bytes: Vec<u8>,
        /// Chunk size in bytes (≥ 1).
        chunk: usize,
        /// Delay between chunks.
        delay_ms: u64,
    },
    /// Send bytes split at explicit offsets (e.g. inside a UTF-8 sequence or a CRLF).
    SplitAt {
        /// Bytes to send.
        bytes: Vec<u8>,
        /// Split offsets, ascending.
        at: Vec<usize>,
        /// Delay between pieces.
        delay_ms: u64,
    },
    /// Stream a bulk string of `len` bytes filled with `fill`, `chunk` bytes per write,
    /// **without materialising it** — used for the 1 GiB synthetic blob (PERF-01).
    BlobStream {
        /// Declared and actual length.
        len: u64,
        /// Bytes per socket write.
        chunk: usize,
        /// Fill byte.
        fill: u8,
    },
    /// Declare a bulk of `declared` bytes but send only `actual` then close (truncation).
    TruncatedBulk {
        /// Declared length.
        declared: u64,
        /// Bytes actually sent.
        actual: usize,
    },
    /// Sleep.
    Delay(u64),
    /// Close the connection.
    Close,
    /// Stop responding but keep the socket open until the client disconnects.
    Hang,
}

/// A full script for one connection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    /// Steps in order.
    pub steps: Vec<Step>,
}

impl Script {
    /// Empty script.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Append a step (builder style).
    #[must_use]
    pub fn then(mut self, s: Step) -> Self {
        self.steps.push(s);
        self
    }
    /// Convenience: expect any command then reply with `f`.
    #[must_use]
    pub fn reply(self, f: Frame) -> Self {
        self.then(Step::ReadCommand).then(Step::Send(f))
    }
}
