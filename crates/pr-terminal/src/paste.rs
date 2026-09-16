//! Pasting, and the staging view that stops it executing (v2.1 §14.1, §24.3, V-C03).
//!
//! §14.1 is unambiguous about what must never happen: **nothing pasted is executed
//! automatically.** Three lines pasted into a terminal that treats newlines as Enter is three
//! commands the user has not read, and one of them may be `FLUSHALL`.
//!
//! Bracketed paste (`CSI ? 2004 h`) makes this easy where it is supported: the terminal
//! brackets the content and we know. Where it is not — `TERM=dumb`, some Windows consoles, a
//! multiplexer that swallows the mode — §14.1 specifies a timing heuristic instead: input
//! arriving in a burst (gaps under 10 ms) that contains a newline is a paste.
//!
//! The heuristic is deliberately biased. §14.1 says "无法判定时宁可多进 staging" — when in
//! doubt, stage. A false positive costs the user one keypress to confirm; a false negative
//! runs their commands.

use std::time::Duration;

/// §14.1: the inter-arrival gap below which input is a burst rather than typing.
pub const BURST_GAP: Duration = Duration::from_millis(10);

/// §24.3: a paste this long goes to staging whatever its timing says.
///
/// Not a memory limit — it is about what a person can be expected to have read. A hundred and
/// twenty characters is roughly a long command; more than that arriving at once is worth a
/// look.
pub const INLINE_LIMIT: usize = 120;

/// One byte of input and when it arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keystroke {
    /// The byte.
    pub byte: u8,
    /// Milliseconds since the session started.
    pub at_ms: u64,
}

/// What a run of input turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// Typed by a person, character by character.
    Typed(Vec<u8>),
    /// Arrived as a unit. Goes to staging.
    Pasted(Vec<u8>),
}

impl Segment {
    /// The bytes, whichever it is.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Typed(b) | Self::Pasted(b) => b,
        }
    }
    /// Whether this must go to the staging view.
    #[must_use]
    pub fn is_paste(&self) -> bool {
        matches!(self, Self::Pasted(_))
    }
}

/// Split a trace of keystrokes into typed and pasted runs (§14.1).
///
/// A burst is a maximal run whose consecutive arrivals are less than `gap` apart. It counts
/// as a paste when it contains a newline, or when it is longer than [`INLINE_LIMIT`].
///
/// A newline arriving *slowly* is just the user pressing Enter, which is why the gap and the
/// newline are both required: without the newline test, holding a key down would stage;
/// without the gap test, every submitted line would.
#[must_use]
pub fn classify(trace: &[Keystroke], gap: Duration) -> Vec<Segment> {
    let gap_ms = u64::try_from(gap.as_millis()).unwrap_or(u64::MAX);
    let mut out: Vec<Segment> = Vec::new();
    let mut burst: Vec<Keystroke> = Vec::new();

    let flush = |burst: &mut Vec<Keystroke>, out: &mut Vec<Segment>| {
        if burst.is_empty() {
            return;
        }
        let bytes: Vec<u8> = burst.iter().map(|k| k.byte).collect();
        let multiline = bytes.iter().any(|b| *b == b'\n' || *b == b'\r');
        let long = bytes.len() > INLINE_LIMIT;
        // A single keystroke is never a burst, however fast it arrived: there is nothing for
        // it to have arrived faster *than*.
        let is_paste = burst.len() > 1 && (multiline || long);
        out.push(if is_paste {
            Segment::Pasted(bytes)
        } else {
            Segment::Typed(bytes)
        });
        burst.clear();
    };

    for k in trace {
        match burst.last() {
            Some(prev) if k.at_ms.saturating_sub(prev.at_ms) < gap_ms => burst.push(*k),
            Some(_) => {
                flush(&mut burst, &mut out);
                burst.push(*k);
            }
            None => burst.push(*k),
        }
    }
    flush(&mut burst, &mut out);
    out
}

/// What the user chose to do with staged content (§14.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Submit each line as its own command, in order.
    OneByOne,
    /// Submit everything as a single multi-line command.
    SingleCommand,
    /// Discard it.
    Cancel,
}

/// The paste-staging view: what arrived, line by line, before anything is sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Staging {
    lines: Vec<String>,
    focused: usize,
    /// Set once the user chooses; nothing leaves until then.
    decided: Option<Choice>,
}

impl Staging {
    /// Stage pasted bytes.
    ///
    /// Line endings are normalised so a file pasted from Windows does not arrive with a stray
    /// carriage return on every line — that would change the bytes actually sent.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let lines: Vec<String> = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(str::to_owned)
            .collect();
        Self {
            lines,
            focused: 0,
            decided: None,
        }
    }

    /// The staged lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// How many lines are staged.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether nothing is staged.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Which line has focus.
    #[must_use]
    pub fn focused(&self) -> usize {
        self.focused
    }

    /// Move focus, clamped.
    pub fn focus(&mut self, index: usize) {
        self.focused = index.min(self.lines.len().saturating_sub(1));
    }

    /// Replace the focused line.
    pub fn edit(&mut self, text: impl Into<String>) {
        if let Some(l) = self.lines.get_mut(self.focused) {
            *l = text.into();
        }
    }

    /// Remove the focused line.
    pub fn delete(&mut self) {
        if self.focused < self.lines.len() {
            self.lines.remove(self.focused);
            self.focused = self.focused.min(self.lines.len().saturating_sub(1));
        }
    }

    /// What the user decided, if anything.
    #[must_use]
    pub fn decision(&self) -> Option<Choice> {
        self.decided
    }

    /// Record the user's choice and produce what should be submitted.
    ///
    /// Empty lines are dropped: a trailing newline on a pasted block would otherwise submit a
    /// blank command, and a blank line in the middle of a file would too.
    pub fn decide(&mut self, choice: Choice) -> Vec<String> {
        self.decided = Some(choice);
        let kept: Vec<String> = self
            .lines
            .iter()
            .map(|l| l.trim_end().to_owned())
            .filter(|l| !l.is_empty())
            .collect();
        match choice {
            Choice::Cancel => Vec::new(),
            Choice::OneByOne => kept,
            Choice::SingleCommand => {
                if kept.is_empty() {
                    Vec::new()
                } else {
                    vec![kept.join("\n")]
                }
            }
        }
    }
}

/// Whether pasted bytes need the staging view at all.
///
/// A single line goes straight into the edit buffer — unsubmitted, but not worth a modal for.
/// Anything with a newline, or longer than [`INLINE_LIMIT`], is staged.
#[must_use]
pub fn needs_staging(bytes: &[u8]) -> bool {
    let has_newline = bytes
        .iter()
        .any(|b| *b == b'\n' || (*b == b'\r' && bytes.len() > 1));
    has_newline || bytes.len() > INLINE_LIMIT
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A trace where every keystroke is `step_ms` after the previous one.
    fn trace(text: &str, step_ms: u64, start_ms: u64) -> Vec<Keystroke> {
        text.bytes()
            .enumerate()
            .map(|(i, byte)| Keystroke {
                byte,
                at_ms: start_ms + u64::try_from(i).unwrap_or(0) * step_ms,
            })
            .collect()
    }

    #[test]
    fn a_fast_multiline_burst_is_a_paste() {
        let t = trace("SET a 1\nSET b 2\nDEL c\n", 1, 0);
        let segs = classify(&t, BURST_GAP);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].is_paste(), "{segs:?}");
    }

    #[test]
    fn ordinary_typing_is_not_a_paste_even_across_an_enter() {
        // A human pressing Enter and carrying on is the false positive that would make the
        // REPL unusable.
        let t = trace("GET k\nGET j\n", 90, 0);
        let segs = classify(&t, BURST_GAP);
        assert!(
            segs.iter().all(|s| !s.is_paste()),
            "typing was staged: {segs:?}"
        );
    }

    #[test]
    fn a_fast_typist_is_still_a_typist() {
        // 15 ms between keys is very fast for a person and still above the threshold.
        let t = trace("HGETALL player:10001\n", 15, 0);
        assert!(classify(&t, BURST_GAP).iter().all(|s| !s.is_paste()));
    }

    #[test]
    fn key_repeat_without_a_newline_is_not_a_paste() {
        // Holding a key down produces a very fast burst with no newline in it.
        let t = trace(&"a".repeat(40), 3, 0);
        let segs = classify(&t, BURST_GAP);
        assert_eq!(segs.len(), 1);
        assert!(!segs[0].is_paste(), "held key was mistaken for a paste");
    }

    #[test]
    fn a_very_long_single_line_burst_is_staged_anyway() {
        // §24.3: more than a person can be expected to have read.
        let t = trace(&"x".repeat(INLINE_LIMIT + 1), 1, 0);
        assert!(classify(&t, BURST_GAP)[0].is_paste());
        let t = trace(&"x".repeat(INLINE_LIMIT), 1, 0);
        assert!(
            !classify(&t, BURST_GAP)[0].is_paste(),
            "exactly at the limit"
        );
    }

    #[test]
    fn a_single_keystroke_is_never_a_burst() {
        // There is nothing for one keystroke to have arrived faster than.
        let t = vec![Keystroke {
            byte: b'\n',
            at_ms: 0,
        }];
        assert_eq!(classify(&t, BURST_GAP), vec![Segment::Typed(vec![b'\n'])]);
    }

    #[test]
    fn typing_then_pasting_splits_into_two_segments() {
        let mut t = trace("GET ", 80, 0);
        t.extend(trace("a\nb\nc\n", 1, 1000));
        let segs = classify(&t, BURST_GAP);
        assert!(segs.len() >= 2, "{segs:?}");
        assert!(!segs[0].is_paste());
        assert!(segs.last().unwrap().is_paste());
        // Nothing was lost in the split.
        let all: Vec<u8> = segs.iter().flat_map(|s| s.bytes().to_vec()).collect();
        assert_eq!(all.len(), t.len());
    }

    #[test]
    fn crlf_pastes_are_normalised_so_the_bytes_sent_are_the_ones_meant() {
        let s = Staging::new(b"SET a 1\r\nSET b 2\r\n");
        assert_eq!(s.lines(), ["SET a 1", "SET b 2", ""]);
        assert!(s.lines().iter().all(|l| !l.contains('\r')));
    }

    #[test]
    fn a_lone_carriage_return_still_separates_lines() {
        // Classic Mac line endings, and what some terminals send for Enter.
        let s = Staging::new(b"SET a 1\rSET b 2");
        assert_eq!(s.lines(), ["SET a 1", "SET b 2"]);
    }

    #[test]
    fn staging_holds_everything_until_the_user_decides() {
        let mut s = Staging::new(b"GET a\nGET b\n");
        assert_eq!(s.decision(), None, "nothing is decided on arrival");
        assert_eq!(s.len(), 3, "two commands and the trailing empty line");

        let out = s.decide(Choice::OneByOne);
        assert_eq!(out, ["GET a", "GET b"], "the blank line is dropped");
        assert_eq!(s.decision(), Some(Choice::OneByOne));
    }

    #[test]
    fn submitting_as_one_command_keeps_the_lines_together() {
        let mut s = Staging::new(b"EVAL \"return 1\"\n0\n");
        assert_eq!(s.decide(Choice::SingleCommand), ["EVAL \"return 1\"\n0"]);
    }

    #[test]
    fn cancelling_submits_nothing() {
        let mut s = Staging::new(b"FLUSHALL\nFLUSHDB\n");
        assert!(s.decide(Choice::Cancel).is_empty());
        assert_eq!(s.decision(), Some(Choice::Cancel));
    }

    #[test]
    fn a_line_can_be_edited_or_removed_before_anything_runs() {
        // The point of the view: the dangerous line in a pasted block can be taken out.
        let mut s = Staging::new(b"GET a\nFLUSHALL\nGET b\n");
        s.focus(1);
        assert_eq!(s.lines()[1], "FLUSHALL");
        s.delete();
        assert_eq!(s.decide(Choice::OneByOne), ["GET a", "GET b"]);

        let mut s = Staging::new(b"GET a\nFLUSHALL\n");
        s.focus(1);
        s.edit("DBSIZE");
        assert_eq!(s.decide(Choice::OneByOne), ["GET a", "DBSIZE"]);
    }

    #[test]
    fn focus_is_clamped_rather_than_allowed_to_point_past_the_end() {
        let mut s = Staging::new(b"one\ntwo");
        s.focus(99);
        assert_eq!(s.focused(), 1);
        s.delete();
        s.delete();
        assert!(s.is_empty());
        s.focus(5);
        assert_eq!(s.focused(), 0);
        s.delete(); // must not panic on an empty staging
    }

    #[test]
    fn a_single_line_paste_does_not_need_the_staging_view() {
        assert!(!needs_staging(b"GET mykey"));
        assert!(needs_staging(b"GET a\nGET b"));
        assert!(needs_staging(&[b'x'; INLINE_LIMIT + 1]));
        assert!(!needs_staging(&[b'x'; INLINE_LIMIT]));
    }

    #[test]
    fn a_trailing_newline_alone_still_stages() {
        // `SET a 1\n` pasted as a block is a command plus an Enter; running it because the
        // newline looked like a submission is exactly the failure §14.1 forbids.
        assert!(needs_staging(b"SET a 1\n"));
    }
}
