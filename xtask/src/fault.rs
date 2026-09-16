//! V-A04 — fault injection: a programmable TCP proxy plus local filesystem faults.
//!
//! Every failure path in v2.1 (§22 cancel/reconnect, §30.5 disk, STATE-01 unknown-after-send,
//! NET-01 TLS) needs a *repeatable* way to produce the fault. This module is that harness.
//!
//! The proxy sits between the client under test and any upstream (including the synthetic
//! RESP server) and can cut, stall, truncate or corrupt the stream at a chosen point.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What the proxy should do to the connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Pass everything through untouched (control case).
    None,
    /// Forward `after_bytes` from the client, then drop the connection without a FIN.
    /// This is how STATE-01 is produced: the command may or may not have been applied.
    CutAfterClientBytes {
        /// Bytes to forward before cutting.
        after_bytes: u64,
    },
    /// Forward `after_bytes` from the server, then drop.
    CutAfterServerBytes {
        /// Bytes to forward before cutting.
        after_bytes: u64,
    },
    /// Accept the connection, forward nothing, never close — produces a read timeout.
    Blackhole,
    /// Forward, but delay every server chunk.
    SlowServer {
        /// Delay per chunk.
        delay_ms: u64,
    },
    /// Forward server bytes in tiny pieces, to exercise incremental decoding.
    FragmentServer {
        /// Bytes per write.
        chunk: usize,
    },
    /// Flip one bit in the server stream at a byte offset, to exercise protocol errors.
    CorruptServerByte {
        /// Offset in the server stream.
        at: u64,
    },
    /// Refuse to accept at all: the listener closes immediately (connection refused-ish).
    RefuseConnection,
}

/// Counters the test can assert on.
#[derive(Clone, Debug, Default)]
pub struct ProxyStats {
    client_to_server: Arc<AtomicU64>,
    server_to_client: Arc<AtomicU64>,
    connections: Arc<AtomicU64>,
}

impl ProxyStats {
    /// Bytes forwarded from client to server.
    #[must_use]
    pub fn client_bytes(&self) -> u64 {
        self.client_to_server.load(Ordering::SeqCst)
    }
    /// Bytes forwarded from server to client.
    #[must_use]
    pub fn server_bytes(&self) -> u64 {
        self.server_to_client.load(Ordering::SeqCst)
    }
    /// Accepted connections.
    #[must_use]
    pub fn connections(&self) -> u64 {
        self.connections.load(Ordering::SeqCst)
    }
}

/// A fault-injecting TCP proxy.
pub struct FaultProxy {
    listener: TcpListener,
    upstream: SocketAddr,
    fault: Fault,
    stats: ProxyStats,
}

impl FaultProxy {
    /// Bind on an ephemeral loopback port in front of `upstream`.
    ///
    /// # Errors
    /// Propagates the bind error.
    pub async fn bind(upstream: SocketAddr, fault: Fault) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        Ok(Self { listener, upstream, fault, stats: ProxyStats::default() })
    }

    /// Address the client under test should connect to.
    ///
    /// # Errors
    /// Propagates the `local_addr` error.
    pub fn addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Counters.
    #[must_use]
    pub fn stats(&self) -> ProxyStats {
        self.stats.clone()
    }

    /// Serve exactly one connection with the configured fault.
    ///
    /// # Errors
    /// Propagates I/O errors other than the deliberate cuts.
    pub async fn run_once(&self) -> std::io::Result<()> {
        if self.fault == Fault::RefuseConnection {
            return Ok(()); // never accept; the client sees a timeout/refusal
        }
        let (client, _) = self.listener.accept().await?;
        self.stats.connections.fetch_add(1, Ordering::SeqCst);
        if self.fault == Fault::Blackhole {
            // Hold the socket open, forward nothing.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            return Ok(());
        }
        let server = TcpStream::connect(self.upstream).await?;
        self.pump(client, server).await
    }

    async fn pump(&self, client: TcpStream, server: TcpStream) -> std::io::Result<()> {
        let (mut cr, mut cw) = client.into_split();
        let (mut sr, mut sw) = server.into_split();
        let fault = self.fault;
        let c2s = Arc::clone(&self.stats.client_to_server);
        let s2c = Arc::clone(&self.stats.server_to_client);

        // client -> server
        let up = tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                let n = match cr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let total = c2s.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
                if let Fault::CutAfterClientBytes { after_bytes } = fault {
                    if total > after_bytes {
                        // Forward only the allowed prefix, then stop: the server may well have
                        // applied it, which is exactly the uncertainty STATE-01 describes.
                        let allowed = after_bytes.saturating_sub(total - n as u64);
                        let allowed = usize::try_from(allowed).unwrap_or(0).min(n);
                        let _ = sw.write_all(&buf[..allowed]).await;
                        let _ = sw.flush().await;
                        break;
                    }
                }
                if sw.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });

        // server -> client
        let down = tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let mut seen: u64 = 0;
            loop {
                let n = match sr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let mut out = buf[..n].to_vec();
                if let Fault::CorruptServerByte { at } = fault {
                    if at >= seen && at < seen + n as u64 {
                        let idx = usize::try_from(at - seen).unwrap_or(0);
                        out[idx] ^= 0xff;
                    }
                }
                let before = seen;
                seen += n as u64;
                s2c.fetch_add(n as u64, Ordering::SeqCst);

                if let Fault::CutAfterServerBytes { after_bytes } = fault {
                    if seen > after_bytes {
                        let allowed = usize::try_from(after_bytes.saturating_sub(before)).unwrap_or(0).min(out.len());
                        let _ = cw.write_all(&out[..allowed]).await;
                        let _ = cw.flush().await;
                        break;
                    }
                }
                let write_res = match fault {
                    Fault::FragmentServer { chunk } => {
                        let mut ok = true;
                        for piece in out.chunks(chunk.max(1)) {
                            if cw.write_all(piece).await.is_err() || cw.flush().await.is_err() {
                                ok = false;
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        ok
                    }
                    Fault::SlowServer { delay_ms } => {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        cw.write_all(&out).await.is_ok()
                    }
                    _ => cw.write_all(&out).await.is_ok(),
                };
                if !write_res {
                    break;
                }
            }
        });

        let _ = tokio::join!(up, down);
        Ok(())
    }
}

/// Local filesystem faults (v2.1 §30.5: disk full, read-only, permission revoked).
pub mod fs {
    use std::io;
    use std::path::Path;

    /// Make a path read-only.
    ///
    /// # Errors
    /// Propagates metadata/permission errors.
    pub fn make_read_only(p: &Path) -> io::Result<()> {
        let mut perm = std::fs::metadata(p)?.permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(p, perm)
    }

    /// Restore write permission.
    ///
    /// # Errors
    /// Propagates metadata/permission errors.
    pub fn make_writable(p: &Path) -> io::Result<()> {
        let mut perm = std::fs::metadata(p)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(p, perm)
    }

    /// Fill a file to `limit` bytes so the next write fails a size check — a portable stand-in
    /// for ENOSPC that does not require a real full filesystem.
    ///
    /// # Errors
    /// Propagates write errors.
    pub fn fill_to(p: &Path, limit: u64) -> io::Result<()> {
        let f = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(p)?;
        f.set_len(limit)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Trivial echo upstream so the proxy has something to talk to.
    async fn echo_upstream() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let l = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = l.local_addr().unwrap();
        let h = tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut b = vec![0u8; 4096];
                    loop {
                        match s.read(&mut b).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if s.write_all(&b[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });
        (addr, h)
    }

    async fn talk(addr: SocketAddr, msg: &[u8], want: usize) -> Vec<u8> {
        let mut c = TcpStream::connect(addr).await.unwrap();
        c.write_all(msg).await.unwrap();
        let mut out = Vec::new();
        let mut b = vec![0u8; 4096];
        while out.len() < want {
            match tokio::time::timeout(Duration::from_millis(500), c.read(&mut b)).await {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
                Ok(Ok(n)) => out.extend_from_slice(&b[..n]),
            }
        }
        out
    }

    #[tokio::test]
    async fn passthrough_is_lossless() {
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::None).await.unwrap();
        let addr = p.addr().unwrap();
        let stats = p.stats();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"hello world", 11).await;
        assert_eq!(got, b"hello world");
        assert_eq!(stats.connections(), 1);
        assert_eq!(stats.client_bytes(), 11);
    }

    #[tokio::test]
    async fn cut_after_client_bytes_truncates_the_request() {
        // STATE-01: the server receives a prefix; the client cannot know whether it applied.
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::CutAfterClientBytes { after_bytes: 4 }).await.unwrap();
        let addr = p.addr().unwrap();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"0123456789", 10).await;
        assert!(got.len() <= 4, "at most the allowed prefix comes back, got {got:?}");
    }

    #[tokio::test]
    async fn cut_after_server_bytes_truncates_the_reply() {
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::CutAfterServerBytes { after_bytes: 3 }).await.unwrap();
        let addr = p.addr().unwrap();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"0123456789", 10).await;
        assert_eq!(got.len(), 3, "reply cut mid-stream");
    }

    #[tokio::test]
    async fn fragment_server_preserves_all_bytes() {
        // Incremental decoding must survive arbitrary chunking (V-B02).
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::FragmentServer { chunk: 1 }).await.unwrap();
        let addr = p.addr().unwrap();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"abcdefgh", 8).await;
        assert_eq!(got, b"abcdefgh", "fragmentation must not lose or reorder bytes");
    }

    #[tokio::test]
    async fn corrupt_server_byte_flips_exactly_one() {
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::CorruptServerByte { at: 2 }).await.unwrap();
        let addr = p.addr().unwrap();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"abcd", 4).await;
        assert_eq!(got.len(), 4);
        assert_eq!(&got[..2], b"ab");
        assert_eq!(got[2], b'c' ^ 0xff);
        assert_eq!(got[3], b'd');
    }

    #[tokio::test]
    async fn blackhole_produces_a_timeout_not_an_error() {
        let (up, _u) = echo_upstream().await;
        let p = FaultProxy::bind(up, Fault::Blackhole).await.unwrap();
        let addr = p.addr().unwrap();
        tokio::spawn(async move { p.run_once().await });
        let got = talk(addr, b"ping", 4).await;
        assert!(got.is_empty(), "blackhole must stall, not reply");
    }

    #[test]
    fn fs_read_only_blocks_writes_and_is_reversible() {
        let dir = std::env::temp_dir().join(format!("pr-fault-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.toml");
        std::fs::write(&f, b"a").unwrap();
        fs::make_read_only(&f).unwrap();
        assert!(std::fs::write(&f, b"b").is_err(), "read-only must reject writes");
        fs::make_writable(&f).unwrap();
        std::fs::write(&f, b"b").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"b");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fs_fill_to_sets_the_size() {
        let dir = std::env::temp_dir().join(format!("pr-fault-fill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("spool");
        fs::fill_to(&f, 4096).unwrap();
        assert_eq!(std::fs::metadata(&f).unwrap().len(), 4096);
        std::fs::remove_dir_all(&dir).ok();
    }
}
