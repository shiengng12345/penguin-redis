//! V-C03 — bracketed paste, and the timing heuristic for terminals without it
//! (v2.1 §14.1, §24.3, R05, ASSIST-082, UX-11).
//!
//! The requirement is one sentence: **nothing pasted runs by itself.** Three lines pasted into
//! a terminal that treats newlines as Enter is three commands the user has not read, and one
//! of them may be `FLUSHALL`.
//!
//! Where bracketed paste works, this is easy and exact. Where it does not — `TERM=dumb`, some
//! Windows consoles, a multiplexer that swallows the mode — §14.1 falls back to timing: a
//! burst with gaps under 10 ms that contains a newline is a paste.
//!
//! A heuristic needs a measured error rate, not an assurance, so this file carries a corpus of
//! timed traces and reports both directions:
//!
//! - **misses** — a paste classified as typing. Zero is required: a miss runs commands.
//! - **false positives** — typing classified as a paste. Costs one keypress, so a rate is
//!   recorded rather than a limit imposed.
//!
//! It also states, as a test rather than as prose, the case timing cannot catch.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_terminal::paste::{BURST_GAP, Choice, INLINE_LIMIT, Keystroke, Staging, classify};
use pr_terminal::{Action, Capture, Coordinator, Input, Key, needs_staging, testing::claim};

/// What a trace really was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Truth {
    Paste,
    Typed,
}

struct Case {
    name: &'static str,
    text: &'static str,
    /// Milliseconds between consecutive bytes.
    step_ms: u64,
    truth: Truth,
}

fn trace(text: &str, step_ms: u64) -> Vec<Keystroke> {
    text.bytes()
        .enumerate()
        .map(|(i, byte)| Keystroke {
            byte,
            at_ms: u64::try_from(i).unwrap_or(0) * step_ms,
        })
        .collect()
}

/// The corpus. Every entry is something a terminal actually does.
const CORPUS: &[Case] = &[
    // ---- real pastes, delivered as a burst -------------------------------------------
    Case {
        name: "three write commands, instant",
        text: "SET a 1\nSET b 2\nDEL c\n",
        step_ms: 0,
        truth: Truth::Paste,
    },
    Case {
        name: "three write commands, 1 ms apart",
        text: "SET a 1\nSET b 2\nDEL c\n",
        step_ms: 1,
        truth: Truth::Paste,
    },
    Case {
        name: "two lines, no trailing newline",
        text: "HGETALL player:1\nHGETALL player:2",
        step_ms: 1,
        truth: Truth::Paste,
    },
    Case {
        name: "CRLF from a Windows editor",
        text: "SET a 1\r\nSET b 2\r\n",
        step_ms: 1,
        truth: Truth::Paste,
    },
    Case {
        name: "ten lines from a script file",
        text: "DEL k1\nDEL k2\nDEL k3\nDEL k4\nDEL k5\nDEL k6\nDEL k7\nDEL k8\nDEL k9\nDEL k10\n",
        step_ms: 2,
        truth: Truth::Paste,
    },
    Case {
        name: "a single very long line",
        text: "SET big aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        step_ms: 1,
        truth: Truth::Paste,
    },
    Case {
        name: "a lone carriage return between lines",
        text: "GET a\rGET b\r",
        step_ms: 1,
        truth: Truth::Paste,
    },
    // ---- a person typing ---------------------------------------------------------------
    Case {
        name: "ordinary typing with an Enter",
        text: "GET mykey\n",
        step_ms: 90,
        truth: Truth::Typed,
    },
    Case {
        name: "two commands typed in succession",
        text: "GET a\nGET b\n",
        step_ms: 120,
        truth: Truth::Typed,
    },
    Case {
        name: "a fast typist",
        text: "HGETALL player:10001\n",
        step_ms: 15,
        truth: Truth::Typed,
    },
    Case {
        name: "a hesitant typist",
        text: "SCAN 0 MATCH x* COUNT 100\n",
        step_ms: 200,
        truth: Truth::Typed,
    },
    Case {
        name: "held key, no newline",
        text: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        step_ms: 3,
        truth: Truth::Typed,
    },
    Case {
        name: "held backspace, no newline",
        text: "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}",
        step_ms: 2,
        truth: Truth::Typed,
    },
    Case {
        name: "typing just under the threshold",
        text: "PING\n",
        step_ms: 11,
        truth: Truth::Typed,
    },
];

fn verdict(case: &Case) -> Truth {
    let segs = classify(&trace(case.text, case.step_ms), BURST_GAP);
    if segs.iter().any(pr_terminal::Segment::is_paste) {
        Truth::Paste
    } else {
        Truth::Typed
    }
}

// ================================================ the measured rates
#[test]
fn assist_082_the_heuristic_misses_no_paste() {
    // A miss runs commands the user did not read, so this one is a limit, not a rate.
    let mut missed = Vec::new();
    for c in CORPUS {
        if c.truth == Truth::Paste && verdict(c) != Truth::Paste {
            missed.push(c.name);
        }
    }
    assert!(
        missed.is_empty(),
        "{} paste(s) were treated as typing: {missed:?}",
        missed.len()
    );
}

#[test]
fn assist_082_the_false_positive_rate_is_measured_and_reported() {
    // A false positive costs one keypress, so it is recorded rather than forbidden. §14.1:
    // "无法判定时宁可多进 staging".
    let typed: Vec<&Case> = CORPUS.iter().filter(|c| c.truth == Truth::Typed).collect();
    let wrong: Vec<&str> = typed
        .iter()
        .filter(|c| verdict(c) == Truth::Paste)
        .map(|c| c.name)
        .collect();
    // Integer arithmetic: a corpus of a dozen traces does not need a float, and a float here
    // would only invite a `cast_precision_loss` allow that says nothing.
    println!(
        "paste heuristic: {} typed trace(s), {} staged unnecessarily ({}%) — {wrong:?}",
        typed.len(),
        wrong.len(),
        wrong.len() * 100 / typed.len()
    );
    // The bar is deliberately loose: erring towards review is the documented preference. What
    // would be unacceptable is *most* typing being staged, which would make the REPL unusable.
    assert!(
        wrong.len() * 2 < typed.len(),
        "the heuristic stages more typing than it lets through: {wrong:?}"
    );
}

#[test]
fn every_corpus_case_is_classified_as_intended() {
    // The per-case assertion, so a regression names the trace rather than a rate.
    for c in CORPUS {
        assert_eq!(
            verdict(c),
            c.truth,
            "{} ({} ms/byte) was classified wrongly",
            c.name,
            c.step_ms
        );
    }
}

#[test]
fn the_case_timing_cannot_catch_is_stated_rather_than_hidden() {
    // A paste relayed byte-by-byte over a slow link is indistinguishable from typing, because
    // it *is* the same signal. Bracketed paste is the answer where it exists; where it does
    // not, this is a real limit and the honest thing is a test that says so rather than a
    // corpus that leaves it out.
    let slow_paste = Case {
        name: "paste relayed at typing speed",
        text: "SET a 1\nSET b 2\n",
        step_ms: 40,
        truth: Truth::Paste,
    };
    assert_eq!(
        verdict(&slow_paste),
        Truth::Typed,
        "if this ever passes, the heuristic improved and this test should record how"
    );
    // The mitigation is not "hope": with bracketed paste on, the same content is exact.
    assert!(needs_staging(slow_paste.text.as_bytes()));
}

// ================================================ bracketed paste is exact
#[test]
fn a_bracketed_paste_needs_no_heuristic() {
    // The terminal told us. §14.1's fallback exists only for terminals that will not.
    for (text, staged) in [
        ("SET a 1\nSET b 2\n", true),
        ("GET onlyoneline", false),
        ("SET a 1\n", true),
        ("", false),
    ] {
        assert_eq!(
            needs_staging(text.as_bytes()),
            staged,
            "bracketed content {text:?}"
        );
    }
    assert!(
        needs_staging(&[b'x'; INLINE_LIMIT + 1]),
        "§24.3: too long to have been read"
    );
}

#[test]
fn every_supported_terminal_gets_the_mode_enabled() {
    // ASSIST-082 begins with us asking for it. A terminal that ignores `CSI ? 2004 h` falls
    // through to the heuristic; one that never receives it has no chance either way.
    let _c = claim();
    let mut co = Coordinator::new(Capture::new(), 80, 24).unwrap();
    co.start();
    let out = String::from_utf8_lossy(&co.sink().bytes).into_owned();
    assert!(
        out.contains("\x1b[?2004h"),
        "bracketed paste was not enabled"
    );
    co.sink_mut().clear();
    co.shutdown();
    let out = String::from_utf8_lossy(&co.sink().bytes).into_owned();
    assert!(
        out.contains("\x1b[?2004l"),
        "and it must be turned off again, or the user's next program inherits it"
    );
}

// ================================================ staging behaviour
#[test]
fn ux_11_a_pasted_block_is_never_executed_line_by_line_on_arrival() {
    let _c = claim();
    let mut co = Coordinator::new(Capture::new(), 80, 24).unwrap();
    co.start();
    let action = co.handle(Input::Paste(
        b"SET a 1\nFLUSHALL\nSET b 2\nSHUTDOWN\n".to_vec(),
    ));
    assert_eq!(action, Action::None, "nothing ran");
    let st = co.staging().expect("under review");
    assert_eq!(
        st.lines()[..4],
        ["SET a 1", "FLUSHALL", "SET b 2", "SHUTDOWN"]
    );
    assert_eq!(st.decision(), None);
}

#[test]
fn each_line_is_reviewable_and_the_choice_decides_what_is_sent() {
    let mut s = Staging::new(b"GET a\nFLUSHALL\nGET b\n");
    assert_eq!(s.lines()[..3], ["GET a", "FLUSHALL", "GET b"]);
    s.focus(1);
    s.delete();
    assert_eq!(s.decide(Choice::OneByOne), ["GET a", "GET b"]);
}

#[test]
fn a_binary_paste_does_not_panic_or_lose_the_line_structure() {
    // Someone pastes a file. It must still be reviewable rather than crash the client.
    let bytes = b"GET a\n\xff\xfe\x00binary\nGET b\n";
    let s = Staging::new(bytes);
    assert_eq!(s.len(), 4, "{:?}", s.lines());
    assert_eq!(s.lines()[0], "GET a");
    assert_eq!(s.lines()[2], "GET b");
    assert!(needs_staging(bytes));
}

#[test]
fn a_paste_arriving_while_a_menu_is_open_closes_the_menu_rather_than_layering_on_it() {
    let _c = claim();
    let mut co = Coordinator::new(Capture::new(), 80, 24).unwrap();
    co.start();
    co.handle(Input::Key(Key::Char('H')));
    co.open_menu(vec!["HGET".into(), "HGETALL".into()]);
    assert!(co.menu().is_some());
    co.handle(Input::Paste(b"GET a\nGET b\n".to_vec()));
    assert!(co.menu().is_none(), "two overlays would fight for the keys");
    assert!(co.staging().is_some());
}
