//! V-A01 — one PTY driver for macOS/Linux PTY and Windows `ConPTY`.
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
use std::sync::atomic::{AtomicUsize, Ordering};
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
    ///
    /// Carries what *did* arrive, escaped. A timeout that only says what it wanted is close
    /// to useless on a CI runner you cannot attach to: the interesting question is always
    /// whether the child printed something else, or nothing at all.
    #[error(
        "timed out after {waited:?} waiting for {needle:?}; received {received} byte(s): {got}"
    )]
    Timeout {
        /// How long the wait lasted.
        waited: Duration,
        /// What it was waiting for.
        needle: String,
        /// How many bytes arrived in total.
        received: usize,
        /// Those bytes, escaped and truncated.
        got: String,
    },
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
    /// A reply the harness itself sent on the terminal's behalf.
    ///
    /// A real terminal answers queries such as Device Status Report; a harness that does not
    /// is not a terminal, and a program waiting for the answer simply stops. Recorded
    /// separately from [`Event::Input`] so a golden makes clear which bytes the test sent and
    /// which the harness did.
    AutoReply {
        /// Milliseconds since session start.
        at_ms: u64,
        /// Bytes, escaped for readability.
        bytes: String,
        /// What was being answered, e.g. `DSR`.
        query: String,
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
                Event::Input { bytes, .. } => G {
                    kind: "in",
                    data: bytes.clone(),
                },
                Event::Output { bytes, .. } => G {
                    kind: "out",
                    data: bytes.clone(),
                },
                Event::AutoReply { bytes, query, .. } => G {
                    kind: "auto",
                    data: format!("{query}:{bytes}"),
                },
                Event::Resize { cols, rows, .. } => G {
                    kind: "resize",
                    data: format!("{cols}x{rows}"),
                },
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
            _ => {
                use std::fmt::Write as _;
                let _ = write!(s, "\\x{c:02x}");
            }
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
    /// Shared with the reader thread, which answers terminal queries on our behalf.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    buf: Arc<Mutex<Vec<u8>>>,
    events: Arc<Mutex<Vec<Event>>>,
    start: Instant,
    cols: u16,
    rows: u16,
    auto_replies: Arc<AtomicUsize>,
}

/// Device Status Report — "where is the cursor?".
///
/// ConPTY sends this as it starts and **blocks** until the terminal answers. A harness that
/// ignores it never sees a single byte of the child's output, which is exactly how the first
/// Windows run of the V-C02 tests failed: four bytes received, `ESC [ 6 n`, and then silence.
const DSR_CURSOR: &[u8] = b"\x1b[6n";

impl PtySession {
    /// Spawn `cmd` on a PTY of the given size.
    ///
    /// # Errors
    /// [`PtyError::Pty`] if the PTY cannot be opened or the child cannot be spawned.
    pub fn spawn(cmd: CommandBuilder, cols: u16, rows: u16) -> Result<Self, PtyError> {
        let sys = NativePtySystem::default();
        let pair = sys
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Pty(e.to_string()))?;
        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Pty(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::Pty(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Pty(e.to_string()))?;

        let buf = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(Vec::new()));
        let start = Instant::now();
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(writer));
        let auto_replies = Arc::new(AtomicUsize::new(0));

        let b2 = Arc::clone(&buf);
        let e2 = Arc::clone(&events);
        let w2 = Arc::clone(&writer);
        let a2 = Arc::clone(&auto_replies);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            // A query can be split across two reads, so the last few bytes of each chunk are
            // carried forward. Four is the length of the longest sequence answered here.
            let mut carry: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let at_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                        if let Ok(mut g) = b2.lock() {
                            g.extend_from_slice(&chunk[..n]);
                        }
                        if let Ok(mut g) = e2.lock() {
                            g.push(Event::Output {
                                at_ms,
                                bytes: escape(&chunk[..n]),
                            });
                        }

                        // Answer terminal queries. A harness that stays silent is not a
                        // terminal, and a program that waits for the answer simply stops.
                        let mut scan = std::mem::take(&mut carry);
                        scan.extend_from_slice(&chunk[..n]);
                        let queries = scan
                            .windows(DSR_CURSOR.len())
                            .filter(|w| *w == DSR_CURSOR)
                            .count();
                        for _ in 0..queries {
                            // Row 1, column 1. Nothing here tracks a real cursor; what the
                            // asking program needs is a well-formed answer, promptly.
                            let reply = b"\x1b[1;1R";
                            if let Ok(mut w) = w2.lock() {
                                let _ = w.write_all(reply);
                                let _ = w.flush();
                            }
                            a2.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut g) = e2.lock() {
                                g.push(Event::AutoReply {
                                    at_ms,
                                    bytes: escape(reply),
                                    query: "DSR".to_owned(),
                                });
                            }
                        }
                        let keep = scan.len().saturating_sub(DSR_CURSOR.len() - 1);
                        carry = scan[keep..].to_vec();
                    }
                }
            }
        });

        Ok(Self {
            pair,
            writer,
            buf,
            events,
            start,
            cols,
            rows,
            auto_replies,
        })
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Write bytes to the child's stdin.
    ///
    /// # Errors
    /// Propagates the write error.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        {
            let mut w = self
                .writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            w.write_all(bytes)?;
            w.flush()?;
        }
        let at_ms = self.now_ms();
        if let Ok(mut g) = self.events.lock() {
            g.push(Event::Input {
                at_ms,
                bytes: escape(bytes),
            });
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
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
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
                .is_ok_and(|g| g.windows(needle.len()).any(|w| w == needle))
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let buf = self.buf.lock().map(|g| g.clone()).unwrap_or_default();
        Err(PtyError::Timeout {
            waited: timeout,
            needle: String::from_utf8_lossy(needle).into_owned(),
            received: buf.len(),
            got: escape(&buf[..buf.len().min(4096)]),
        })
    }

    /// Current size.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// How many terminal queries the harness has answered on the child's behalf.
    ///
    /// Zero on a session where nothing asked; on Windows, ConPTY asks as it starts, so a zero
    /// here alongside a stalled child is the signature of the failure this exists to prevent.
    #[must_use]
    pub fn auto_replies(&self) -> usize {
        self.auto_replies.load(Ordering::Relaxed)
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
        // A CI runner may have no TERM and no terminfo database, so tests read the window
        // size from termios via `stty` rather than from `tput`.
        c.env("TERM", "xterm-256color");
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
        // `stty size` prints "<rows> <cols>" straight from termios.
        let s = PtySession::spawn(sh("stty size"), 100, 30).unwrap();
        s.wait_for(b"30 100", Duration::from_secs(10)).unwrap();
        assert_eq!(s.size(), (100, 30));
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn resize_is_visible_to_the_child() {
        // UX-07 / ASSIST-067: resize must actually reach the process under test.
        let mut s = PtySession::spawn(sh("sleep 0.5; stty size"), 80, 24).unwrap();
        s.resize(40, 12).unwrap();
        s.wait_for(b"12 40", Duration::from_secs(10)).unwrap();
        assert_eq!(s.size(), (40, 12));
        assert!(
            s.recording()
                .events
                .iter()
                .any(|e| matches!(e, Event::Resize { cols: 40, .. }))
        );
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
        assert!(
            input.contains("\\x1b[200~"),
            "paste start marker recorded: {input}"
        );
        assert!(
            input.contains("\\x1b[201~"),
            "paste end marker recorded: {input}"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn a_cursor_position_query_is_answered() {
        // The harness has to behave like a terminal. ConPTY asks this as it starts and blocks
        // until it is answered — an unanswered query means the child's output never arrives,
        // which is how every V-C02 PTY test failed on the first Windows run.
        let mut s = PtySession::spawn(
            sh("printf '\\033[6n'; sleep 0.4; printf 'after-query'"),
            80,
            24,
        )
        .unwrap();
        s.wait_for(b"after-query", Duration::from_secs(5)).unwrap();
        assert_eq!(s.auto_replies(), 1, "the query was answered exactly once");

        // The reply is recorded, and marked as ours rather than as test input.
        let rec = s.recording();
        let replies: Vec<&Event> = rec
            .events
            .iter()
            .filter(|e| matches!(e, Event::AutoReply { .. }))
            .collect();
        assert_eq!(replies.len(), 1);
        match replies[0] {
            Event::AutoReply { query, bytes, .. } => {
                assert_eq!(query, "DSR");
                assert!(bytes.contains("1;1R"), "the answer is well formed: {bytes}");
            }
            other => panic!("expected an auto reply, got {other:?}"),
        }
        let golden = rec.to_golden().unwrap();
        assert!(golden.contains(r#""kind": "auto""#), "{golden}");
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn a_session_with_no_queries_sends_no_replies() {
        let s = PtySession::spawn(sh("printf 'quiet'"), 80, 24).unwrap();
        s.wait_for(b"quiet", Duration::from_secs(5)).unwrap();
        assert_eq!(s.auto_replies(), 0, "nothing asked, so nothing was sent");
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn a_query_split_across_two_reads_is_still_answered() {
        // The sequence is four bytes; a read boundary can fall anywhere inside it.
        let mut s = PtySession::spawn(
            sh("printf '\\033['; sleep 0.3; printf '6n'; sleep 0.4; printf 'done'"),
            80,
            24,
        )
        .unwrap();
        s.wait_for(b"done", Duration::from_secs(5)).unwrap();
        assert_eq!(
            s.auto_replies(),
            1,
            "a query split across reads must still be recognised"
        );
    }

    #[test]
    #[cfg_attr(windows, ignore = "sh is not available; V-C07 covers Windows")]
    fn wait_for_times_out_with_the_needle_and_what_arrived() {
        let s = PtySession::spawn(sh("printf 'x'"), 80, 24).unwrap();
        let e = s
            .wait_for(b"never-appears", Duration::from_millis(300))
            .unwrap_err();
        match e {
            PtyError::Timeout {
                needle,
                received,
                got,
                ..
            } => {
                assert_eq!(needle, "never-appears");
                // The output that *did* arrive is what makes a CI failure diagnosable.
                assert!(received >= 1, "the child did print something");
                assert!(got.contains('x'), "and the timeout says so: {got:?}");
            }
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
        assert!(
            !g1.contains('\x1b'),
            "golden file must never carry a live escape"
        );
        assert!(rec.platform.contains(std::env::consts::OS));
    }
}
