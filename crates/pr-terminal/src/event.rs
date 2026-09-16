//! The single event source (v2.1 §14.4, ADR-022, V-C02).
//!
//! Everything the coordinator reacts to arrives here, whether it came from the keyboard, a
//! resize, a signal or a Tokio worker holding a pub/sub message. Merging them into one stream
//! is what makes the ordering decidable: a message cannot arrive "during" a keystroke,
//! because there is one queue and one consumer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A key the editor understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A character typed.
    Char(char),
    /// Backspace.
    Backspace,
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Esc,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Ctrl plus a letter, lowercased.
    Ctrl(char),
    /// A function key.
    Function(u8),
}

/// Something the coordinator has to react to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A key.
    Key(Key),
    /// Text delivered as one bracketed paste (§14.1, ASSIST-082).
    ///
    /// Kept whole rather than replayed as keystrokes: a paste is one event the user can see
    /// and confirm, and splitting it is what turns three pasted lines into three submissions.
    Paste(Vec<u8>),
    /// The terminal changed size.
    Resize {
        /// New column count.
        cols: u16,
        /// New row count.
        rows: u16,
    },
    /// An out-of-band message produced by a worker — a pub/sub push, a reconnect notice.
    Message(Notice),
    /// The user asked to leave.
    Interrupt,
}

/// An out-of-band line to show the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Already escaped for display (§23.5). This type carries text that is safe to print.
    pub text: String,
    /// Where it came from, for the prefix.
    pub origin: NoticeOrigin,
}

/// Who produced a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeOrigin {
    /// A pub/sub delivery.
    Push,
    /// A connection-state change.
    Connection,
    /// Penguin itself.
    Client,
}

impl NoticeOrigin {
    /// Short prefix shown before the text.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Connection => "conn",
            Self::Client => "info",
        }
    }
}

/// Where inputs come from. Implemented once over crossterm, and by tests over a script.
pub trait Source {
    /// Wait up to `timeout` for the next input.
    fn next(&mut self, timeout: Duration) -> Option<Input>;
}

/// A queue a worker thread can push into and the coordinator drains.
///
/// This is the only way a worker reaches the terminal. It never writes; it enqueues, and the
/// coordinator decides when it is safe to print (§14.4).
#[derive(Clone, Debug, Default)]
pub struct Mailbox {
    inner: Arc<Mutex<VecDeque<Input>>>,
}

impl Mailbox {
    /// An empty mailbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue an input from another thread.
    pub fn post(&self, input: Input) {
        if let Ok(mut g) = self.inner.lock() {
            g.push_back(input);
        }
    }

    /// Enqueue a notice.
    pub fn notify(&self, origin: NoticeOrigin, text: impl Into<String>) {
        self.post(Input::Message(Notice {
            text: text.into(),
            origin,
        }));
    }

    /// Take the next input, if any.
    #[must_use]
    pub fn take(&self) -> Option<Input> {
        self.inner.lock().ok()?.pop_front()
    }

    /// How many are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |g| g.len())
    }

    /// Whether nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A source that reads the keyboard first and the mailbox second.
///
/// The order matters: a flood of pub/sub messages must not starve the keyboard, or the user
/// cannot type during a busy subscription (ASSIST-062).
pub struct Merged<K> {
    keys: K,
    mailbox: Mailbox,
}

impl<K: Source> Merged<K> {
    /// Merge a keyboard source with a mailbox.
    pub fn new(keys: K, mailbox: Mailbox) -> Self {
        Self { keys, mailbox }
    }
}

impl<K: Source> Source for Merged<K> {
    fn next(&mut self, timeout: Duration) -> Option<Input> {
        if let Some(k) = self.keys.next(Duration::from_millis(0)) {
            return Some(k);
        }
        if let Some(m) = self.mailbox.take() {
            return Some(m);
        }
        self.keys.next(timeout).or_else(|| self.mailbox.take())
    }
}

/// A source driven by a fixed script. Used by tests, and by the PTY demo binary's `--script`
/// mode so the same code path is exercised with and without a real terminal.
#[derive(Debug, Default)]
pub struct Scripted {
    queue: VecDeque<Input>,
}

impl Scripted {
    /// A source that will yield `inputs` in order and then nothing.
    #[must_use]
    pub fn new(inputs: Vec<Input>) -> Self {
        Self {
            queue: inputs.into(),
        }
    }

    /// Type a string as individual key presses.
    #[must_use]
    pub fn typing(s: &str) -> Vec<Input> {
        s.chars().map(|c| Input::Key(Key::Char(c))).collect()
    }
}

impl Source for Scripted {
    fn next(&mut self, _timeout: Duration) -> Option<Input> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_reaches_the_terminal_only_by_posting() {
        let m = Mailbox::new();
        assert!(m.is_empty());
        let w = m.clone();
        let t = std::thread::spawn(move || {
            for i in 0..5 {
                w.notify(NoticeOrigin::Push, format!("message {i}"));
            }
        });
        t.join().expect("worker");
        assert_eq!(m.len(), 5);
        // Order is preserved, which is what makes a message log readable.
        for i in 0..5 {
            match m.take() {
                Some(Input::Message(n)) => {
                    assert_eq!(n.text, format!("message {i}"));
                    assert_eq!(n.origin, NoticeOrigin::Push);
                }
                other => panic!("expected a message, got {other:?}"),
            }
        }
        assert!(m.is_empty());
    }

    #[test]
    fn the_keyboard_is_served_before_a_backlog_of_messages() {
        // ASSIST-062: a busy subscription must not make the terminal feel stuck.
        let mailbox = Mailbox::new();
        for i in 0..100 {
            mailbox.notify(NoticeOrigin::Push, format!("flood {i}"));
        }
        let mut merged = Merged::new(Scripted::new(Scripted::typing("HG")), mailbox.clone());

        assert_eq!(
            merged.next(Duration::from_millis(0)),
            Some(Input::Key(Key::Char('H'))),
            "a keystroke must not wait behind 100 queued messages"
        );
        assert_eq!(
            merged.next(Duration::from_millis(0)),
            Some(Input::Key(Key::Char('G')))
        );
        // Only once the keyboard is quiet do the messages flow.
        assert!(matches!(
            merged.next(Duration::from_millis(0)),
            Some(Input::Message(_))
        ));
        assert_eq!(mailbox.len(), 99, "the rest are still queued, not dropped");
    }

    #[test]
    fn a_paste_stays_one_event() {
        // Splitting it into keystrokes is what turns three pasted lines into three
        // submissions (§14.1).
        let mut s = Scripted::new(vec![Input::Paste(b"SET a 1\nSET b 2\n".to_vec())]);
        let got = s.next(Duration::from_millis(0)).unwrap();
        match got {
            // Counted by hand rather than with a byte-counting crate: two bytes in a test
            // fixture is not worth a dependency.
            #[allow(clippy::naive_bytecount)]
            Input::Paste(b) => assert_eq!(
                b.iter().filter(|c| **c == b'\n').count(),
                2,
                "both newlines are preserved inside the single paste"
            ),
            other => panic!("expected one paste, got {other:?}"),
        }
        assert_eq!(s.next(Duration::from_millis(0)), None);
    }

    #[test]
    fn notice_origins_are_distinguishable() {
        assert_ne!(NoticeOrigin::Push.label(), NoticeOrigin::Connection.label());
        assert_ne!(NoticeOrigin::Client.label(), NoticeOrigin::Push.label());
    }
}
