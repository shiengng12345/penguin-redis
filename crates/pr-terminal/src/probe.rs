//! Measuring what the terminal actually does with a character (v2.1 §14.6, V-C04).
//!
//! §14.6 settles a question that has no universal answer: terminals and fonts disagree about
//! East Asian Ambiguous characters, ZWJ sequences and variation selectors, so "the display
//! width is correct" is defined as *consistent with a declared policy*, not as correct
//! everywhere.
//!
//! A policy can still be wrong for the terminal in front of the user, and this is how that is
//! found out: print a character, ask the terminal where the cursor ended up (`CSI 6 n`), and
//! compare. When the answer contradicts the policy, tables switch to
//! [`DrawMode::CursorReset`](pr_render::DrawMode::CursorReset) so the frame stays aligned even
//! though the content width is being judged wrongly.
//!
//! Two constraints from §14.6 are structural here rather than advisory:
//!
//! - **TTY only.** A cursor report requires a terminal to answer. Emitting `CSI 6 n` into a
//!   pipe writes an escape sequence into the user's data and waits for a reply that will never
//!   come.
//! - **User-triggered only.** Probing writes to the screen and blocks for a reply. Doing that
//!   during startup would make every session pay for it, and would hang against anything that
//!   looks like a terminal but does not answer.

use pr_render::{DrawMode, WidthPolicy};

/// What one character measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// The character probed.
    pub ch: char,
    /// Columns the policy predicted.
    pub assumed: usize,
    /// Columns the terminal actually used.
    pub measured: usize,
}

impl Sample {
    /// Whether the terminal agreed with the policy.
    #[must_use]
    pub fn agrees(self) -> bool {
        self.assumed == self.measured
    }
}

/// Why a probe did not run.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProbeError {
    /// Output is not a terminal.
    #[error("width probing needs a terminal; output is not a tty")]
    NotATty,
    /// The terminal did not answer in time.
    ///
    /// Not every terminal implements the report, and a multiplexer may swallow it. Giving up
    /// keeps the policy, which is a defined behaviour rather than a hang.
    #[error("the terminal did not report a cursor position")]
    NoReply,
    /// The reply was not a cursor position report.
    #[error("unrecognised reply to the cursor position request")]
    BadReply,
}

/// The characters worth measuring.
///
/// Chosen because each one is a case terminals are known to disagree on, not because they are
/// common: an agreement on `a` proves nothing.
#[must_use]
pub fn representative_chars() -> Vec<char> {
    vec![
        '中',        // East Asian Wide — 2 under any table, so a disagreement here is severe
        '→',         // Ambiguous — 1 or 2 depending on the terminal and font
        '±',         // Ambiguous
        '§',         // Ambiguous
        '①',         // Ambiguous
        '\u{1f427}', // 🐧 — emoji presentation, usually 2
        '\u{301}',   // combining acute — zero width, and often rendered as if it were not
    ]
}

/// The bytes that measure one character.
///
/// Return to column zero, clear the line, print the character, ask where the cursor is. The
/// clearing matters: without it the measurement includes whatever was already on the line.
#[must_use]
pub fn probe_sequence(ch: char) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(b"\r\x1b[K");
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    out.extend_from_slice(b"\x1b[6n");
    out
}

/// The bytes that clean up after a probe.
#[must_use]
pub fn probe_cleanup() -> Vec<u8> {
    b"\r\x1b[K".to_vec()
}

/// Parse a cursor position report, `ESC [ row ; col R`, into one-based `(row, col)`.
///
/// Tolerant of leading noise, because a terminal may deliver keystrokes the user typed while
/// the probe was in flight, and losing the report because something else arrived first would
/// turn a measurable terminal into an unmeasurable one.
#[must_use]
pub fn parse_cursor_report(bytes: &[u8]) -> Option<(usize, usize)> {
    let start = bytes
        .windows(2)
        .rposition(|w| w == b"\x1b[")
        .map(|i| i + 2)?;
    let end = start + bytes[start..].iter().position(|b| *b == b'R')?;
    let body = std::str::from_utf8(&bytes[start..end]).ok()?;
    let (r, c) = body.split_once(';')?;
    Some((r.trim().parse().ok()?, c.trim().parse().ok()?))
}

/// Run the probe against a terminal, given something that can write and read it back.
///
/// The transport is a closure so this is testable against a simulated terminal — a probe that
/// can only be exercised by a human at a real terminal is a probe nobody runs.
///
/// # Errors
/// [`ProbeError`] when the terminal is not a tty, does not answer, or answers with something
/// else.
pub fn run<F>(policy: WidthPolicy, is_tty: bool, mut exchange: F) -> Result<Vec<Sample>, ProbeError>
where
    F: FnMut(&[u8]) -> Option<Vec<u8>>,
{
    if !is_tty {
        return Err(ProbeError::NotATty);
    }
    let mut samples = Vec::new();
    for ch in representative_chars() {
        let reply = exchange(&probe_sequence(ch)).ok_or(ProbeError::NoReply)?;
        let (_, col) = parse_cursor_report(&reply).ok_or(ProbeError::BadReply)?;
        // The report is one-based and the cursor sits *after* the character, so a character
        // printed at column 1 that occupies two cells reports column 3.
        samples.push(Sample {
            ch,
            assumed: policy.char_width(ch),
            measured: col.saturating_sub(1),
        });
    }
    let _ = exchange(&probe_cleanup());
    Ok(samples)
}

/// What a set of samples means for drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conclusion {
    /// Characters the terminal measured differently from the policy.
    pub disagreements: Vec<Sample>,
    /// How tables should be drawn from now on.
    pub draw: DrawMode,
}

/// Decide what to do about a probe's results.
///
/// A single disagreement is enough to switch. The alternative — switching only when "most"
/// characters disagree — would leave a table misaligned in exactly the case the user noticed
/// and ran the probe about.
#[must_use]
pub fn conclude(samples: &[Sample]) -> Conclusion {
    let disagreements: Vec<Sample> = samples.iter().copied().filter(|s| !s.agrees()).collect();
    let draw = if disagreements.is_empty() {
        DrawMode::Padded
    } else {
        DrawMode::CursorReset
    };
    Conclusion {
        disagreements,
        draw,
    }
}

/// Run the probe against the process's real terminal.
///
/// Raw mode is required and is restored before returning: the report arrives as ordinary
/// input, and in cooked mode the line discipline would hold it until the user pressed Enter.
///
/// # Errors
/// [`ProbeError`] when stdout is not a tty, the terminal does not answer within `timeout`, or
/// the answer is not a cursor position report.
pub fn run_on_terminal(
    policy: WidthPolicy,
    timeout: std::time::Duration,
) -> Result<Vec<Sample>, ProbeError> {
    use std::io::Write as _;

    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return Err(ProbeError::NotATty);
    }
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && crossterm::terminal::enable_raw_mode().is_err() {
        return Err(ProbeError::NotATty);
    }

    let result = run(policy, true, |bytes| {
        let mut out = std::io::stdout();
        out.write_all(bytes).ok()?;
        out.flush().ok()?;
        if !bytes.ends_with(b"\x1b[6n") {
            return Some(Vec::new());
        }
        read_report(timeout)
    });

    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    result
}

/// Collect bytes until a cursor position report is complete, or the deadline passes.
fn read_report(timeout: std::time::Duration) -> Option<Vec<u8>> {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    let deadline = std::time::Instant::now() + timeout;
    let mut buf: Vec<u8> = Vec::new();
    while std::time::Instant::now() < deadline {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if !crossterm::event::poll(left).ok()? {
            break;
        }
        // crossterm decodes the report for us when it recognises it; otherwise the bytes
        // arrive as ordinary keys and are reassembled here.
        // A resize, a mouse move or a focus change mid-probe is not the report; waiting
        // through them is the point of the loop.
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = crossterm::event::read().ok()?
        {
            if let KeyCode::Char(c) = code {
                if modifiers.contains(KeyModifiers::CONTROL) && c == '[' {
                    buf.push(0x1b);
                } else {
                    let mut b = [0u8; 4];
                    buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                }
            } else if code == KeyCode::Esc {
                buf.push(0x1b);
            }
        }
        if buf.contains(&b'R') && parse_cursor_report(&buf).is_some() {
            return Some(buf);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use pr_render::AmbiguousWidth;

    /// A terminal that answers cursor reports according to a width rule of its own.
    fn fake_terminal(ambiguous_is_wide: bool) -> impl FnMut(&[u8]) -> Option<Vec<u8>> {
        move |bytes: &[u8]| {
            if !bytes.ends_with(b"\x1b[6n") {
                return Some(Vec::new());
            }
            // The character sits between the clear and the request.
            let body = &bytes[b"\r\x1b[K".len()..bytes.len() - b"\x1b[6n".len()];
            let ch = std::str::from_utf8(body).ok()?.chars().next()?;
            let w = {
                use unicode_width::UnicodeWidthChar as _;
                if ambiguous_is_wide {
                    ch.width_cjk().unwrap_or(0)
                } else {
                    ch.width().unwrap_or(0)
                }
            };
            Some(format!("\x1b[1;{}R", w + 1).into_bytes())
        }
    }

    #[test]
    fn a_probe_sequence_clears_prints_and_asks() {
        let s = probe_sequence('中');
        assert!(s.starts_with(b"\r\x1b[K"), "the line is cleared first");
        assert!(s.ends_with(b"\x1b[6n"), "and the report is requested last");
        assert!(
            s.windows(3).any(|w| w == "中".as_bytes()),
            "the character really is printed"
        );
    }

    #[test]
    fn a_cursor_report_is_parsed() {
        assert_eq!(parse_cursor_report(b"\x1b[12;34R"), Some((12, 34)));
        assert_eq!(parse_cursor_report(b"\x1b[1;3R"), Some((1, 3)));
    }

    #[test]
    fn a_report_arriving_behind_a_keystroke_is_still_found() {
        // The user may type while the probe is in flight; losing the report because something
        // else arrived first would make a measurable terminal look unmeasurable.
        assert_eq!(parse_cursor_report(b"x\x1b[1;3R"), Some((1, 3)));
        assert_eq!(parse_cursor_report(b"\x1b[?1;2c\x1b[1;3R"), Some((1, 3)));
    }

    #[test]
    fn something_that_is_not_a_report_is_refused_rather_than_guessed_at() {
        assert_eq!(parse_cursor_report(b""), None);
        assert_eq!(parse_cursor_report(b"\x1b[R"), None);
        assert_eq!(parse_cursor_report(b"\x1b[abc;defR"), None);
        assert_eq!(parse_cursor_report(b"hello"), None);
    }

    #[test]
    fn probing_a_pipe_is_refused_before_anything_is_written() {
        // Emitting `CSI 6 n` into a pipe writes an escape sequence into the user's data and
        // then waits forever for an answer.
        let mut touched = false;
        let r = run(WidthPolicy::narrow(), false, |_| {
            touched = true;
            Some(b"\x1b[1;2R".to_vec())
        });
        assert_eq!(r.unwrap_err(), ProbeError::NotATty);
        assert!(!touched, "nothing may be written to a non-tty");
    }

    #[test]
    fn a_terminal_that_never_answers_gives_up_rather_than_hanging() {
        let r = run(WidthPolicy::narrow(), true, |_| None);
        assert_eq!(r.unwrap_err(), ProbeError::NoReply);
    }

    #[test]
    fn a_terminal_that_answers_with_nonsense_is_reported_as_such() {
        let r = run(WidthPolicy::narrow(), true, |_| Some(b"maybe?".to_vec()));
        assert_eq!(r.unwrap_err(), ProbeError::BadReply);
    }

    #[test]
    fn a_matching_terminal_produces_no_disagreements() {
        // Narrow policy, terminal that treats ambiguous as narrow.
        let samples = run(WidthPolicy::narrow(), true, fake_terminal(false)).unwrap();
        assert_eq!(samples.len(), representative_chars().len());
        let c = conclude(&samples);
        assert!(c.disagreements.is_empty(), "{:?}", c.disagreements);
        assert_eq!(c.draw, DrawMode::Padded, "no reason to change how we draw");
    }

    #[test]
    fn a_cjk_terminal_under_a_narrow_policy_triggers_the_fallback() {
        // The case §14.6 is about: the policy says one column, the terminal uses two, and
        // every row of a padded table drifts a little further right.
        let samples = run(WidthPolicy::narrow(), true, fake_terminal(true)).unwrap();
        let c = conclude(&samples);
        assert!(
            !c.disagreements.is_empty(),
            "the ambiguous characters must have been caught"
        );
        assert!(
            c.disagreements.iter().any(|s| s.ch == '→'),
            "an arrow is the classic ambiguous case: {:?}",
            c.disagreements
        );
        assert_eq!(c.draw, DrawMode::CursorReset);
        for s in &c.disagreements {
            assert_eq!(s.assumed, 1);
            assert_eq!(s.measured, 2);
        }
    }

    #[test]
    fn a_wide_policy_against_a_narrow_terminal_also_triggers_it() {
        // The disagreement is symmetric: being too generous misaligns just as surely.
        let samples = run(WidthPolicy::wide(), true, fake_terminal(false)).unwrap();
        let c = conclude(&samples);
        assert_eq!(c.draw, DrawMode::CursorReset);
        assert!(c.disagreements.iter().all(|s| s.assumed > s.measured));
    }

    #[test]
    fn one_disagreement_is_enough_to_switch() {
        // Switching only on a majority would leave the table wrong in exactly the case the
        // user noticed and ran the probe about.
        let samples = [
            Sample {
                ch: 'a',
                assumed: 1,
                measured: 1,
            },
            Sample {
                ch: '中',
                assumed: 2,
                measured: 2,
            },
            Sample {
                ch: '→',
                assumed: 1,
                measured: 2,
            },
        ];
        let c = conclude(&samples);
        assert_eq!(c.disagreements.len(), 1);
        assert_eq!(c.draw, DrawMode::CursorReset);
    }

    #[test]
    fn a_wide_character_is_expected_to_be_wide_under_either_policy() {
        // `中` is East Asian Wide, not Ambiguous; a policy that got it wrong would be a bug in
        // the policy rather than a terminal disagreement.
        for p in [WidthPolicy::narrow(), WidthPolicy::wide()] {
            assert_eq!(p.char_width('中'), 2);
        }
        assert_eq!(WidthPolicy::narrow().ambiguous, AmbiguousWidth::Narrow);
        assert_eq!(WidthPolicy::wide().ambiguous, AmbiguousWidth::Wide);
    }

    #[test]
    fn the_representative_set_covers_the_cases_that_actually_differ() {
        let chars = representative_chars();
        let narrow = WidthPolicy::narrow();
        let wide = WidthPolicy::wide();
        let differing: Vec<char> = chars
            .iter()
            .copied()
            .filter(|c| narrow.char_width(*c) != wide.char_width(*c))
            .collect();
        assert!(
            differing.len() >= 4,
            "a probe set whose characters cannot disagree measures nothing; only {differing:?} \
             differ between the two policies"
        );
        // And the set is not *only* ambiguous characters: a wide character and a zero-width
        // one are there so a terminal that is wrong about those is caught too.
        assert!(chars.contains(&'中'));
        assert!(chars.iter().any(|c| narrow.char_width(*c) == 0));
    }
}
