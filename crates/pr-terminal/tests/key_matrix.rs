//! V-C05 — key reachability (v2.1 §14.2, ASSIST-068).
//!
//! §14.2's pass criterion is not "every key works". It cannot be: macOS binds F1–F6 to system
//! functions unless the user changes a setting, an input method takes `Ctrl+Space` on all
//! three platforms, and a multiplexer takes whatever it likes. The criterion is the other
//! half of that section — **every function has a text entry** — and "不能要求用户先修改系统
//! 快捷键才能找帮助".
//!
//! So there are two kinds of test here, and they answer different questions:
//!
//! - what a terminal actually *delivers* for each sequence, recorded through a real PTY. A
//!   key that does not arrive is information, not a failure.
//! - that every function is reachable by typing regardless, which **is** a failure if it
//!   breaks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use portable_pty::CommandBuilder;
use pr_repl::{ENTRIES, KeyMap, text_entries};
use pty_harness::PtySession;
use std::time::Duration;

const T: Duration = Duration::from_secs(10);

/// The sequences a terminal sends for the keys §14.2 names.
///
/// Both spellings are listed where terminals disagree: `SS3` (`ESC O P`) and the CSI form
/// (`ESC [ 1 1 ~`) are both F1 depending on the terminal and its mode.
const SEQUENCES: &[(&str, &[u8], &str)] = &[
    ("F1 (SS3)", b"\x1bOP", "Function(1)"),
    ("F1 (CSI)", b"\x1b[11~", "Function(1)"),
    ("F2 (SS3)", b"\x1bOQ", "Function(2)"),
    ("F2 (CSI)", b"\x1b[12~", "Function(2)"),
    ("F3 (SS3)", b"\x1bOR", "Function(3)"),
    ("F4 (SS3)", b"\x1bOS", "Function(4)"),
    ("F5", b"\x1b[15~", "Function(5)"),
    ("F6", b"\x1b[17~", "Function(6)"),
    ("Ctrl+R", b"\x12", "Ctrl('r')"),
    ("Ctrl+P", b"\x10", "Ctrl('p')"),
    ("Ctrl+N", b"\x0e", "Ctrl('n')"),
    // `Ctrl+Space` is NUL on a Unix terminal, and many terminals and input methods swallow it
    // entirely. §14.2 says it is never the only way to anything, so a miss here is recorded
    // rather than failed.
    ("Ctrl+Space", b"\x00", "Char(' ')"),
];

fn probe() -> PtySession {
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_pr-terminal-demo"));
    cmd.env("PR_KEYPROBE", "1");
    cmd.env("TERM", "xterm-256color");
    let p = PtySession::spawn(cmd, 80, 24).expect("spawn the key probe");
    p.wait_for_screen("keyprobe ready", T)
        .expect("the probe started");
    p
}

#[test]
fn what_this_terminal_delivers_is_recorded() {
    let mut p = probe();
    let mut delivered = Vec::new();
    let mut missing = Vec::new();

    for (name, bytes, expect) in SEQUENCES {
        p.send(bytes).unwrap();
        // A key that is not delivered produces nothing, so the wait has to be short or the
        // test takes a minute to learn nothing.
        if p.wait_for_screen(&format!("key={expect}"), Duration::from_millis(600))
            .is_ok()
        {
            delivered.push(*name);
        } else {
            missing.push(*name);
        }
    }

    println!(
        "V-C05 on {}: delivered {:?}; not delivered {:?}",
        std::env::consts::OS,
        delivered,
        missing
    );

    // The function keys and Ctrl chords that §14.2 depends on should arrive through a plain
    // PTY — this is the baseline against which a real terminal's interception is measured.
    for required in ["Ctrl+R", "Ctrl+P", "Ctrl+N"] {
        assert!(
            delivered.contains(&required),
            "{required} did not arrive even through a bare PTY, so the binding is not \
             implementable at all: delivered {delivered:?}"
        );
    }
    assert!(
        delivered.iter().any(|n| n.starts_with("F1")),
        "no spelling of F1 arrived: {delivered:?}"
    );

    p.send(b"\x04").unwrap();
    p.wait_for_screen("keyprobe done", T).unwrap();
}

#[test]
fn a_key_that_is_swallowed_leaves_the_function_reachable() {
    // ASSIST-068, and the actual pass criterion. Simulate every contended key being taken by
    // the system: the text entry is what is left, and it must cover everything.
    let swallowed: Vec<&str> = ENTRIES
        .iter()
        .filter(|e| e.key_is_contended())
        .map(|e| e.function)
        .collect();
    assert!(swallowed.len() >= 6, "{swallowed:?}");

    let map = KeyMap::new();
    for function in swallowed {
        if function.starts_with("cancel") {
            // `Ctrl+C` is the one key every terminal delivers, and by the time it is needed
            // the line is not the user's to type into.
            continue;
        }
        assert!(
            map.text_for(function).is_some(),
            "{function} would be unreachable on a system that takes its key"
        );
    }
}

#[test]
fn the_text_entries_are_typeable_under_any_input_method() {
    // An input method intercepts chords, not letters. So a text entry made of ordinary ASCII
    // is reachable wherever a user can type a Redis command at all — which they must be able
    // to do, or the client is useless regardless.
    for (function, text) in text_entries() {
        assert!(
            text.is_ascii(),
            "{function}: {text:?} is not plain ASCII, so an input method could interfere"
        );
        assert!(
            !text.contains('\u{1b}') && !text.chars().any(|c| (c as u32) < 0x20),
            "{function}: {text:?} contains a control character"
        );
    }
}

#[test]
fn the_probe_decodes_ordinary_typing_too() {
    // A probe that reports nothing for everything would pass the "not delivered" branch above
    // without proving anything.
    let mut p = probe();
    p.send(b"a").unwrap();
    p.wait_for_screen("key=Char('a')", T)
        .expect("the probe is actually decoding");
    p.send(b"\x1b[A").unwrap();
    p.wait_for_screen("key=Up", T).expect("arrow keys arrive");
    p.send(b"\x7f").unwrap();
    p.wait_for_screen("key=Backspace", T).unwrap();
    p.send(b"\x04").unwrap();
    p.wait_for_screen("keyprobe done", T).unwrap();
}
