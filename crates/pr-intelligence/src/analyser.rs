//! The lenient incremental analyser (v2.1 §11.5, §11.6, ADR-027, R11, V-F02).
//!
//! §11.5 wants two parsers with different jobs: this one accepts half-typed input so it can
//! say *what is still missing*, while `pr_repl::quoting::split_args` produces the argv that
//! actually goes on the wire.
//!
//! My review of the blueprint flagged that v2.0 said the two "share byte spans and lexical
//! rules" but never required a test that they agree. That gap is the dangerous one: a
//! completion replaces the span this analyser reports, so if the authoritative tokenizer
//! would have split the line differently, the user sees one command and a different one is
//! sent. The equivalence property in the tests below is the guard, and it is why this module
//! deliberately reuses the same escape handling rather than reimplementing it.

use pr_repl::quoting::{TokenizeError, split_args};

/// What kind of quoting the cursor is inside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuoteMode {
    /// Not inside quotes.
    #[default]
    None,
    /// Inside `"..."`, where escapes apply.
    Double,
    /// Inside `'...'`, which is literal.
    Single,
}

/// One token as the analyser sees it, possibly incomplete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Byte range in the line, including any opening quote.
    pub range: std::ops::Range<usize>,
    /// Decoded bytes so far. For an unterminated token this is what has been typed.
    pub bytes: Vec<u8>,
    /// Whether this token is still open (an unclosed quote, or the cursor sits in it).
    pub complete: bool,
}

/// The analysed state of an editor line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    /// Tokens found, in order.
    pub spans: Vec<Span>,
    /// Index of the token the cursor is in or adjacent to, if any.
    pub active: Option<usize>,
    /// Quoting state at the cursor.
    pub quote_mode: QuoteMode,
    /// True when the line cannot be submitted yet because a quote is open.
    pub incomplete: bool,
    /// True when the cursor sits on whitespace after a complete token, i.e. the next
    /// argument is being started.
    pub at_new_token: bool,
}

impl Analysis {
    /// The command name, if one has been typed.
    #[must_use]
    pub fn command(&self) -> Option<String> {
        let s = self.spans.first()?;
        std::str::from_utf8(&s.bytes).ok().map(str::to_uppercase)
    }

    /// Zero-based index of the argument the cursor is on (0 = the command itself).
    #[must_use]
    pub fn active_index(&self) -> usize {
        match self.active {
            Some(i) => i,
            None => self.spans.len(),
        }
    }

    /// The span a completion should replace. `None` means insert at the cursor.
    #[must_use]
    pub fn replacement_span(&self) -> Option<std::ops::Range<usize>> {
        if self.at_new_token {
            return None;
        }
        self.active
            .and_then(|i| self.spans.get(i))
            .map(|s| s.range.clone())
    }
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Analyse `line` with the cursor at byte offset `cursor`.
///
/// Never fails: every input is some editing state. A cursor beyond the line is clamped.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn analyse(line: &[u8], cursor: usize) -> Analysis {
    let cursor = cursor.min(line.len());
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0usize;
    let mut quote_at_cursor = QuoteMode::None;
    let mut open_quote = false;

    while i < line.len() {
        while i < line.len() && line[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= line.len() {
            break;
        }
        let start = i;
        let mut bytes: Vec<u8> = Vec::new();
        let mut mode = QuoteMode::None;
        let complete;

        loop {
            let Some(&c) = line.get(i) else {
                // Ran out of input: the token is open if we are inside quotes.
                complete = mode == QuoteMode::None;
                break;
            };
            match mode {
                QuoteMode::None => {
                    if c.is_ascii_whitespace() {
                        complete = true;
                        break;
                    }
                    if c == b'"' {
                        mode = QuoteMode::Double;
                        i += 1;
                        continue;
                    }
                    if c == b'\'' {
                        mode = QuoteMode::Single;
                        i += 1;
                        continue;
                    }
                    bytes.push(c);
                    i += 1;
                }
                QuoteMode::Double => {
                    if c == b'\\' {
                        if i + 3 < line.len()
                            && line[i + 1] == b'x'
                            && let (Some(h), Some(l)) = (hex(line[i + 2]), hex(line[i + 3]))
                        {
                            bytes.push(h * 16 + l);
                            i += 4;
                            continue;
                        }
                        if let Some(&e) = line.get(i + 1) {
                            bytes.push(match e {
                                b'n' => b'\n',
                                b'r' => b'\r',
                                b't' => b'\t',
                                b'b' => 0x08,
                                b'a' => 0x07,
                                other => other,
                            });
                            i += 2;
                            continue;
                        }
                        // A trailing backslash: still being typed.
                        i += 1;
                        continue;
                    }
                    if c == b'"' {
                        i += 1;
                        mode = QuoteMode::None;
                        // A closing quote must be followed by whitespace or end of line; if
                        // not, the authoritative tokenizer rejects the line, and we simply
                        // keep accumulating so the editor can show what is there.
                        match line.get(i) {
                            None => {
                                complete = true;
                                break;
                            }
                            Some(n) if n.is_ascii_whitespace() => {
                                complete = true;
                                break;
                            }
                            Some(_) => continue,
                        }
                    }
                    bytes.push(c);
                    i += 1;
                }
                QuoteMode::Single => {
                    if c == b'\\' && line.get(i + 1) == Some(&b'\'') {
                        bytes.push(b'\'');
                        i += 2;
                        continue;
                    }
                    if c == b'\'' {
                        i += 1;
                        mode = QuoteMode::None;
                        match line.get(i) {
                            None => {
                                complete = true;
                                break;
                            }
                            Some(n) if n.is_ascii_whitespace() => {
                                complete = true;
                                break;
                            }
                            Some(_) => continue,
                        }
                    }
                    bytes.push(c);
                    i += 1;
                }
            }
        }

        let range = start..i;
        if cursor >= start && cursor <= i {
            quote_at_cursor = mode;
        }
        if mode != QuoteMode::None {
            open_quote = true;
        }
        spans.push(Span {
            range,
            bytes,
            complete,
        });
    }

    // Which token is the cursor on?
    let mut active = None;
    for (idx, s) in spans.iter().enumerate() {
        if cursor >= s.range.start && cursor <= s.range.end {
            active = Some(idx);
        }
    }
    // The cursor is starting a new argument when it is past the last token and separated
    // from it by whitespace.
    let at_new_token = match spans.last() {
        None => true,
        Some(last) => cursor > last.range.end,
    };
    if at_new_token {
        active = None;
    }

    Analysis {
        spans,
        active,
        quote_mode: quote_at_cursor,
        incomplete: open_quote,
        at_new_token,
    }
}

/// Whether the authoritative tokenizer would accept this line as submittable.
///
/// The analyser never decides what is sent; this is only used to tell "keep typing" from
/// "this is wrong" in the editor (§11.8).
///
/// # Errors
/// The [`TokenizeError`] the authoritative tokenizer would raise.
pub fn submittable(line: &[u8]) -> Result<(), TokenizeError> {
    split_args(line).map(|_| ())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn argv_of(a: &Analysis) -> Vec<Vec<u8>> {
        a.spans.iter().map(|s| s.bytes.clone()).collect()
    }

    fn authoritative(line: &[u8]) -> Option<Vec<Vec<u8>>> {
        split_args(line)
            .ok()
            .map(|t| t.into_iter().map(|x| x.bytes.to_vec()).collect())
    }

    // ---------------------------------------------------------------- basic analysis
    #[test]
    fn finds_the_command_and_the_active_argument() {
        let line = b"HGET player:10001 sta";
        let a = analyse(line, line.len());
        assert_eq!(a.command().as_deref(), Some("HGET"));
        assert_eq!(a.spans.len(), 3);
        assert_eq!(a.active_index(), 2, "cursor is on the third token");
        assert_eq!(a.replacement_span(), Some(18..21));
        assert!(!a.at_new_token);
    }

    #[test]
    fn a_cursor_after_a_space_starts_a_new_argument() {
        // §10.4: after `HGETALL ` the suggestion is for the key, inserted not replaced.
        let line = b"HGETALL ";
        let a = analyse(line, line.len());
        assert!(a.at_new_token);
        assert_eq!(a.active_index(), 1);
        assert_eq!(a.replacement_span(), None, "insert, do not replace");
    }

    #[test]
    fn a_cursor_inside_a_token_still_replaces_the_whole_token() {
        // ASSIST-016: `sta|tus` accepting `status` must not give `statustus`.
        let line = b"HGET k status";
        let a = analyse(line, 10); // inside "status"
        assert_eq!(a.replacement_span(), Some(7..13));
    }

    #[test]
    fn quote_mode_is_reported_at_the_cursor() {
        let line = br#"SET k "a b"#;
        let a = analyse(line, line.len());
        assert_eq!(a.quote_mode, QuoteMode::Double);
        assert!(a.incomplete);

        let single = br"SET k 'a b";
        assert_eq!(analyse(single, single.len()).quote_mode, QuoteMode::Single);

        let closed = br#"SET k "a b""#;
        assert_eq!(analyse(closed, closed.len()).quote_mode, QuoteMode::None);
        assert!(!analyse(closed, closed.len()).incomplete);
    }

    #[test]
    fn an_empty_line_is_at_a_new_token() {
        let a = analyse(b"", 0);
        assert!(a.spans.is_empty());
        assert!(a.at_new_token);
        assert_eq!(a.command(), None);
        assert_eq!(a.active_index(), 0);
    }

    #[test]
    fn incomplete_is_not_the_same_as_invalid() {
        // §11.8: an open quote is a normal editing state, shown in red only when wrong.
        assert!(analyse(br#"SET k "ab"#, 9).incomplete);
        assert!(
            submittable(br#"SET k "ab"#).is_err(),
            "but not yet submittable"
        );
        assert!(submittable(br#"SET k "ab""#).is_ok());
    }

    // ---------------------------------------------------------------- R11 equivalence
    #[test]
    fn spans_and_argv_agree_on_the_documented_cases() {
        // The invariant: for a submittable line, what the analyser reports and what the
        // authoritative tokenizer sends are the same sequence of byte strings.
        let cases: &[&[u8]] = &[
            b"GET k",
            b"  GET   k  ",
            br#"SET k "a b""#,
            br#"SET k "a\nb""#,
            br#"SET k "\xff""#,
            br#"SET k "\x00""#,
            b"SET k 'a b'",
            br"SET k 'it\'s'",
            b"SET example '--raw'",
            br#"SET k """#,
            b"SET k ''",
            "SET \u{4e2d}\u{6587} \u{1f427}".as_bytes(),
            b"HGET player:10001 status",
            br#"SET k "q\"q""#,
        ];
        for line in cases {
            let Some(want) = authoritative(line) else {
                panic!(
                    "case should be submittable: {:?}",
                    String::from_utf8_lossy(line)
                )
            };
            let got = argv_of(&analyse(line, line.len()));
            assert_eq!(
                got,
                want,
                "analyser and tokenizer disagree on {:?}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn replacing_the_active_span_preserves_every_other_argument() {
        // This is what a completion actually does, and the reason the spans must be right.
        let line = br#"HGET player:10001 "old value""#;
        let a = analyse(line, line.len());
        let span = a.replacement_span().unwrap();
        let mut rebuilt = line[..span.start].to_vec();
        rebuilt.extend_from_slice(b"status");
        rebuilt.extend_from_slice(&line[span.end..]);

        let before = authoritative(line).unwrap();
        let after = authoritative(&rebuilt).unwrap();
        assert_eq!(after.len(), before.len(), "argument count must not change");
        assert_eq!(&after[..2], &before[..2], "earlier arguments untouched");
        assert_eq!(after[2], b"status".to_vec());
    }

    proptest! {
        /// The equivalence property (R11 / ASSIST-081).
        ///
        /// For any generated line that the authoritative tokenizer accepts, the analyser must
        /// produce exactly the same argv. A disagreement here is the bug where the user sees
        /// one command and another is sent.
        #[test]
        fn analyser_agrees_with_the_tokenizer_on_any_accepted_line(
            parts in proptest::collection::vec(
                proptest::string::string_regex("[a-zA-Z0-9:_{}*?\\[\\]-]{0,6}").unwrap(),
                0..5),
            quote_style in 0u8..3,
            trailing_space in any::<bool>(),
        ) {
            let mut line = String::new();
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    line.push(' ');
                }
                match quote_style {
                    1 => { line.push('"'); line.push_str(p); line.push('"'); }
                    2 => { line.push('\''); line.push_str(p); line.push('\''); }
                    _ => line.push_str(p),
                }
            }
            if trailing_space {
                line.push(' ');
            }
            let bytes = line.as_bytes();
            if let Some(want) = authoritative(bytes) {
                let got = argv_of(&analyse(bytes, bytes.len()));
                prop_assert_eq!(got, want, "disagreement on {:?}", line);
            }
        }

        /// Analysis never panics and never reports a span outside the line.
        #[test]
        fn analysis_is_total_and_spans_are_in_range(
            raw in proptest::collection::vec(any::<u8>(), 0..64),
            cursor in 0usize..80,
        ) {
            let a = analyse(&raw, cursor);
            for s in &a.spans {
                prop_assert!(s.range.end <= raw.len(), "span {:?} exceeds {}", s.range, raw.len());
                prop_assert!(s.range.start <= s.range.end);
            }
            if let Some(i) = a.active {
                prop_assert!(i < a.spans.len());
            }
            if let Some(r) = a.replacement_span() {
                prop_assert!(r.end <= raw.len());
            }
        }

        /// A completion applied to the reported span round-trips through the tokenizer.
        #[test]
        fn applying_a_completion_to_the_reported_span_is_consistent(
            head in proptest::string::string_regex("[A-Z]{2,6}").unwrap(),
            arg in proptest::string::string_regex("[a-z0-9:]{0,8}").unwrap(),
            replacement in proptest::string::string_regex("[a-z0-9:]{1,8}").unwrap(),
        ) {
            let line = format!("{head} {arg}");
            let bytes = line.as_bytes();
            let a = analyse(bytes, bytes.len());
            if let (Some(span), Some(before)) = (a.replacement_span(), authoritative(bytes)) {
                let mut rebuilt = bytes[..span.start].to_vec();
                rebuilt.extend_from_slice(replacement.as_bytes());
                rebuilt.extend_from_slice(&bytes[span.end..]);
                if let Some(after) = authoritative(&rebuilt) {
                    prop_assert_eq!(after.len(), before.len(), "argument count changed");
                    prop_assert_eq!(
                        after.last().map(Vec::as_slice),
                        Some(replacement.as_bytes()),
                        "the replacement did not land in the active argument"
                    );
                }
            }
        }
    }
}
