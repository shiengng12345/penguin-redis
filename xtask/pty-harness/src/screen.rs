//! A minimal terminal screen model (V-A01, V-C02, and everything in Track C).
//!
//! Asserting on the raw PTY byte stream works on Unix and does not work on Windows, and the
//! reason is structural rather than incidental. `ConPTY` does not forward what the child wrote:
//! it renders the child's output into its own screen buffer and then emits **a diff of that
//! buffer**. So a program that writes `\r penguin> HGX` may produce a stream in which the
//! letters never appear next to each other, even though that is exactly what is on screen.
//!
//! The fix is to assert on what the user sees. This applies the VT stream to a grid and lets
//! a test ask "does the screen say this?" — which is the question every Track C acceptance
//! case is really asking, and which happens to be the same question on all three platforms.
//!
//! It is deliberately small. It models what Penguin itself emits plus what `ConPTY` emits
//! around it; anything else is skipped rather than guessed at, and [`Screen::unhandled`]
//! counts the skips so a test can notice if that assumption stops holding.
//!
//! Width is one cell per character here. Getting East Asian width right is V-C04's subject,
//! and baking a half-answer into the harness would make that measurement circular.

/// A rendered terminal screen, plus what has scrolled off the top.
#[derive(Clone, Debug)]
pub struct Screen {
    cols: usize,
    rows: usize,
    grid: Vec<Vec<char>>,
    row: usize,
    col: usize,
    /// Lines that scrolled off the top of the main screen.
    scrollback: Vec<String>,
    /// Saved main screen while the alternate screen is up.
    saved: Option<(Vec<Vec<char>>, usize, usize)>,
    /// Cursor stashed by `ESC 7` or `CSI s`, restored by `ESC 8` or `CSI u`.
    saved_cursor: Option<(usize, usize)>,
    /// Partial UTF-8 sequence carried between feeds.
    pending: Vec<u8>,
    state: State,
    params: Vec<u8>,
    unhandled: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Csi,
    /// A string-terminated sequence: OSC, DCS, APC, PM.
    StringSeq,
    /// Just saw ESC inside a string sequence; a `\` ends it.
    StringSeqEsc,
    /// Consume exactly one more byte (charset designators).
    SkipOne,
}

impl Screen {
    /// A blank screen.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        let (cols, rows) = (usize::from(cols).max(1), usize::from(rows).max(1));
        Self {
            cols,
            rows,
            grid: vec![vec![' '; cols]; rows],
            row: 0,
            col: 0,
            scrollback: Vec::new(),
            saved: None,
            saved_cursor: None,
            pending: Vec::new(),
            state: State::Ground,
            params: Vec::new(),
            unhandled: 0,
        }
    }

    /// Apply a chunk of terminal output.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(bytes);
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            match self.state {
                State::Ground => {
                    if b == 0x1b {
                        self.state = State::Escape;
                        i += 1;
                    } else if b < 0x80 {
                        self.control_or_print(char::from(b));
                        i += 1;
                    } else {
                        // A multi-byte character may be split across chunks.
                        let len = utf8_len(b);
                        if i + len > buf.len() {
                            break;
                        }
                        match std::str::from_utf8(&buf[i..i + len]) {
                            Ok(s) => {
                                for c in s.chars() {
                                    self.put(c);
                                }
                            }
                            // Invalid UTF-8 is shown as the replacement character rather than
                            // dropped: a test about binary output needs to see that something
                            // was there.
                            Err(_) => self.put('\u{fffd}'),
                        }
                        i += len;
                    }
                }
                State::Escape => {
                    self.state = match b {
                        b'[' => {
                            self.params.clear();
                            State::Csi
                        }
                        b']' | b'P' | b'_' | b'^' => State::StringSeq,
                        b'(' | b')' | b'*' | b'+' => State::SkipOne,
                        b'7' => {
                            self.saved_cursor = Some((self.row, self.col));
                            State::Ground
                        }
                        b'8' => {
                            self.restore_cursor();
                            State::Ground
                        }
                        b'c' => {
                            self.erase_all();
                            self.row = 0;
                            self.col = 0;
                            State::Ground
                        }
                        b'M' => {
                            self.row = self.row.saturating_sub(1);
                            State::Ground
                        }
                        _ => {
                            self.unhandled += 1;
                            State::Ground
                        }
                    };
                    i += 1;
                }
                State::SkipOne => {
                    self.state = State::Ground;
                    i += 1;
                }
                State::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        self.csi(b);
                        self.state = State::Ground;
                    } else {
                        self.params.push(b);
                    }
                    i += 1;
                }
                State::StringSeq => {
                    self.state = match b {
                        0x07 => State::Ground,
                        0x1b => State::StringSeqEsc,
                        _ => State::StringSeq,
                    };
                    i += 1;
                }
                State::StringSeqEsc => {
                    // `ESC \` is the string terminator; anything else stays inside the string.
                    self.state = if b == b'\\' {
                        State::Ground
                    } else {
                        State::StringSeq
                    };
                    i += 1;
                }
            }
        }
        self.pending = buf[i..].to_vec();
    }

    fn control_or_print(&mut self, c: char) {
        match c {
            '\r' => self.col = 0,
            '\n' => self.newline(),
            '\u{8}' => self.col = self.col.saturating_sub(1),
            '\t' => self.col = ((self.col / 8) + 1) * 8,
            // Bell and the other C0 codes leave no mark on the screen.
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            c => self.put(c),
        }
    }

    fn put(&mut self, c: char) {
        if self.col >= self.cols {
            self.col = 0;
            self.newline();
        }
        if let Some(row) = self.grid.get_mut(self.row)
            && let Some(cell) = row.get_mut(self.col)
        {
            *cell = c;
        }
        self.col += 1;
    }

    fn newline(&mut self) {
        self.row += 1;
        if self.row >= self.rows {
            self.row = self.rows - 1;
            let gone: String = self.grid.remove(0).into_iter().collect();
            // Only the main screen has scrollback; the alternate screen is explicitly not
            // supposed to produce any (§14.4).
            if self.saved.is_none() {
                self.scrollback.push(gone.trim_end().to_owned());
            }
            self.grid.push(vec![' '; self.cols]);
        }
    }

    /// Restore the position stashed by `ESC 7` / `CSI s`.
    ///
    /// Position only: colour and character-set state are not modelled, because they do not
    /// change which characters the user sees.
    fn restore_cursor(&mut self) {
        if let Some((r, c)) = self.saved_cursor {
            self.row = r.min(self.rows - 1);
            self.col = c.min(self.cols - 1);
        }
    }

    fn nums(&self) -> Vec<usize> {
        String::from_utf8_lossy(&self.params)
            .trim_start_matches(['?', '>', '<', '='])
            .split(';')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    }

    fn private(&self) -> bool {
        self.params.first() == Some(&b'?')
    }

    #[allow(clippy::too_many_lines)]
    fn csi(&mut self, final_byte: u8) {
        let n = self.nums();
        let first = n.first().copied().unwrap_or(0);
        let count = first.max(1);
        match final_byte {
            b'A' => self.row = self.row.saturating_sub(count),
            b'B' => self.row = (self.row + count).min(self.rows - 1),
            b'C' => self.col = (self.col + count).min(self.cols - 1),
            b'D' => self.col = self.col.saturating_sub(count),
            b'E' => {
                self.row = (self.row + count).min(self.rows - 1);
                self.col = 0;
            }
            b'F' => {
                self.row = self.row.saturating_sub(count);
                self.col = 0;
            }
            b'G' | b'`' => self.col = count.saturating_sub(1).min(self.cols - 1),
            b'd' => self.row = count.saturating_sub(1).min(self.rows - 1),
            b'H' | b'f' => {
                let r = n.first().copied().unwrap_or(1).max(1);
                let c = n.get(1).copied().unwrap_or(1).max(1);
                self.row = (r - 1).min(self.rows - 1);
                self.col = (c - 1).min(self.cols - 1);
            }
            b'J' => match first {
                1 => self.erase_to_start(),
                2 | 3 => self.erase_all(),
                _ => self.erase_below(),
            },
            b'K' => match first {
                1 => self.erase_line_to_start(),
                2 => self.erase_line(),
                _ => self.erase_line_to_end(),
            },
            b'X' => {
                for c in self.col..(self.col + count).min(self.cols) {
                    self.grid[self.row][c] = ' ';
                }
            }
            b'P' => {
                let row = &mut self.grid[self.row];
                for _ in 0..count.min(self.cols - self.col) {
                    row.remove(self.col);
                    row.push(' ');
                }
            }
            b'@' => {
                let row = &mut self.grid[self.row];
                for _ in 0..count.min(self.cols - self.col) {
                    row.insert(self.col, ' ');
                    row.pop();
                }
            }
            b'L' => {
                for _ in 0..count.min(self.rows - self.row) {
                    self.grid.insert(self.row, vec![' '; self.cols]);
                    self.grid.pop();
                }
            }
            b'M' => {
                for _ in 0..count.min(self.rows - self.row) {
                    self.grid.remove(self.row);
                    self.grid.push(vec![' '; self.cols]);
                }
            }
            b'h' if self.private() && n.contains(&1049) => self.enter_alternate(),
            b'l' if self.private() && n.contains(&1049) => self.leave_alternate(),
            // Colour, cursor visibility, mode sets, status reports and scroll regions do not
            // change which characters are on the screen.
            b's' => self.saved_cursor = Some((self.row, self.col)),
            b'u' => self.restore_cursor(),
            // Colour, cursor visibility, mode sets, status reports and scroll regions do not
            // change which characters are on the screen.
            b'm' | b'h' | b'l' | b'n' | b'r' | b't' | b'c' | b'q' => {}
            _ => self.unhandled += 1,
        }
    }

    fn enter_alternate(&mut self) {
        if self.saved.is_none() {
            self.saved = Some((self.grid.clone(), self.row, self.col));
            self.erase_all();
            self.row = 0;
            self.col = 0;
        }
    }

    fn leave_alternate(&mut self) {
        if let Some((grid, row, col)) = self.saved.take() {
            self.grid = grid;
            self.row = row;
            self.col = col;
        }
    }

    fn erase_all(&mut self) {
        for r in &mut self.grid {
            r.fill(' ');
        }
    }

    fn erase_below(&mut self) {
        self.erase_line_to_end();
        for r in self.row + 1..self.rows {
            self.grid[r].fill(' ');
        }
    }

    fn erase_to_start(&mut self) {
        self.erase_line_to_start();
        for r in 0..self.row {
            self.grid[r].fill(' ');
        }
    }

    fn erase_line(&mut self) {
        self.grid[self.row].fill(' ');
    }

    fn erase_line_to_end(&mut self) {
        for c in self.col..self.cols {
            self.grid[self.row][c] = ' ';
        }
    }

    fn erase_line_to_start(&mut self) {
        for c in 0..=self.col.min(self.cols - 1) {
            self.grid[self.row][c] = ' ';
        }
    }

    /// One row of the visible screen, trailing blanks removed.
    #[must_use]
    pub fn line(&self, row: usize) -> String {
        self.grid
            .get(row)
            .map(|r| r.iter().collect::<String>().trim_end().to_owned())
            .unwrap_or_default()
    }

    /// Every visible row.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        (0..self.rows).map(|r| self.line(r)).collect()
    }

    /// The visible screen as text.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines().join("\n")
    }

    /// Everything the user has seen: what scrolled off, then the visible screen.
    #[must_use]
    pub fn history(&self) -> String {
        let mut out = self.scrollback.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.text());
        out
    }

    /// Whether the visible screen shows `needle` on a single row.
    #[must_use]
    pub fn shows(&self, needle: &str) -> bool {
        self.lines().iter().any(|l| l.contains(needle))
    }

    /// Whether `needle` appears anywhere the user could have seen it, scrollback included.
    #[must_use]
    pub fn seen(&self, needle: &str) -> bool {
        self.history().lines().any(|l| l.contains(needle))
    }

    /// Cursor position, zero-based `(row, col)`.
    #[must_use]
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// Whether the alternate screen is currently up.
    #[must_use]
    pub fn in_alternate(&self) -> bool {
        self.saved.is_some()
    }

    /// Lines that have scrolled off the main screen.
    #[must_use]
    pub fn scrollback(&self) -> &[String] {
        &self.scrollback
    }

    /// How many sequences were skipped because this model does not implement them.
    ///
    /// A test can assert this stays small; a jump means a program started emitting something
    /// the model silently ignores, and an assertion about the screen would then be about a
    /// screen the user does not have.
    #[must_use]
    pub fn unhandled(&self) -> usize {
        self.unhandled
    }

    /// Size, as `(cols, rows)`.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (
            u16::try_from(self.cols).unwrap_or(u16::MAX),
            u16::try_from(self.rows).unwrap_or(u16::MAX),
        )
    }

    /// Resize, keeping what fits.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (usize::from(cols).max(1), usize::from(rows).max(1));
        for r in &mut self.grid {
            r.resize(cols, ' ');
        }
        self.grid.resize(rows, vec![' '; cols]);
        self.cols = cols;
        self.rows = rows;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xf0..=0xf7 => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn screen(bytes: &[u8]) -> Screen {
        let mut s = Screen::new(40, 6);
        s.feed(bytes);
        s
    }

    #[test]
    fn plain_text_lands_on_the_first_row() {
        let s = screen(b"hello");
        assert_eq!(s.line(0), "hello");
        assert_eq!(s.cursor(), (0, 5));
        assert!(s.shows("hello"));
    }

    #[test]
    fn a_carriage_return_overwrites_rather_than_appends() {
        // This is the shape Penguin's repaint uses, and the reason a byte-stream assertion
        // says something different from what the screen says.
        let s = screen(b"penguin> HG\rpenguin> HGX");
        assert_eq!(s.line(0), "penguin> HGX");
        assert!(
            s.shows("HGX"),
            "the screen says HGX even though the stream does not"
        );
    }

    #[test]
    fn erase_to_end_of_line_removes_what_was_there() {
        let s = screen(b"abcdef\r\x1b[3C\x1b[K");
        assert_eq!(s.line(0), "abc");
    }

    #[test]
    fn erase_below_clears_the_following_rows() {
        let s = screen(b"one\r\ntwo\r\nthree\r\x1b[1;1H\x1b[J");
        assert_eq!(s.lines().iter().filter(|l| !l.is_empty()).count(), 0);
    }

    #[test]
    fn cursor_movement_is_absolute_and_relative() {
        let mut s = Screen::new(40, 6);
        s.feed(b"\x1b[3;5Hx");
        assert_eq!(s.line(2), "    x");
        s.feed(b"\x1b[2A\x1b[2Dy");
        assert_eq!(s.cursor().0, 0);
        assert_eq!(s.line(0), "   y");
    }

    #[test]
    fn a_newline_past_the_bottom_scrolls_and_keeps_history() {
        let mut s = Screen::new(10, 3);
        for i in 0..6 {
            s.feed(format!("line{i}\r\n").as_bytes());
        }
        // The screen holds the last rows; the rest is scrollback, and both are "seen".
        assert!(s.shows("line5"));
        assert!(!s.shows("line0"));
        assert!(s.seen("line0"), "history: {:?}", s.history());
        assert!(s.scrollback().iter().any(|l| l == "line0"));
    }

    #[test]
    fn the_alternate_screen_hides_and_restores_the_main_one() {
        let mut s = Screen::new(20, 4);
        s.feed(b"draft in progress");
        assert!(s.shows("draft in progress"));

        s.feed(b"\x1b[?1049h");
        assert!(s.in_alternate());
        assert!(!s.shows("draft in progress"), "the TUI covers it");
        s.feed(b"TUI here");
        assert!(s.shows("TUI here"));

        s.feed(b"\x1b[?1049l");
        assert!(!s.in_alternate());
        assert!(s.shows("draft in progress"), "and it comes back");
        assert!(!s.shows("TUI here"));
    }

    #[test]
    fn the_alternate_screen_produces_no_scrollback() {
        // §14.4: only the main screen keeps history.
        let mut s = Screen::new(10, 2);
        s.feed(b"\x1b[?1049h");
        for i in 0..5 {
            s.feed(format!("alt{i}\r\n").as_bytes());
        }
        assert!(s.scrollback().is_empty(), "{:?}", s.scrollback());
    }

    #[test]
    fn colour_and_mode_sequences_leave_no_marks() {
        let s = screen(b"\x1b[1;31mred\x1b[0m\x1b[?25l\x1b[?2004h");
        assert_eq!(s.line(0), "red");
        assert_eq!(s.unhandled(), 0, "these are all modelled as no-ops");
    }

    #[test]
    fn an_osc_title_does_not_reach_the_grid() {
        // ConPTY sets the window title on startup; it must not appear as text.
        let s = screen(b"\x1b]0;D:\\path\\to\\prog.exe\x07ready");
        assert_eq!(s.line(0), "ready");
    }

    #[test]
    fn an_osc_terminated_by_st_is_also_consumed() {
        let s = screen(b"\x1b]8;;http://example\x1b\\link");
        assert_eq!(s.line(0), "link");
    }

    #[test]
    fn a_multibyte_character_split_across_feeds_is_reassembled() {
        let mut s = Screen::new(20, 2);
        let bytes = "中文".as_bytes();
        s.feed(&bytes[..2]);
        s.feed(&bytes[2..]);
        assert_eq!(s.line(0), "中文");
    }

    #[test]
    fn invalid_utf8_is_visible_rather_than_silently_dropped() {
        let s = screen(&[b'a', 0xff, 0xfe, b'b']);
        assert!(s.line(0).contains('a') && s.line(0).contains('b'));
        assert!(s.line(0).contains('\u{fffd}'), "{:?}", s.line(0));
    }

    #[test]
    fn wrapping_moves_to_the_next_row() {
        let mut s = Screen::new(5, 3);
        s.feed(b"abcdefgh");
        assert_eq!(s.line(0), "abcde");
        assert_eq!(s.line(1), "fgh");
    }

    #[test]
    fn an_unmodelled_sequence_is_counted_not_ignored_silently() {
        let s = screen(b"\x1b[5Wtext");
        assert_eq!(s.unhandled(), 1, "the skip is visible to a test");
        assert_eq!(s.line(0), "text");
    }

    #[test]
    fn the_cursor_can_be_saved_and_restored() {
        // Both spellings: `ESC 7`/`ESC 8` and `CSI s`/`CSI u`.
        let mut s = Screen::new(20, 4);
        s.feed(b"\x1b[3;5H\x1b7\x1b[1;1Habc\x1b8X");
        assert_eq!(s.line(0), "abc");
        assert_eq!(s.line(2), "    X");

        let mut s = Screen::new(20, 4);
        s.feed(b"\x1b[2;3H\x1b[s\x1b[4;1Hzz\x1b[uY");
        assert_eq!(s.line(3), "zz");
        assert_eq!(s.line(1), "  Y");
    }

    #[test]
    fn a_resize_keeps_what_fits() {
        let mut s = Screen::new(20, 4);
        s.feed(b"hello there");
        s.resize(6, 2);
        assert_eq!(s.size(), (6, 2));
        assert_eq!(s.line(0), "hello");
    }
}
