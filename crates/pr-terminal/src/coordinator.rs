//! The terminal coordinator (v2.1 §14.4, §15.8, ADR-022, V-C02).
//!
//! One object owns the terminal, consumes one merged event stream, and produces every frame.
//! REPL editing, the self-drawn dropdown, out-of-band notices, and the TUI's alternate screen
//! are all states of this one machine rather than four subsystems sharing a file descriptor.
//!
//! Three rules carry most of the weight, and each exists because of a specific failure:
//!
//! - **Notices are printed, never injected.** A worker posts to a [`Mailbox`]; the coordinator
//!   decides when to print. Printing happens by erasing the prompt block, writing the line so
//!   it scrolls into history, and redrawing the block — so a pub/sub flood cannot land in the
//!   middle of what the user is typing (ASSIST-062).
//! - **While the alternate screen is up, notices wait.** The TUI owns every cell; printing
//!   into it would corrupt a frame that the TUI will not know to redraw.
//! - **Leaving the alternate screen restores the REPL draft.** The TUI's command box is
//!   carried back as an *uncommitted* draft and is never executed (§15.8 scenario H, UX-12).

use crate::event::{Input, Key, Notice};
use crate::ownership::Ownership;
use crate::paint::{
    DISABLE_BRACKETED_PASTE, ENABLE_BRACKETED_PASTE, ENTER_ALTERNATE, HIDE_CURSOR, LEAVE_ALTERNATE,
    Painter, SHOW_CURSOR, Sink,
};
use crate::paste::{BURST_GAP, Choice, Staging, needs_staging};
use pr_render::WidthPolicy;
use pr_repl::LineBuffer;

/// Which surface currently holds the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The main screen, with scrollback preserved.
    Repl,
    /// The alternate screen, used by the TUI.
    Alternate,
}

/// The self-drawn candidate dropdown.
///
/// Drawn by us rather than by the line editor, because it has to share the frame with notices
/// and with the TUI transition — a menu the editor draws on its own schedule is exactly the
/// second writer §14.4 forbids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    /// Rows, already escaped for display.
    pub items: Vec<String>,
    /// Which row has focus, if any.
    pub focused: Option<usize>,
}

impl Menu {
    /// A menu with nothing focused.
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        Self {
            items,
            focused: None,
        }
    }
    /// Move focus down, wrapping.
    pub fn down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.focused = Some(match self.focused {
            None => 0,
            Some(i) => (i + 1) % self.items.len(),
        });
    }
    /// Move focus up, wrapping.
    pub fn up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.focused = Some(match self.focused {
            None | Some(0) => self.items.len() - 1,
            Some(i) => i - 1,
        });
    }
    /// The focused row's text.
    #[must_use]
    pub fn selection(&self) -> Option<&str> {
        self.focused
            .and_then(|i| self.items.get(i))
            .map(String::as_str)
    }
}

/// What the application should do after an input was handled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing; the coordinator has already repainted.
    None,
    /// The user submitted this line.
    Submit(String),
    /// The user released a staged paste as several commands, in order (§14.1).
    ///
    /// Separate from [`Action::Submit`] so a caller cannot run a pasted block by accident:
    /// handling it means deciding, explicitly, what to do with more than one command.
    SubmitMany(Vec<String>),
    /// The user asked to leave.
    Quit,
}

/// Everything the TUI surface holds while it is up.
#[derive(Clone, Debug, Default)]
struct Tui {
    /// The TUI's own command box.
    box_text: String,
    /// Cursor within the box, as a byte index.
    box_cursor: usize,
    /// Whether the F1 help overlay is showing.
    help: bool,
}

/// The one thing that reads input and paints.
pub struct Coordinator<S: Sink> {
    owner: Ownership,
    sink: S,
    cols: u16,
    rows: u16,
    width: WidthPolicy,
    prompt: String,
    buffer: LineBuffer,
    menu: Option<Menu>,
    /// Pasted content held for review. Nothing in it executes until the user chooses.
    staging: Option<Staging>,
    surface: Surface,
    tui: Tui,
    /// The REPL draft saved when the alternate screen went up.
    stash: Option<(String, usize)>,
    /// Source label shown to the user: target, DB and result id (§15.8).
    origin: String,
    /// Rows the cursor currently sits below the top of the painted block.
    cursor_row: usize,
    /// Notices that arrived while it was not safe to print.
    pending: Vec<Notice>,
    /// Frames presented. A counter rather than a log, because the assertion is about how many
    /// coherent frames a flood produced, not about their contents.
    frames: usize,
    /// Notices printed into scrollback.
    printed: usize,
    /// When the last key arrived, for the §14.1 timing fallback.
    last_key_at: Option<std::time::Instant>,
    /// Lines completed inside the current burst, held back from execution.
    pending_paste: Vec<u8>,
    /// Whether the burst has written into the edit buffer as well.
    burst_touched_buffer: bool,
}

impl<S: Sink> Coordinator<S> {
    /// Claim the terminal and start in REPL mode.
    ///
    /// # Errors
    /// [`crate::OwnershipError::AlreadyOwned`] if something already owns the terminal. That
    /// is the enforcement of §14.4: a second coordinator cannot be built.
    pub fn new(sink: S, cols: u16, rows: u16) -> Result<Self, crate::OwnershipError> {
        Ok(Self {
            owner: Ownership::acquire()?,
            sink,
            cols,
            rows,
            width: WidthPolicy::narrow(),
            prompt: "penguin> ".to_owned(),
            buffer: LineBuffer::new(),
            menu: None,
            staging: None,
            surface: Surface::Repl,
            tui: Tui::default(),
            stash: None,
            origin: String::new(),
            cursor_row: 0,
            pending: Vec::new(),
            frames: 0,
            printed: 0,
            last_key_at: None,
            pending_paste: Vec::new(),
            burst_touched_buffer: false,
        })
    }

    /// Set the prompt text. It is Penguin-authored, so it is not escaped here.
    pub fn set_prompt(&mut self, p: impl Into<String>) {
        self.prompt = p.into();
    }

    /// Set the width policy (§14.6).
    pub fn set_width_policy(&mut self, w: WidthPolicy) {
        self.width = w;
    }

    /// Set the source label carried across a TUI round trip.
    pub fn set_origin(&mut self, o: impl Into<String>) {
        self.origin = o.into();
    }

    /// The source label.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The current surface.
    #[must_use]
    pub fn surface(&self) -> Surface {
        self.surface
    }

    /// The REPL buffer text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// The REPL cursor, as a byte index.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.buffer.cursor()
    }

    /// The TUI command box, while the alternate screen is up.
    #[must_use]
    pub fn tui_box(&self) -> &str {
        &self.tui.box_text
    }

    /// Whether the TUI help overlay is showing.
    #[must_use]
    pub fn tui_help_open(&self) -> bool {
        self.tui.help
    }

    /// The paste under review, if any.
    #[must_use]
    pub fn staging(&self) -> Option<&Staging> {
        self.staging.as_ref()
    }

    /// The dropdown, if one is open.
    #[must_use]
    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    /// Notices waiting because it is not safe to print.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Frames presented so far.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Notices printed into scrollback so far.
    #[must_use]
    pub fn printed(&self) -> usize {
        self.printed
    }

    /// Terminal size.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Borrow the sink, for a test to inspect what was painted.
    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Mutably borrow the sink, for a test to clear its capture between frames.
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Open the dropdown with these rows.
    pub fn open_menu(&mut self, items: Vec<String>) {
        self.menu = Some(Menu::new(items));
        self.repaint();
    }

    /// Close the dropdown.
    pub fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.repaint();
        }
    }

    // ---------------------------------------------------------------- lifecycle

    /// Paint the first frame and turn on the modes we need.
    pub fn start(&mut self) {
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.raw(ENABLE_BRACKETED_PASTE).done();
        self.frames += 1;
        self.repaint();
    }

    /// Restore the terminal. Called on the way out, and on drop.
    pub fn shutdown(&mut self) {
        if self.surface == Surface::Alternate {
            self.leave_alternate();
        }
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.erase_block()
            .raw(DISABLE_BRACKETED_PASTE)
            .raw(SHOW_CURSOR)
            .newline()
            .done();
        self.frames += 1;
    }

    // ---------------------------------------------------------------- input

    /// Handle one input and repaint.
    pub fn handle(&mut self, input: Input) -> Action {
        self.handle_at(input, std::time::Instant::now())
    }

    /// Nothing is pending from a burst that has not finished yet.
    #[must_use]
    pub fn pending_paste(&self) -> usize {
        self.pending_paste.len()
    }

    /// Let time pass.
    ///
    /// The timing fallback needs a moment of quiet to decide a burst has ended; the event
    /// loop calls this when its poll times out. Without it, a paste that arrives without
    /// bracketing would sit in limbo until the user pressed another key.
    pub fn tick(&mut self, at: std::time::Instant) {
        let quiet = self
            .last_key_at
            .is_none_or(|p| at.saturating_duration_since(p) >= BURST_GAP);
        if quiet && !self.pending_paste.is_empty() {
            self.flush_burst();
        }
    }

    /// Handle one input that arrived at `at`, and repaint.
    ///
    /// The timestamp is a parameter so the §14.1 timing fallback can be tested with a script
    /// instead of by typing quickly and hoping.
    pub fn handle_at(&mut self, input: Input, at: std::time::Instant) -> Action {
        match input {
            Input::Resize { cols, rows } => {
                self.cols = cols;
                self.rows = rows;
                // A resize invalidates everything we believed about the cursor's row, so the
                // block is redrawn from a known position rather than patched.
                self.cursor_row = 0;
                self.repaint();
                Action::None
            }
            Input::Message(n) => {
                self.deliver(n);
                Action::None
            }
            Input::Interrupt => Action::Quit,
            Input::Paste(bytes) => {
                if self.surface == Surface::Repl {
                    if needs_staging(&bytes) {
                        // §14.1: multi-line, or more than a person can be expected to have
                        // read. It goes to review; nothing is executed.
                        self.staging = Some(Staging::new(&bytes));
                        self.menu = None;
                    } else {
                        // A single line goes into the edit buffer — unsubmitted, but not
                        // worth a modal for.
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let _ = self.buffer.insert(&text);
                    }
                    self.repaint();
                }
                Action::None
            }
            Input::Key(k) if self.staging.is_some() => {
                self.last_key_at = Some(at);
                self.staging_key(k)
            }
            Input::Key(k) => match self.surface {
                Surface::Repl => self.repl_key_at(k, at),
                Surface::Alternate => {
                    self.tui_key(k);
                    Action::None
                }
            },
        }
    }

    /// Handle a REPL key, applying §14.1's timing fallback.
    ///
    /// Where bracketed paste works the terminal tells us and none of this runs. Where it does
    /// not — and Windows `ConPTY` does not: crossterm delivers pasted text there as ordinary key
    /// presses, Enter included — this is the only thing standing between a pasted `FLUSHALL`
    /// and a `FLUSHALL`. The first Windows CI run of the paste tests executed all three pasted
    /// commands, which is exactly the failure §14.1 forbids.
    fn repl_key_at(&mut self, k: Key, at: std::time::Instant) -> Action {
        let bursty = self
            .last_key_at
            .is_some_and(|p| at.saturating_duration_since(p) < BURST_GAP);
        self.last_key_at = Some(at);

        // A gap ended the burst: release whatever it held before handling this key normally.
        if !bursty && !self.pending_paste.is_empty() {
            self.flush_burst();
        }

        if bursty {
            match k {
                Key::Enter => {
                    // A line completed *inside a burst* is not a submission. Hold it.
                    let line = self.buffer.text().to_owned();
                    self.pending_paste.extend_from_slice(line.as_bytes());
                    self.pending_paste.push(b'\n');
                    self.buffer.set_text("");
                    self.burst_touched_buffer = false;
                    self.menu = None;
                    self.repaint();
                    return Action::None;
                }
                Key::Char(_) | Key::Backspace => self.burst_touched_buffer = true,
                _ => {}
            }
        }

        self.repl_key(k)
    }

    /// Decide what a finished burst was.
    ///
    /// Multi-line, or longer than a person can be expected to have read, goes to review. A
    /// single line goes back into the edit buffer **unsubmitted** — which costs a very fast
    /// typist one extra Enter, and costs nobody a command they did not read.
    fn flush_burst(&mut self) {
        let mut content = std::mem::take(&mut self.pending_paste);
        if self.burst_touched_buffer {
            content.extend_from_slice(self.buffer.text().as_bytes());
            self.buffer.set_text("");
        }
        self.burst_touched_buffer = false;
        if content.is_empty() {
            return;
        }
        // The final newline is the Enter we withheld, not a line separator. Counting it would
        // put a single fast-typed command behind a modal, and the protection is the same
        // either way: with it stripped the line lands in the buffer unsubmitted, so running it
        // still takes one deliberate keypress.
        let content: &[u8] = content.strip_suffix(b"\n").unwrap_or(&content);
        if needs_staging(content) {
            self.staging = Some(Staging::new(content));
            self.menu = None;
        } else {
            let text = String::from_utf8_lossy(content).into_owned();
            let _ = self.buffer.insert(&text);
        }
        self.repaint();
    }

    fn repl_key(&mut self, k: Key) -> Action {
        let action = match k {
            Key::Char(c) => {
                let _ = self.buffer.insert(&c.to_string());
                // Typing invalidates the menu: the candidates were computed for older text
                // (§12.8). Leaving it up is how a stale row gets accepted.
                self.menu = None;
                Action::None
            }
            Key::Backspace => {
                self.buffer.backspace();
                self.menu = None;
                Action::None
            }
            Key::Left => {
                self.buffer.move_left();
                Action::None
            }
            Key::Right => {
                self.buffer.move_right();
                Action::None
            }
            Key::Down => {
                if let Some(m) = self.menu.as_mut() {
                    m.down();
                }
                Action::None
            }
            Key::Up => {
                if let Some(m) = self.menu.as_mut() {
                    m.up();
                }
                Action::None
            }
            Key::Esc => {
                self.menu = None;
                Action::None
            }
            Key::Tab | Key::Enter if self.menu.as_ref().is_some_and(|m| m.focused.is_some()) => {
                self.accept_selection();
                Action::None
            }
            Key::Enter => {
                let line = self.buffer.text().to_owned();
                self.buffer.set_text("");
                self.menu = None;
                // The submitted line is committed to scrollback before anything else is
                // drawn, so the history reads in the order things happened.
                let mut p = Painter::new(&self.owner, &mut self.sink);
                p.erase_block()
                    .text(&format!("{}{line}", self.prompt))
                    .newline()
                    .done();
                self.frames += 1;
                self.cursor_row = 0;
                Action::Submit(line)
            }
            Key::Ctrl('c') => {
                self.buffer.set_text("");
                self.menu = None;
                Action::None
            }
            Key::Ctrl('d') if self.buffer.is_empty() => Action::Quit,
            // Tab with nothing focused, an unbound Ctrl chord, a function key: the REPL
            // surface has no meaning for these, and inventing one is worse than ignoring it.
            Key::Tab | Key::Ctrl(_) | Key::Function(_) => Action::None,
        };
        self.repaint();
        action
    }

    /// Keys while a paste is under review (§14.1).
    ///
    /// The three choices are spelled out rather than bound to Enter, because Enter is the key
    /// the user presses without thinking and this is the moment not to.
    fn staging_key(&mut self, k: Key) -> Action {
        let Some(st) = self.staging.as_mut() else {
            return Action::None;
        };
        let action = match k {
            Key::Down => {
                let next = st.focused() + 1;
                st.focus(next);
                Action::None
            }
            Key::Up => {
                let prev = st.focused().saturating_sub(1);
                st.focus(prev);
                Action::None
            }
            Key::Backspace => {
                st.delete();
                if st.is_empty() {
                    self.staging = None;
                }
                Action::None
            }
            Key::Char('1') => {
                let out = st.decide(Choice::OneByOne);
                self.staging = None;
                Action::SubmitMany(out)
            }
            Key::Char('2') => {
                let out = st.decide(Choice::SingleCommand);
                self.staging = None;
                out.into_iter().next().map_or(Action::None, Action::Submit)
            }
            Key::Esc | Key::Ctrl('c') => {
                st.decide(Choice::Cancel);
                self.staging = None;
                Action::None
            }
            // Deliberately inert, Enter included: §14.1 forbids an automatic execution, and
            // the key most likely to be pressed by reflex is the one that must do nothing.
            _ => Action::None,
        };
        self.repaint();
        action
    }

    fn accept_selection(&mut self) {
        // Accepting replaces the word under the cursor. It is one undoable edit and never an
        // execution (ASSIST-020).
        let Some(sel) = self
            .menu
            .as_ref()
            .and_then(Menu::selection)
            .map(str::to_owned)
        else {
            return;
        };
        let text = self.buffer.text();
        let cursor = self.buffer.cursor();
        let start = text[..cursor]
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + 1);
        let _ = self
            .buffer
            .accept_completion(&pr_repl::TextEdit::new(start..cursor, sel));
        self.menu = None;
    }

    fn tui_key(&mut self, k: Key) {
        match k {
            Key::Function(1) => {
                // §15.8: opening help must leave the command box and its cursor untouched.
                self.tui.help = !self.tui.help;
            }
            Key::Esc => {
                if self.tui.help {
                    self.tui.help = false;
                } else {
                    self.leave_alternate();
                    return;
                }
            }
            Key::Char(c) if !self.tui.help => {
                self.tui.box_text.insert(self.tui.box_cursor, c);
                self.tui.box_cursor += c.len_utf8();
            }
            Key::Backspace if !self.tui.help => {
                if let Some((i, ch)) = self.tui.box_text[..self.tui.box_cursor]
                    .char_indices()
                    .next_back()
                {
                    self.tui.box_text.remove(i);
                    self.tui.box_cursor -= ch.len_utf8();
                }
            }
            _ => {}
        }
        self.paint_alternate();
    }

    // ---------------------------------------------------------------- surfaces

    /// Put up the alternate screen.
    ///
    /// The inline block is erased first: leaving a stale prompt behind means it reappears when
    /// the alternate screen comes down, below the restored one.
    pub fn enter_alternate(&mut self) {
        if self.surface == Surface::Alternate {
            return;
        }
        self.stash = Some((self.buffer.text().to_owned(), self.buffer.cursor()));
        self.tui = Tui {
            box_text: self.buffer.text().to_owned(),
            box_cursor: self.buffer.cursor(),
            help: false,
        };
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.erase_block().raw(ENTER_ALTERNATE).raw(HIDE_CURSOR).done();
        self.frames += 1;
        self.surface = Surface::Alternate;
        self.paint_alternate();
    }

    /// Take the alternate screen down and go back to the REPL.
    ///
    /// The TUI's command box comes back as the REPL draft and is **not** executed: §15.8 is
    /// explicit that returning carries the uncommitted draft, and UX-12 requires the source
    /// label to be the same one the user was looking at.
    pub fn leave_alternate(&mut self) {
        if self.surface != Surface::Alternate {
            return;
        }
        let carried = std::mem::take(&mut self.tui.box_text);
        let carried_cursor = self.tui.box_cursor.min(carried.len());
        self.tui = Tui::default();

        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.raw(LEAVE_ALTERNATE).raw(SHOW_CURSOR).done();
        self.frames += 1;
        self.surface = Surface::Repl;
        self.cursor_row = 0;

        self.buffer.set_text(&carried);
        let _ = self.buffer.set_cursor(carried_cursor);
        self.stash = None;

        // Anything that arrived while the TUI held the screen is printed now, in order.
        let waiting = std::mem::take(&mut self.pending);
        for n in waiting {
            self.print_notice(&n);
        }
        self.repaint();
    }

    // ---------------------------------------------------------------- notices

    fn deliver(&mut self, n: Notice) {
        if self.surface == Surface::Alternate {
            // The TUI owns every cell; printing into it corrupts a frame it will not redraw.
            self.pending.push(n);
            return;
        }
        self.print_notice(&n);
        self.repaint();
    }

    /// Print-and-redraw: erase our block, write the line so it enters scrollback, and let the
    /// caller repaint. The prompt and the user's text are never touched.
    fn print_notice(&mut self, n: &Notice) {
        let line = format!("[{}] {}", n.origin.label(), n.text);
        let cursor_row = self.cursor_row;
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.up(cursor_row).erase_block().text(&line).newline().done();
        self.frames += 1;
        self.printed += 1;
        self.cursor_row = 0;
    }

    // ---------------------------------------------------------------- painting

    /// Redraw the prompt block in place.
    fn repaint(&mut self) {
        if self.surface == Surface::Alternate {
            return;
        }
        let cols = usize::from(self.cols).max(1);
        let prompt_w = self.width.str_width(&self.prompt);
        let text = self.buffer.text().to_owned();
        let cursor = self.buffer.cursor().min(text.len());
        let before_w = self.width.str_width(&text[..cursor]);
        let cursor_col = prompt_w + before_w;

        let staging_lines: Vec<String> = self.staging.as_ref().map_or_else(Vec::new, |st| {
            let mut out = vec![format!(
                "-- pasted {} line(s), nothing has run --",
                st.lines().iter().filter(|l| !l.trim().is_empty()).count()
            )];
            for (i, l) in st.lines().iter().enumerate() {
                let marker = if st.focused() == i { "> " } else { "  " };
                let (shown, _) = self.width.truncate(l, cols.saturating_sub(2));
                out.push(format!("{marker}{shown}"));
            }
            out.push("[1] run one by one  [2] run as one command  [Esc] cancel".to_owned());
            out
        });

        let menu_lines: Vec<String> = self.menu.as_ref().map_or_else(Vec::new, |m| {
            m.items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let marker = if m.focused == Some(i) { "> " } else { "  " };
                    let (shown, _) = self.width.truncate(item, cols.saturating_sub(2));
                    format!("{marker}{shown}")
                })
                .collect()
        });

        let up = self.cursor_row;
        let prompt = self.prompt.clone();
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.up(up).erase_block().text(&prompt).text(&text);
        for line in &staging_lines {
            p.newline().text(line);
        }
        for line in &menu_lines {
            p.newline().text(line);
        }
        // Come back to the edit position. The menu sits below, so the cursor returns to the
        // prompt row rather than staying wherever the last menu row ended.
        p.up(menu_lines.len() + staging_lines.len())
            .raw(b"\r")
            .right(cursor_col % cols);
        p.done();

        self.frames += 1;
        self.cursor_row = cursor_col / cols;
    }

    fn paint_alternate(&mut self) {
        let box_text = self.tui.box_text.clone();
        let help = self.tui.help;
        let origin = self.origin.clone();
        let mut p = Painter::new(&self.owner, &mut self.sink);
        p.raw(b"\x1b[H\x1b[2J")
            .text(&format!("penguin TUI — {origin}"))
            .newline()
            .text(&format!("command: {box_text}"));
        if help {
            p.newline().text("[help] F1 closes this. Esc returns.");
        }
        p.done();
        self.frames += 1;
    }
}

impl<S: Sink> Drop for Coordinator<S> {
    fn drop(&mut self) {
        // If the alternate screen is still up when we unwind, the user is left staring at a
        // blank buffer with no shell. Put it down unconditionally.
        if self.surface == Surface::Alternate {
            let mut p = Painter::new(&self.owner, &mut self.sink);
            p.raw(LEAVE_ALTERNATE).raw(SHOW_CURSOR).done();
        }
    }
}
