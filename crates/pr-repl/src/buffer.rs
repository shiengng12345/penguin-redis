//! Grapheme-aware line buffer with undo transactions (ADR-026 fallback, V-C01).
//!
//! SPIKE-001 rejected Reedline as the editing engine: its undo lives on a private `Editor`,
//! and `set_insertion_point` accepts a mid-codepoint offset that panics inside the library on
//! the next move. That second point is the decisive one — §12.8 says a late async candidate
//! may carry a **stale span**, and the correct response is to discard it, not to abort the
//! process. So every fallible operation here returns `Err` and nothing panics.
//!
//! What this owes the rest of the system:
//! - the cursor is always on a grapheme boundary
//! - accepting a completion is one [`EditTransaction`]: a single undo puts it all back
//!   (ADR-016)
//! - an edit whose span no longer matches the buffer is refused, so a stale candidate is
//!   rejected rather than applied at the wrong place

use thiserror::Error;

/// Editing failures. All recoverable; none panic.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EditError {
    /// A byte offset was not on a character boundary.
    #[error("offset {0} is not a character boundary")]
    NotACharBoundary(usize),
    /// A range was outside the buffer.
    #[error("range {0}..{1} is outside a buffer of {2} bytes")]
    OutOfRange(usize, usize, usize),
    /// A range was inverted.
    #[error("range {0}..{1} is inverted")]
    InvertedRange(usize, usize),
    /// The buffer changed since the edit was computed (§12.8 stale candidate).
    #[error("buffer revision {expected} is stale; buffer is now at {actual}")]
    StaleRevision {
        /// Revision the edit was computed against.
        expected: u64,
        /// Current revision.
        actual: u64,
    },
    /// The text at the span is not what the edit expected.
    #[error("span no longer contains the expected text")]
    SpanContentChanged,
    /// Nothing left to undo or redo.
    #[error("nothing to {0}")]
    NothingTo(&'static str),
}

/// One applied edit, retained so it can be reversed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AppliedEdit {
    at: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
}

/// A group of edits that undo and redo as one (ADR-016).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Transaction {
    edits: Vec<AppliedEdit>,
    label: &'static str,
}

/// A text replacement computed against a known buffer revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    /// Byte range to replace.
    pub span: std::ops::Range<usize>,
    /// Replacement text.
    pub text: String,
    /// Revision the span was computed against, if the producer knows it.
    pub revision: Option<u64>,
    /// Text the producer believes occupies `span`, if it wants that checked.
    pub expected: Option<String>,
}

impl TextEdit {
    /// A plain replacement with no staleness checking.
    #[must_use]
    pub fn new(span: std::ops::Range<usize>, text: impl Into<String>) -> Self {
        Self {
            span,
            text: text.into(),
            revision: None,
            expected: None,
        }
    }
    /// Bind this edit to a buffer revision, so a late candidate is refused (§12.8).
    #[must_use]
    pub fn at_revision(mut self, r: u64) -> Self {
        self.revision = Some(r);
        self
    }
    /// Also require that the span still contains `expected`.
    #[must_use]
    pub fn expecting(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }
}

/// The editable line.
#[derive(Clone, Debug, Default)]
pub struct LineBuffer {
    text: String,
    cursor: usize,
    revision: u64,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    open: Option<Transaction>,
}

fn is_boundary(s: &str, i: usize) -> bool {
    i == s.len() || s.is_char_boundary(i)
}

/// Previous grapheme boundary before `i`.
///
/// Approximates UAX#29 well enough for editing: it steps back over a base character and then
/// over any combining marks, ZWJ-joined sequences and variation selectors attached to it.
fn prev_grapheme(text: &str, from: usize) -> usize {
    if from == 0 {
        return 0;
    }
    let mut at = from;
    loop {
        at -= 1;
        while at > 0 && !text.is_char_boundary(at) {
            at -= 1;
        }
        let Some(ch) = text[at..].chars().next() else {
            return at;
        };
        if !is_extending(ch) {
            // If the char before this one is a ZWJ, keep going: we are inside a cluster.
            let before = prev_char_start(text, at);
            if before < at && text[before..].starts_with('\u{200d}') {
                at = before;
                continue;
            }
            return at;
        }
        if at == 0 {
            return 0;
        }
    }
}

fn prev_char_start(text: &str, from: usize) -> usize {
    if from == 0 {
        return 0;
    }
    let mut at = from - 1;
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Next grapheme boundary after `i`.
fn next_grapheme(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut j = i;
    let Some(first) = s[j..].chars().next() else {
        return s.len();
    };
    j += first.len_utf8();
    loop {
        let Some(c) = s[j..].chars().next() else {
            return j;
        };
        if is_extending(c) {
            j += c.len_utf8();
            continue;
        }
        if c == '\u{200d}' {
            j += c.len_utf8();
            if let Some(after) = s[j..].chars().next() {
                j += after.len_utf8();
            }
            continue;
        }
        return j;
    }
}

/// Combining marks, variation selectors, skin-tone modifiers and joiners.
fn is_extending(c: char) -> bool {
    matches!(u32::from(c),
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x1F3FB..=0x1F3FF | 0xE0100..=0xE01EF
        | 0x200D)
}

impl LineBuffer {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current contents.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor byte offset. Always on a grapheme boundary.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Monotonic revision, incremented on every mutation (§12.8).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True when nothing has been typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Move the cursor, refusing a non-boundary offset instead of panicking later.
    ///
    /// # Errors
    /// [`EditError::NotACharBoundary`] or [`EditError::OutOfRange`].
    pub fn set_cursor(&mut self, at: usize) -> Result<(), EditError> {
        if at > self.text.len() {
            return Err(EditError::OutOfRange(at, at, self.text.len()));
        }
        if !is_boundary(&self.text, at) {
            return Err(EditError::NotACharBoundary(at));
        }
        self.cursor = at;
        Ok(())
    }

    /// Move one grapheme left.
    pub fn move_left(&mut self) {
        self.cursor = prev_grapheme(&self.text, self.cursor);
    }

    /// Move one grapheme right.
    pub fn move_right(&mut self) {
        self.cursor = next_grapheme(&self.text, self.cursor);
    }

    /// Begin a group; every edit until [`LineBuffer::commit`] undoes as one (ADR-016).
    pub fn begin(&mut self, label: &'static str) {
        self.open = Some(Transaction {
            edits: Vec::new(),
            label,
        });
    }

    /// Close the open group.
    pub fn commit(&mut self) {
        if let Some(t) = self.open.take()
            && !t.edits.is_empty()
        {
            self.undo.push(t);
            self.redo.clear();
        }
    }

    fn record(&mut self, e: AppliedEdit) {
        if let Some(t) = &mut self.open {
            t.edits.push(e);
        } else {
            self.undo.push(Transaction {
                edits: vec![e],
                label: "edit",
            });
            self.redo.clear();
        }
    }

    /// Insert text at the cursor.
    ///
    /// # Errors
    /// Never fails in practice; returns [`EditError`] for symmetry with [`LineBuffer::apply`].
    pub fn insert(&mut self, s: &str) -> Result<(), EditError> {
        let at = self.cursor;
        self.text.insert_str(at, s);
        self.record(AppliedEdit {
            at,
            removed: String::new(),
            inserted: s.to_owned(),
            cursor_before: at,
        });
        self.cursor = at + s.len();
        self.revision += 1;
        Ok(())
    }

    /// Delete the grapheme before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = prev_grapheme(&self.text, self.cursor);
        let removed = self.text[start..self.cursor].to_owned();
        let cursor_before = self.cursor;
        self.text.replace_range(start..self.cursor, "");
        self.record(AppliedEdit {
            at: start,
            removed,
            inserted: String::new(),
            cursor_before,
        });
        self.cursor = start;
        self.revision += 1;
    }

    /// Apply an external edit, e.g. accepting a completion candidate.
    ///
    /// # Errors
    /// [`EditError::StaleRevision`] when the buffer moved on (the caller should discard the
    /// candidate), [`EditError::SpanContentChanged`], [`EditError::OutOfRange`],
    /// [`EditError::InvertedRange`] or [`EditError::NotACharBoundary`].
    pub fn apply(&mut self, edit: &TextEdit) -> Result<(), EditError> {
        if let Some(r) = edit.revision
            && r != self.revision
        {
            return Err(EditError::StaleRevision {
                expected: r,
                actual: self.revision,
            });
        }
        let (s, e) = (edit.span.start, edit.span.end);
        if s > e {
            return Err(EditError::InvertedRange(s, e));
        }
        if e > self.text.len() {
            return Err(EditError::OutOfRange(s, e, self.text.len()));
        }
        if !is_boundary(&self.text, s) {
            return Err(EditError::NotACharBoundary(s));
        }
        if !is_boundary(&self.text, e) {
            return Err(EditError::NotACharBoundary(e));
        }
        let removed = self.text[s..e].to_owned();
        if let Some(exp) = &edit.expected
            && *exp != removed
        {
            return Err(EditError::SpanContentChanged);
        }
        let cursor_before = self.cursor;
        self.text.replace_range(s..e, &edit.text);
        self.record(AppliedEdit {
            at: s,
            removed,
            inserted: edit.text.clone(),
            cursor_before,
        });
        // The cursor follows the edit — the thing Reedline does not do (SPIKE-001 A2).
        self.cursor = s + edit.text.len();
        self.revision += 1;
        Ok(())
    }

    /// Accept a completion: one transaction, one undo (ADR-016, ASSIST-020).
    ///
    /// # Errors
    /// As [`LineBuffer::apply`]. On failure nothing is changed.
    pub fn accept_completion(&mut self, edit: &TextEdit) -> Result<(), EditError> {
        self.begin("accept completion");
        let r = self.apply(edit);
        if r.is_err() {
            self.open = None; // leave no empty transaction behind
        } else {
            self.commit();
        }
        r
    }

    fn reverse(&mut self, t: &Transaction) {
        for e in t.edits.iter().rev() {
            let end = e.at + e.inserted.len();
            self.text.replace_range(e.at..end, &e.removed);
            self.cursor = e.cursor_before;
        }
        self.revision += 1;
    }

    fn forward(&mut self, t: &Transaction) {
        for e in &t.edits {
            let end = e.at + e.removed.len();
            self.text.replace_range(e.at..end, &e.inserted);
            self.cursor = e.at + e.inserted.len();
        }
        self.revision += 1;
    }

    /// Undo the last transaction.
    ///
    /// # Errors
    /// [`EditError::NothingTo`] when the stack is empty.
    pub fn undo(&mut self) -> Result<(), EditError> {
        let t = self.undo.pop().ok_or(EditError::NothingTo("undo"))?;
        self.reverse(&t);
        self.redo.push(t);
        Ok(())
    }

    /// Redo the last undone transaction.
    ///
    /// # Errors
    /// [`EditError::NothingTo`] when the stack is empty.
    pub fn redo(&mut self) -> Result<(), EditError> {
        let t = self.redo.pop().ok_or(EditError::NothingTo("redo"))?;
        self.forward(&t);
        self.undo.push(t);
        Ok(())
    }

    /// Number of undoable transactions.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Replace the whole buffer, e.g. recalling a history entry.
    pub fn set_text(&mut self, s: &str) {
        self.begin("set text");
        let removed = std::mem::take(&mut self.text);
        let cursor_before = self.cursor;
        self.text.push_str(s);
        self.record(AppliedEdit {
            at: 0,
            removed,
            inserted: s.to_owned(),
            cursor_before,
        });
        self.cursor = self.text.len();
        self.revision += 1;
        self.commit();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn buf(s: &str) -> LineBuffer {
        let mut b = LineBuffer::new();
        b.insert(s).unwrap();
        b
    }

    // ---------------------------------------------------------------- SPIKE-001 (a)
    #[test]
    fn external_edit_is_byte_exact_and_moves_the_cursor() {
        // Reedline applied the text but left the cursor behind (SPIKE-001 A2).
        let mut b = buf("HGET player:10001 sta");
        b.apply(&TextEdit::new(18..21, "status")).unwrap();
        assert_eq!(b.text(), "HGET player:10001 status");
        assert_eq!(b.cursor(), 24, "cursor must follow the edit");
    }

    #[test]
    fn multibyte_replacement_is_exact() {
        let mut b = buf("SET k 中文");
        b.apply(&TextEdit::new(6..12, "🐧x")).unwrap();
        assert_eq!(b.text(), "SET k 🐧x");
    }

    #[test]
    fn partial_token_edit_does_not_duplicate_the_suffix() {
        // ASSIST-016: `sta|tus` accepting `status` must not give `statustus`.
        let mut b = buf("HGET player:10001 status");
        b.set_cursor(21).unwrap();
        b.apply(&TextEdit::new(18..24, "status")).unwrap();
        assert_eq!(b.text(), "HGET player:10001 status");
    }

    // ---------------------------------------------------------------- boundary safety
    #[test]
    fn a_mid_codepoint_cursor_is_refused_not_deferred_to_a_panic() {
        // The decisive SPIKE-001 finding: Reedline accepted this and panicked later.
        let mut b = buf("中文");
        assert_eq!(b.set_cursor(1), Err(EditError::NotACharBoundary(1)));
        assert_eq!(b.cursor(), 6, "cursor unchanged after a refused move");
        b.move_left(); // must not panic
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn edits_on_bad_boundaries_are_refused() {
        let mut b = buf("中文");
        assert_eq!(
            b.apply(&TextEdit::new(1..3, "x")),
            Err(EditError::NotACharBoundary(1))
        );
        assert_eq!(
            b.apply(&TextEdit::new(0..4, "x")),
            Err(EditError::NotACharBoundary(4))
        );
        assert_eq!(
            b.apply(&TextEdit::new(0..99, "x")),
            Err(EditError::OutOfRange(0, 99, 6))
        );
        let inverted = std::ops::Range { start: 3, end: 0 };
        assert_eq!(
            b.apply(&TextEdit::new(inverted, "x")),
            Err(EditError::InvertedRange(3, 0))
        );
        assert_eq!(b.text(), "中文", "a refused edit changes nothing");
    }

    // ---------------------------------------------------------------- §12.8 staleness
    #[test]
    fn a_stale_candidate_is_refused_rather_than_applied_at_the_wrong_place() {
        // ASSIST-026: a late async result must be discarded, not land on new text.
        let mut b = buf("HG");
        let rev = b.revision();
        b.insert("ETALL").unwrap(); // user kept typing
        let late = TextEdit::new(0..2, "HGET").at_revision(rev);
        assert_eq!(
            b.apply(&late),
            Err(EditError::StaleRevision {
                expected: rev,
                actual: b.revision()
            })
        );
        assert_eq!(b.text(), "HGETALL", "buffer untouched");
    }

    #[test]
    fn span_content_check_catches_a_moved_token() {
        let mut b = buf("GET foo");
        let e = TextEdit::new(4..7, "foobar").expecting("bar");
        assert_eq!(b.apply(&e), Err(EditError::SpanContentChanged));
        assert_eq!(b.text(), "GET foo");
        let ok = TextEdit::new(4..7, "foobar").expecting("foo");
        assert!(b.apply(&ok).is_ok());
        assert_eq!(b.text(), "GET foobar");
    }

    #[test]
    fn a_current_revision_edit_applies() {
        let mut b = buf("HG");
        let e = TextEdit::new(0..2, "HGETALL").at_revision(b.revision());
        assert!(b.apply(&e).is_ok());
        assert_eq!(b.text(), "HGETALL");
    }

    // ---------------------------------------------------------------- SPIKE-001 (b) undo
    #[test]
    fn accepting_a_completion_is_a_single_undo() {
        // ADR-016 / ASSIST-020. Reedline could not do this at all: Editor is private.
        let mut b = buf("HGET player:10001 sta");
        let before = b.text().to_owned();
        b.accept_completion(&TextEdit::new(18..21, "status"))
            .unwrap();
        assert_eq!(b.text(), "HGET player:10001 status");
        assert_eq!(b.undo_depth(), 2, "one for typing, one for the completion");
        b.undo().unwrap();
        assert_eq!(b.text(), before, "one undo reverts the whole completion");
    }

    #[test]
    fn a_failed_completion_leaves_no_transaction() {
        let mut b = buf("abc");
        let depth = b.undo_depth();
        assert!(b.accept_completion(&TextEdit::new(0..99, "x")).is_err());
        assert_eq!(b.undo_depth(), depth, "no empty transaction left behind");
        assert_eq!(b.text(), "abc");
    }

    #[test]
    fn undo_and_redo_round_trip() {
        let mut b = LineBuffer::new();
        b.insert("HGET").unwrap();
        b.insert(" k").unwrap();
        assert_eq!(b.text(), "HGET k");
        b.undo().unwrap();
        assert_eq!(b.text(), "HGET");
        b.undo().unwrap();
        assert_eq!(b.text(), "");
        assert_eq!(b.undo(), Err(EditError::NothingTo("undo")));
        b.redo().unwrap();
        assert_eq!(b.text(), "HGET");
        b.redo().unwrap();
        assert_eq!(b.text(), "HGET k");
        assert_eq!(b.redo(), Err(EditError::NothingTo("redo")));
    }

    #[test]
    fn grouped_edits_undo_together() {
        let mut b = LineBuffer::new();
        b.begin("guide insert");
        b.insert("SET").unwrap();
        b.insert(" k").unwrap();
        b.insert(" v").unwrap();
        b.commit();
        assert_eq!(b.undo_depth(), 1);
        b.undo().unwrap();
        assert_eq!(b.text(), "", "the whole group reverts at once");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut b = buf("a");
        b.undo().unwrap();
        b.insert("b").unwrap();
        assert_eq!(b.redo(), Err(EditError::NothingTo("redo")));
    }

    // ---------------------------------------------------------------- graphemes
    #[test]
    fn movement_is_grapheme_wise_not_byte_wise() {
        let mut b = buf("a中🐧");
        assert_eq!(b.cursor(), 8);
        b.move_left();
        assert_eq!(b.cursor(), 4, "skipped the whole 4-byte penguin");
        b.move_left();
        assert_eq!(b.cursor(), 1, "skipped the 3-byte 中");
        b.move_left();
        assert_eq!(b.cursor(), 0);
        b.move_left();
        assert_eq!(b.cursor(), 0, "clamps at the start");
        b.move_right();
        assert_eq!(b.cursor(), 1);
    }

    #[test]
    fn combining_marks_move_as_one_grapheme() {
        // ASSIST-018: e + combining acute is one cluster.
        let mut b = buf("e\u{301}x");
        assert_eq!(b.cursor(), 4);
        b.move_left();
        assert_eq!(b.cursor(), 3);
        b.move_left();
        assert_eq!(b.cursor(), 0, "e and its mark move together");
    }

    #[test]
    fn zwj_sequences_move_as_one_grapheme() {
        let fam = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let mut b = buf(&format!("a{fam}b"));
        b.move_left(); // over 'b'
        let after_b = b.cursor();
        b.move_left(); // over the whole family
        assert_eq!(
            b.cursor(),
            1,
            "ZWJ family is one cluster, got {} from {after_b}",
            b.cursor()
        );
    }

    #[test]
    fn backspace_deletes_a_whole_grapheme() {
        let mut b = buf("a🐧");
        b.backspace();
        assert_eq!(b.text(), "a", "one backspace removes the whole emoji");
        let mut c = buf("e\u{301}");
        c.backspace();
        assert_eq!(c.text(), "", "mark and base go together");
        let mut d = LineBuffer::new();
        d.backspace(); // must not panic on an empty buffer
        assert_eq!(d.text(), "");
    }

    // ---------------------------------------------------------------- misc
    #[test]
    fn revision_advances_on_every_mutation() {
        let mut b = LineBuffer::new();
        let r0 = b.revision();
        b.insert("a").unwrap();
        let r1 = b.revision();
        assert!(r1 > r0);
        b.backspace();
        assert!(b.revision() > r1);
    }

    #[test]
    fn set_text_replaces_and_is_undoable() {
        let mut b = buf("typed");
        b.set_text("HGETALL player:10001");
        assert_eq!(b.text(), "HGETALL player:10001");
        assert_eq!(b.cursor(), 20);
        b.undo().unwrap();
        assert_eq!(b.text(), "typed", "history recall is one undo");
    }

    #[test]
    fn every_byte_value_survives_as_escaped_text() {
        // The buffer holds UTF-8 text (ADR-027); binary arrives already escaped.
        let mut escaped = String::new();
        for byte in 0u8..=255 {
            use std::fmt::Write as _;
            let _ = write!(escaped, "\\x{byte:02x}");
        }
        let mut b = LineBuffer::new();
        b.insert(&escaped).unwrap();
        assert_eq!(b.text(), escaped);
        assert_eq!(b.cursor(), escaped.len());
    }
}
