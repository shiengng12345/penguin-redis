//! A recording TCP proxy — layers 1 and 2 of the §32.2 comparison.
//!
//! Layer 1 is "did the same argv bytes go out" and layer 2 is "did the same reply bytes come
//! back". Neither question can be answered by looking at what a client *prints*, so the
//! harness puts itself on the wire and records both directions verbatim.
//!
//! `MONITOR` would have been easier and would have been wrong: it shows the command after the
//! server has re-quoted it, which is a rendering of the argv rather than the argv.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;

/// What one client connection sent and received, verbatim.
#[derive(Clone, Debug, Default)]
pub struct Recording {
    /// Every byte the client sent, in order.
    pub to_server: Vec<u8>,
    /// Every byte the server sent, in order.
    pub to_client: Vec<u8>,
}

/// A proxy that accepts exactly the connections one client run makes, records them, and stops.
#[derive(Debug)]
pub struct RecordingProxy {
    port: u16,
    handle: JoinHandle<Vec<Recording>>,
    stop: mpsc::Sender<()>,
}

impl RecordingProxy {
    /// Listen on an ephemeral port and forward to `upstream`.
    ///
    /// # Errors
    /// Propagates the listen failure.
    pub fn start(upstream: String) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let (stop, stopped) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let mut out = Vec::new();
            loop {
                match listener.accept() {
                    Ok((client, _)) => {
                        let _ = client.set_nonblocking(false);
                        if let Ok(rec) = pump(client, &upstream) {
                            out.push(rec);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if stopped.try_recv().is_ok() {
                            return out;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    Err(_) => return out,
                }
            }
        });

        Ok(Self { port, handle, stop })
    }

    /// The address a client should connect to.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop accepting and return everything recorded.
    ///
    /// # Errors
    /// Returns the recordings gathered so far even if the thread panicked, because a lost
    /// recording is a lost layer of the comparison and silence would look like agreement.
    #[must_use]
    pub fn finish(self) -> Vec<Recording> {
        let _ = self.stop.send(());
        self.handle.join().unwrap_or_default()
    }
}

/// Copy in both directions until either side closes, recording everything.
fn pump(client: TcpStream, upstream: &str) -> std::io::Result<Recording> {
    let server = TcpStream::connect(upstream)?;
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;

    let (c_read, c_write) = (client.try_clone()?, client);
    let (s_read, s_write) = (server.try_clone()?, server);

    let up = std::thread::spawn(move || copy_recording(c_read, s_write));
    let down = copy_recording(s_read, c_write);
    let to_server = up.join().unwrap_or_default();

    Ok(Recording {
        to_server,
        to_client: down,
    })
}

fn copy_recording(mut from: TcpStream, mut to: TcpStream) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                seen.extend_from_slice(&buf[..n]);
                if to.write_all(&buf[..n]).is_err() || to.flush().is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown(Shutdown::Write);
    seen
}
