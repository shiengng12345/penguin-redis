//! The single painter (v2.1 §14.4, ADR-022, V-C02).
//!
//! Every byte that reaches the terminal goes through a [`Painter`], and a `Painter` can only
//! be made from an [`Ownership`](crate::Ownership) token plus an exclusive borrow of the
//! sink. Those two together are the enforcement: the token means no other subsystem is
//! painting, and the `&mut` means nothing else can be mid-write.
//!
//! The escape sequences are deliberately in one place. Spreading them across the editor, the
//! menu and the TUI is how a cursor ends up one row from where the next writer assumes it is.

use std::io::Write;

/// Somewhere painted bytes go.
pub trait Sink {
    /// Append bytes.
    fn put(&mut self, bytes: &[u8]);
    /// Make them visible.
    fn flush(&mut self);
}

/// A sink that records everything, for tests and for the recording in the PTY harness.
#[derive(Debug, Default, Clone)]
pub struct Capture {
    /// Everything written, in order.
    pub bytes: Vec<u8>,
    /// How many times the painter flushed, which is one per coherent frame.
    pub flushes: usize,
}

impl Capture {
    /// An empty capture.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Everything written so far, lossily as text.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
    /// Forget what has been written, keeping the flush count.
    pub fn clear(&mut self) {
        self.bytes.clear();
    }
}

impl Sink for Capture {
    fn put(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn flush(&mut self) {
        self.flushes += 1;
    }
}

/// The real sink: the process's stdout, locked for the duration of a frame.
pub struct Stdout {
    out: std::io::Stdout,
}

impl Stdout {
    /// Wrap the process stdout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            out: std::io::stdout(),
        }
    }
}

impl Default for Stdout {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for Stdout {
    fn put(&mut self, bytes: &[u8]) {
        // A failed write to a closed terminal is not something to panic over: the session is
        // ending, and the caller's own exit path reports it.
        let _ = self.out.write_all(bytes);
    }
    fn flush(&mut self) {
        let _ = Write::flush(&mut self.out);
    }
}

/// Erase from the cursor to the end of the screen.
pub const ERASE_BELOW: &[u8] = b"\x1b[J";
/// Move the cursor to column 0 of the current row.
pub const COLUMN_ZERO: &[u8] = b"\r";
/// Switch to the alternate screen buffer.
pub const ENTER_ALTERNATE: &[u8] = b"\x1b[?1049h";
/// Switch back to the main screen buffer, restoring scrollback.
pub const LEAVE_ALTERNATE: &[u8] = b"\x1b[?1049l";
/// Hide the cursor while a frame is being composed.
pub const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
/// Show it again.
pub const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
/// Ask the terminal to send paste boundaries (§14.1).
pub const ENABLE_BRACKETED_PASTE: &[u8] = b"\x1b[?2004h";
/// Stop asking.
pub const DISABLE_BRACKETED_PASTE: &[u8] = b"\x1b[?2004l";

/// Writes one coherent frame, then flushes.
///
/// Held for the duration of a frame and no longer, so a half-drawn frame is never left on the
/// screen while something else decides to write.
pub struct Painter<'a, S: Sink> {
    sink: &'a mut S,
}

impl<'a, S: Sink> Painter<'a, S> {
    /// Begin a frame.
    ///
    /// Requires the ownership token by reference: a painter cannot exist without it, and the
    /// token cannot be copied, so there is never a second painter.
    pub fn new(_owner: &'a crate::Ownership, sink: &'a mut S) -> Self {
        Self { sink }
    }

    /// Raw bytes.
    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.sink.put(bytes);
        self
    }

    /// Text.
    pub fn text(&mut self, s: &str) -> &mut Self {
        self.sink.put(s.as_bytes());
        self
    }

    /// Move the cursor up `n` rows. Zero is a no-op rather than `\x1b[0A`, which some
    /// terminals treat as one row.
    pub fn up(&mut self, n: usize) -> &mut Self {
        if n > 0 {
            self.sink.put(format!("\x1b[{n}A").as_bytes());
        }
        self
    }

    /// Move the cursor right `n` columns.
    pub fn right(&mut self, n: usize) -> &mut Self {
        if n > 0 {
            self.sink.put(format!("\x1b[{n}C").as_bytes());
        }
        self
    }

    /// Carriage return then erase everything below.
    pub fn erase_block(&mut self) -> &mut Self {
        self.sink.put(COLUMN_ZERO);
        self.sink.put(ERASE_BELOW);
        self
    }

    /// Start a new line, which is what puts the previous one into scrollback.
    pub fn newline(&mut self) -> &mut Self {
        self.sink.put(b"\r\n");
        self
    }

    /// Finish the frame.
    pub fn done(&mut self) {
        self.sink.flush();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::Ownership;
    use crate::testing::claim;

    #[test]
    fn a_frame_is_one_flush() {
        let _c = claim();
        let own = Ownership::acquire().unwrap();
        let mut cap = Capture::new();
        {
            let mut p = Painter::new(&own, &mut cap);
            p.erase_block().text("prompt> GET k").done();
        }
        assert_eq!(
            cap.flushes, 1,
            "a frame must be presented once, not per write"
        );
        assert!(cap.text().contains("prompt> GET k"));
    }

    #[test]
    fn a_zero_movement_emits_nothing() {
        // `ESC [ 0 A` means "up one row" on several terminals, so zero must be silence.
        let _c = claim();
        let own = Ownership::acquire().unwrap();
        let mut cap = Capture::new();
        {
            let mut p = Painter::new(&own, &mut cap);
            p.up(0).right(0).done();
        }
        assert!(cap.bytes.is_empty(), "emitted {:?}", cap.text());
    }

    #[test]
    fn movements_are_emitted_once_with_the_right_count() {
        let _c = claim();
        let own = Ownership::acquire().unwrap();
        let mut cap = Capture::new();
        {
            let mut p = Painter::new(&own, &mut cap);
            p.up(3).right(12).done();
        }
        assert_eq!(cap.text(), "\x1b[3A\x1b[12C");
    }

    #[test]
    fn the_alternate_screen_pair_is_exact() {
        // Getting these wrong loses the user's scrollback, which cannot be recovered.
        assert_eq!(ENTER_ALTERNATE, b"\x1b[?1049h");
        assert_eq!(LEAVE_ALTERNATE, b"\x1b[?1049l");
        assert_ne!(ENTER_ALTERNATE, LEAVE_ALTERNATE);
    }
}
