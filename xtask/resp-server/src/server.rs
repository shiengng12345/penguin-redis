//! Tokio TCP server that plays a [`Script`] against one connection.

use crate::encode::{Proto, to_vec};
use crate::inbound::{InboundError, parse_command};
use crate::script::{Script, Step};
use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Errors while running a script.
#[derive(Debug, Error)]
pub enum ServeError {
    /// I/O.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Encoding.
    #[error("encode: {0}")]
    Encode(#[from] crate::encode::EncodeError),
    /// Client sent something that is not a command.
    #[error("inbound: {0}")]
    Inbound(#[from] InboundError),
    /// `Expect` mismatch.
    #[error("expected argv {expected:?}, got {got:?}")]
    Mismatch {
        /// Expected argv.
        expected: Vec<Vec<u8>>,
        /// Received argv.
        got: Vec<Vec<u8>>,
    },
    /// Client closed before the script finished.
    #[error("client disconnected at step {0}")]
    Disconnected(usize),
}

/// A bound synthetic server.
pub struct SyntheticServer {
    listener: TcpListener,
    proto: Proto,
}

impl SyntheticServer {
    /// Bind on `127.0.0.1:0` (ephemeral port).
    ///
    /// # Errors
    /// Propagates the bind error.
    pub async fn bind(proto: Proto) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        Ok(Self { listener, proto })
    }

    /// Local address to connect to.
    ///
    /// # Errors
    /// Propagates the `local_addr` error.
    pub fn addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept one connection and play `script` on it. Returns the commands received.
    ///
    /// # Errors
    /// [`ServeError`] on I/O failure, encoding failure, malformed client input,
    /// an `Expect` mismatch, or an early client disconnect.
    pub async fn run_once(&self, script: &Script) -> Result<Vec<Vec<Vec<u8>>>, ServeError> {
        let (stream, _) = self.listener.accept().await?;
        play(stream, self.proto, script).await
    }
}

async fn read_one_command(stream: &mut TcpStream, buf: &mut Vec<u8>, step_idx: usize) -> Result<Vec<Vec<u8>>, ServeError> {
    loop {
        match parse_command(buf) {
            Ok((cmd, used)) => {
                buf.drain(..used);
                return Ok(cmd.0.into_iter().map(|b| b.to_vec()).collect());
            }
            Err(InboundError::Incomplete) => {
                let mut tmp = [0u8; 4096];
                let n = stream.read(&mut tmp).await?;
                if n == 0 {
                    return Err(ServeError::Disconnected(step_idx));
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn play(mut stream: TcpStream, proto: Proto, script: &Script) -> Result<Vec<Vec<Vec<u8>>>, ServeError> {
    stream.set_nodelay(true)?;
    let mut inbuf: Vec<u8> = Vec::new();
    let mut received = Vec::new();
    for (i, step) in script.steps.iter().enumerate() {
        match step {
            Step::Expect(expected) => {
                let got = read_one_command(&mut stream, &mut inbuf, i).await?;
                if !expected.is_empty() && &got != expected {
                    return Err(ServeError::Mismatch { expected: expected.clone(), got });
                }
                received.push(got);
            }
            Step::ReadCommand => {
                let got = read_one_command(&mut stream, &mut inbuf, i).await?;
                received.push(got);
            }
            Step::Send(f) => {
                let bytes = to_vec(f, proto)?;
                stream.write_all(&bytes).await?;
            }
            Step::SendRaw(b) => stream.write_all(b).await?,
            Step::SendChunked { bytes, chunk, delay_ms } => {
                let chunk = (*chunk).max(1);
                for piece in bytes.chunks(chunk) {
                    stream.write_all(piece).await?;
                    stream.flush().await?;
                    if *delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
                    }
                }
            }
            Step::SplitAt { bytes, at, delay_ms } => {
                let mut prev = 0usize;
                for &cut in at {
                    let cut = cut.min(bytes.len());
                    if cut > prev {
                        stream.write_all(&bytes[prev..cut]).await?;
                        stream.flush().await?;
                        if *delay_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
                        }
                        prev = cut;
                    }
                }
                if prev < bytes.len() {
                    stream.write_all(&bytes[prev..]).await?;
                }
            }
            Step::BlobStream { len, chunk, fill } => {
                stream.write_all(format!("${len}\r\n").as_bytes()).await?;
                let chunk = (*chunk).max(1);
                let block = vec![*fill; chunk];
                let mut remaining = *len;
                let chunk_u64 = u64::try_from(chunk).unwrap_or(u64::MAX);
                while remaining > 0 {
                    let n = usize::try_from(remaining.min(chunk_u64)).unwrap_or(chunk);
                    stream.write_all(&block[..n]).await?;
                    remaining -= u64::try_from(n).unwrap_or(0);
                }
                stream.write_all(b"\r\n").await?;
            }
            Step::TruncatedBulk { declared, actual } => {
                stream.write_all(format!("${declared}\r\n").as_bytes()).await?;
                let block = vec![b'x'; *actual];
                stream.write_all(&block).await?;
                stream.shutdown().await?;
                return Ok(received);
            }
            Step::Delay(ms) => tokio::time::sleep(Duration::from_millis(*ms)).await,
            Step::Close => {
                stream.shutdown().await?;
                return Ok(received);
            }
            Step::Hang => {
                // keep the socket open until the client goes away
                let mut tmp = [0u8; 1024];
                loop {
                    let n = stream.read(&mut tmp).await?;
                    if n == 0 {
                        return Ok(received);
                    }
                }
            }
        }
    }
    stream.flush().await?;
    Ok(received)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::frame::Frame;

    async fn client_roundtrip(addr: SocketAddr, send: &[u8], expect_len: usize) -> Vec<u8> {
        let mut c = TcpStream::connect(addr).await.unwrap();
        c.write_all(send).await.unwrap();
        let mut out = Vec::new();
        let mut tmp = vec![0u8; 65536];
        while out.len() < expect_len {
            let n = c.read(&mut tmp).await.unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&tmp[..n]);
        }
        out
    }

    #[tokio::test]
    async fn expect_and_reply() {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        let addr = srv.addr().unwrap();
        let script = Script::new()
            .then(Step::Expect(vec![b"PING".to_vec()]))
            .then(Step::Send(Frame::simple("PONG")));
        let server = tokio::spawn(async move { srv.run_once(&script).await });
        let got = client_roundtrip(addr, b"*1\r\n$4\r\nPING\r\n", 7).await;
        assert_eq!(got, b"+PONG\r\n");
        let received = server.await.unwrap().unwrap();
        assert_eq!(received, vec![vec![b"PING".to_vec()]]);
    }

    #[tokio::test]
    async fn expect_mismatch_is_error() {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        let addr = srv.addr().unwrap();
        let script = Script::new().then(Step::Expect(vec![b"PING".to_vec()]));
        let server = tokio::spawn(async move { srv.run_once(&script).await });
        let _ = client_roundtrip(addr, b"*1\r\n$4\r\nQUIT\r\n", 0).await;
        assert!(matches!(server.await.unwrap(), Err(ServeError::Mismatch { .. })));
    }

    #[tokio::test]
    async fn split_at_delivers_all_bytes() {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        let addr = srv.addr().unwrap();
        let payload = b"$5\r\nh\xc3\xa9llo\r\n".to_vec(); // split inside the UTF-8 sequence
        let script = Script::new()
            .then(Step::ReadCommand)
            .then(Step::SplitAt { bytes: payload.clone(), at: vec![5, 6], delay_ms: 1 });
        let server = tokio::spawn(async move { srv.run_once(&script).await });
        let got = client_roundtrip(addr, b"PING\r\n", payload.len()).await;
        assert_eq!(got, payload);
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn blob_stream_declares_and_sends_exact_length() {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        let addr = srv.addr().unwrap();
        let len = 100_003u64;
        let script = Script::new()
            .then(Step::ReadCommand)
            .then(Step::BlobStream { len, chunk: 4096, fill: b'z' });
        let server = tokio::spawn(async move { srv.run_once(&script).await });
        let header = format!("${len}\r\n");
        let len_usize = usize::try_from(len).unwrap();
        let total = header.len() + len_usize + 2;
        let got = client_roundtrip(addr, b"PING\r\n", total).await;
        assert_eq!(got.len(), total);
        assert!(got.starts_with(header.as_bytes()));
        assert!(got.ends_with(b"\r\n"));
        assert!(got[header.len()..header.len() + len_usize].iter().all(|b| *b == b'z'));
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn truncated_bulk_closes_early() {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        let addr = srv.addr().unwrap();
        let script = Script::new()
            .then(Step::ReadCommand)
            .then(Step::TruncatedBulk { declared: 1_000_000, actual: 10 });
        let server = tokio::spawn(async move { srv.run_once(&script).await });
        let got = client_roundtrip(addr, b"PING\r\n", usize::MAX).await;
        assert_eq!(got, b"$1000000\r\nxxxxxxxxxx".to_vec());
        server.await.unwrap().unwrap();
    }
}
