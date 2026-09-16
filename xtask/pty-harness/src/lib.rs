//! V-A01 — one PTY driver for macOS/Linux PTY and Windows ConPTY.
//!
//! Every terminal claim in v2.1 (§14 input contract, §7.3 colour, §6.2 width, §35.1 Windows)
//! has to be checked against a *real* terminal, not a component snapshot. `portable-pty`
//! gives the same API over both backends, so the same fixture runs on all three platforms
//! (V-C07, WIN-01).
//!
//! The harness records every input event and output byte with a timestamp so a run can be
//! replayed and diffed — that is what makes a terminal regression reviewable rather than
//! "it looked fine on my machine".

use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Harness errors.
#[derive(Debug, Error)]
pub enum PtyError {
    /// Failed to open or drive the PTY.
    #[error("pty: {0}")]
    Pty(String),
    /// I/O against the PTY.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Waited for output that never arrived.
    #[error("timed out after {0:?} waiting for {1:?}")]
    Timeout(Duration, String),
    /// Serialising a recording.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// One recorded event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Bytes written to the child's stdin.
    Input {
        /// Milliseconds since session start.
        at_ms: u64,
        /// Bytes, escaped for readability.
        bytes: String,
    },
    /// Bytes read from the child's stdout.
    Output {
        /// Milliseconds since session start.
        at_ms: u64,
        /// Bytes, escaped for readability.
        bytes: String,
    },
    /// Terminal was resized.
    Resize {
        /// Milliseconds since session start.
        at_ms: u64,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
}

/// A recorded session, suitable for golden-diffing.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recording {
    /// Ordered events.
    pub events: Vec<Event>,
    /// Platform the recording was made on.
    pub platform: String,
}

impl Recording {
    /// All output bytes concatenated, unescaped.
    #[must_use]
    pub fn output_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for e in &self.events {
            if let Event::Output { bytes, .. } = e {
                out.extend_from_slice(&unescape(bytes));
            }
        }
        out
    }

    /// Serialise for a golden file. Timestamps are dropped so a diff is deterministic.
    ///
    /// # Errors
    /// Propagates serialisation failure.
    pub fn to_golden(&self) -> Result<String, PtyError> {
        #[derive(Serialize)]
        struct G<'a> {
            kind: &'a str,
            data: String,
        }
        let g: Vec<G<'_>> = self
            .events
            .iter()
            .map(|e| match e {
                Event::Input { bytes, .. } => G { kind: "in", data: bytes.clone() },
                Event::Output { bytes, .. } => G { kind: "out", data: bytes.clone() },
                Event::Resize { cols, rows, .. } => G { kind: "resize", data: format!("{cols}x{rows}") },
            })
            .collect();
        Ok(serde_json::to_string_pretty(&g)?)
    }
}

/// Escape bytes so a recording stays readable and diffable, and so an ESC in a golden file
/// can never drive the terminal of whoever reviews it.
#[must_use]
pub fn escape(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len());
    for &c in b {
        match c {
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(c as char),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s
}

/// Inverse of [`escape`].
#[must_use]
pub fn unescape(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            match b[i + 1] {
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'x' if i + 3 < b.len() => {
                    let h = std::str::from_utf8(&b[i + 2..i + 4]).unwrap_or("00");
                    out.push(u8::from_str_radix(h, 16).unwrap_or(0));
                    i += 4;
                }
                _ => {
                    out.push(b[i]);
                    i += 1;
                }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// A live PTY session running a child process.
pub struct PtySession {
    pair: PtyPair,
    writer: Box<dyn Write + Send>,
    buf: Arc<Mutex<Vec<u8>>>,
    events: Arc<Mutex<Vec<Event>>>,
    start: Instant,
    cols: u16,
    rows: u16,
}

impl PtySession {
    /// Spawn `cmd` on a PTY of the given size.
    ///
    /// # Errors
    /// [`PtyError::Pty`] if the PTY cannot be opened or the child cannot be spawned.
    pub fn spawn(cmd: CommandBuilder, cols: u16, rows: u16) -> Result<Self, PtyError> {
        let sys = NativePtySystem::default();
        let pair = sys
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| PtyError::Pty(e.to_string()))?;
        let _child = pair.slave.spawn_command(cmd).map_err(|e| PtyError::Pty(e.to_string()))?;
        let writer = pair.master.take_writer().map_err(|e| PtyError::Pty(e.to_string()))?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| PtyError::Pty(e.to_string()))?;

        let buf = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(Vec::new()));
        let start = Instant::now();

        let b2 = Arc::clone(&buf);
        let e2 = Arc::clone(&events);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let at_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                        if let Ok(mut g) = b2.lock() {
                            g.extend_from_slice(&chunk[..n]);
                        }
                        if let Ok(mut g) = e2.lock() {
                            g.push(Event::Output { at_ms, bytes: escape(&chunk[..n]) });
                        }
                    }
                }
            }
        });

        Ok(Self { pair, writer, buf, events, start, cols, rows })
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Write bytes to the child's stdin.
    ///
    /// # Errors
    /// Propagates the write error.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        let at_ms = self.now_ms();
        if let Ok(mut g) = self.events.lock() {
            g.push(Event::Input { at_ms, bytes: escape(bytes) });
        }
        Ok(())
    }

    /// Send a bracketed paste: `ESC[200~ <text> ESC[201~` (v2.1 §14.1, ASSIST-082).
    ///
    /// # Errors
    /// Propagates the write error.
    pub fn send_bracketed_paste(&mut self, text: &[u8]) -> Result<(), PtyError> {
        let mut b = Vec::with_capacity(text.len() + 12);
        b.extend_from_slice(b"\x1b[200~");
        b.extend_from_slice(text);
        b.extend_from_slice(b"\x1b[201~");
        self.send(&b)
    }

    /// Resize the terminal.
    ///
    /// # Errors
    /// [`PtyError::Pty`] if the resize fails.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.pair
            .master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| PtyError::Pty(e.to_string()))?;
        self.cols = cols;
        self.rows = rows;
        let at_ms = self.now_ms();
        if let Ok(mut g) = self.events.lock() {
            g.push(Event::Resize { at_ms, cols, rows });
        }
        Ok(())
    }

    /// Everything read so far.
    #[must_use]
    pub fn output(&self) -> Vec<u8> {
        self.buf.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Block until `needle` appears in the output, or time out.
    ///
    /// # Errors
    /// [`PtyError::Timeout`] with the needle, so a failing test says what it was waiting for.
    pub fn wait_for(&self, needle: &[u8], timeout: Duration) -> Result<(), PtyError> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self
                .buf
                .lock()
                .map(|g| g.windows(needle.len()).any(|w| w == needle))
                .unwrap_or(false)
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(PtyError::Timeout(timeout, String::from_utf8_lossy(needle).into_owned()))
    }

    /// Current size.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Take the recording.
    #[must_use]
    pub fn recording(&self) -> Recording {
        Recording {
            events: self.events.lock().map(|g| g.clone()).unwrap_or_default(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn sh(script: &str) -> CommandBuilder {
        // `sh` exists on macOS and Linux; the Windows path is exercised by V-C07 with cmd.exe.
        let mut c = CommandBuilder::new("sh");
        c.arg("-c");
        c.arg(script);
        c
    }

    #[test]
    fn escape_round_trips_every_byte() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(unescape(&escape(&all)), all);
        // and the escaped form is safe to print
        let e = escape(&all);
        assert!(!e.contains('\x1b'), "escaped form must not contain ESC");
        assert!(!e.contains('\x07'));
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn captures_child_output() {
        let mut s = PtySession::spawn(sh("printf 'hello-pty'"), 80, 24).unwrap();
        s.wait_for(b"hello-pty", Duration::from_secs(5)).unwrap();
        assert!(s.output().windows(9).any(|w| w == b"hello-pty"));
        let _ = &mut s;
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn sends_input_and_reads_the_echo() {
        let mut s = PtySession::spawn(sh("read line; printf 'got:%s' \"$line\""), 80, 24).unwrap();
        s.send(b"penguin\n").unwrap();
        s.wait_for(b"got:penguin", Duration::from_secs(5)).unwrap();
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn reports_terminal_size_to_the_child() {
        let mut s = PtySession::spawn(sh("printf 'cols=%s' \"$(tput cols)\""), 100, 30).unwrap();
        s.wait_for(b"cols=100", Duration::from_secs(5)).unwrap();
        assert_eq!(s.size(), (100, 30));
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn resize_is_visible_to_the_child() {
        // UX-07 / ASSIST-067: resize must actually reach the process under test.
        let mut s = PtySession::spawn(sh("sleep 0.3; printf 'cols=%s' \"$(tput cols)\""), 80, 24).unwrap();
        s.resize(40, 12).unwrap();
        s.wait_for(b"cols=40", Duration::from_secs(5)).unwrap();
        assert_eq!(s.size(), (40, 12));
        assert!(s.recording().events.iter().any(|e| matches!(e, Event::Resize { cols: 40, .. })));
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn bracketed_paste_markers_reach_the_child() {
        // ASSIST-082: the harness must be able to produce a real bracketed paste.
        let mut s = PtySession::spawn(sh("cat"), 80, 24).unwrap();
        s.send_bracketed_paste(b"SET a 1\nSET b 2\n").unwrap();
        s.wait_for(b"SET b 2", Duration::from_secs(5)).unwrap();
        let rec = s.recording();
        let input: String = rec
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Input { bytes, .. } => Some(bytes.clone()),
                _ => None,
            })
            .collect();
        assert!(input.contains("\\x1b[200~"), "paste start marker recorded: {input}");
        assert!(input.contains("\\x1b[201~"), "paste end marker recorded: {input}");
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn wait_for_times_out_with_the_needle_named() {
        let s = PtySession::spawn(sh("printf 'x'"), 80, 24).unwrap();
        let e = s.wait_for(b"never-appears", Duration::from_millis(300)).unwrap_err();
        match e {
            PtyError::Timeout(_, n) => assert_eq!(n, "never-appears"),
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn recording_is_replayable_and_golden_stable() {
        let mut s = PtySession::spawn(sh("read l; printf 'ok:%s' \"$l\""), 80, 24).unwrap();
        s.send(b"z\n").unwrap();
        s.wait_for(b"ok:z", Duration::from_secs(5)).unwrap();
        let rec = s.recording();
        assert!(rec.output_bytes().windows(4).any(|w| w == b"ok:z"));
        let g1 = rec.to_golden().unwrap();
        let g2 = rec.to_golden().unwrap();
        assert_eq!(g1, g2, "golden form must be deterministic");
        assert!(!g1.contains('\x1b'), "golden file must never carry a live escape");
        assert!(rec.platform.contains(std::env::consts::OS));
    }
}
