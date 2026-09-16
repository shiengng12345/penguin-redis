//! One-shot passthrough connection — V-A02 scaffolding (v2.1 §31.3, §32.2).
//!
//! **Scope, stated plainly.** This is the "骨架 `prc`（只做透传）" the Phase 0 plan asks V-A02
//! to build the differential harness against: connect, send exactly the argv bytes the user
//! typed, read exactly one reply, print it. It is blocking, single-connection, and knows
//! nothing about policy, routing, retry or cancellation.
//!
//! It exists so that the harness has something real to compare against `redis-cli` **before**
//! the kernel exists, which is the only way the four-layer comparison can be built and
//! debugged in Phase 0 rather than discovered to be wrong in Phase 1.
//!
//! ADR-008 says every entry point goes through one execution kernel. This is not that kernel
//! and must not grow into one: Phase 1 replaces it. What it must not do is acquire features —
//! the moment it learns about policy or retries, there are two semantics to keep aligned, and
//! §31.2 is explicit that there must not be.

use bytes::Bytes;
use pr_protocol::decoder::{DecodeError, Decoder, Step};
use pr_protocol::value::Value;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Open a TCP connection with the timeouts a one-shot needs.
///
/// Shared with [`crate::tls`], which needs the same socket before it can hand it to rustls.
///
/// # Errors
/// [`CallError::Io`] if the connection fails, [`CallError::Unresolved`] if the name does not
/// resolve.
pub fn connect_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, CallError> {
    let addr = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| CallError::Unresolved(format!("{host}:{port}")))?;
    let stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// Why a one-shot call did not produce a reply.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The socket failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The server's bytes are not RESP, or exceeded a budget.
    #[error("{0}")]
    Decode(#[from] DecodeError),
    /// The peer closed before a whole reply arrived.
    ///
    /// Distinct from [`CallError::Io`] on purpose: the command was sent, so whether it took
    /// effect is unknown (ADR-007).
    #[error("the server closed the connection with a reply half-sent")]
    TruncatedReply,
    /// No address resolved.
    #[error("cannot resolve {0}")]
    Unresolved(String),
}

/// A blocking connection that can carry one command at a time.
///
/// Generic over the transport so plaintext and TLS share one command path. §31.2's warning
/// about two subtly different semantics applies here too: a TLS client that framed replies
/// slightly differently from the plaintext one would be a second protocol implementation.
#[derive(Debug)]
pub struct Oneshot<S = TcpStream> {
    stream: S,
    decoder: Decoder,
}

impl Oneshot<TcpStream> {
    /// Connect to `host:port` in plaintext.
    ///
    /// # Errors
    /// [`CallError::Io`] if the connection fails, [`CallError::Unresolved`] if the name does
    /// not resolve.
    pub fn connect(host: &str, port: u16, timeout: Duration) -> Result<Self, CallError> {
        Ok(Self::over(connect_tcp(host, port, timeout)?))
    }
}

impl<S: Read + Write> Oneshot<S> {
    /// Wrap an already-established transport, such as a completed TLS handshake.
    pub fn over(stream: S) -> Self {
        Self {
            stream,
            decoder: Decoder::with_defaults(),
        }
    }

    /// Send a command and read one reply.
    ///
    /// Push frames are skipped rather than returned: §19.3 says a push never occupies a
    /// command's reply slot. A passthrough has nowhere to route them, so it drops them — and
    /// says so here, because silently dropping out-of-band data is the sort of thing that
    /// should be written down even in scaffolding.
    ///
    /// # Errors
    /// [`CallError`] for any socket, framing or budget failure.
    pub fn call(&mut self, argv: &[Bytes]) -> Result<Value, CallError> {
        let wire = encode_command(argv);
        self.stream.write_all(&wire)?;
        self.stream.flush()?;
        self.read_reply()
    }

    fn read_reply(&mut self) -> Result<Value, CallError> {
        // On the heap: a 64 KiB stack frame is fine on a main thread and is not fine on a
        // thread with a small stack, which is where this will eventually be called from.
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match self.decoder.decode()? {
                Step::Value(v) if v.is_push() => continue,
                Step::Value(v) => return Ok(v),
                Step::Incomplete => {}
            }
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                return Err(CallError::TruncatedReply);
            }
            self.decoder.feed(&buf[..n]);
        }
    }
}

/// Encode argv as a RESP2 array of bulk strings — **exactly** the bytes given.
///
/// No case folding, no quoting, no trimming. Layer 1 of the differential comparison is "did
/// the same bytes go out", and the only way to answer it is for this function to be the sole
/// place that decides.
#[must_use]
pub fn encode_command(argv: &[Bytes]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + argv.iter().map(|a| a.len() + 16).sum::<usize>());
    out.extend_from_slice(format!("*{}\r\n", argv.len()).as_bytes());
    for a in argv {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a);
        out.extend_from_slice(b"\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_bytes_go_out_untouched() {
        // CMD-01 and CMD-02 in one assertion: a value that looks like a flag stays a value,
        // and a mixed-case key keeps its case.
        let argv = [
            Bytes::from_static(b"SET"),
            Bytes::from_static(b"MixedCase"),
            Bytes::from_static(b"--raw"),
        ];
        assert_eq!(
            encode_command(&argv),
            b"*3\r\n$3\r\nSET\r\n$9\r\nMixedCase\r\n$5\r\n--raw\r\n".to_vec()
        );
    }

    #[test]
    fn a_nul_byte_and_a_newline_survive_encoding() {
        let argv = [
            Bytes::from_static(b"SET"),
            Bytes::from_static(b"k"),
            Bytes::from_static(b"a\x00b\r\nc"),
        ];
        let wire = encode_command(&argv);
        assert!(wire.ends_with(b"$6\r\na\x00b\r\nc\r\n"), "{wire:?}");
    }

    #[test]
    fn an_empty_argument_is_a_zero_length_bulk_not_an_omission() {
        let argv = [Bytes::from_static(b"SET"), Bytes::from_static(b"")];
        assert_eq!(
            encode_command(&argv),
            b"*2\r\n$3\r\nSET\r\n$0\r\n\r\n".to_vec()
        );
    }
}
