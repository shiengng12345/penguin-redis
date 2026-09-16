//! V-C02 — the same coordinator, on a real terminal (v2.1 §14.4, §15.8, ADR-022).
//!
//! `tests/owner.rs` drives the state machine through a capture sink and asserts on the exact
//! bytes. That is the right place for a byte contract: the sink is ours, so the assertion
//! means the same thing everywhere.
//!
//! These tests ask a different question — does it still work on a *real* terminal, through
//! raw mode and crossterm's own event decoding? So they assert on the **rendered screen**
//! rather than on the stream. That is not a convenience: `ConPTY` does not forward what the
//! child wrote, it renders into its own buffer and emits a diff of that. A program that
//! writes `\rpenguin> HGX` can produce a Windows stream in which those letters never appear
//! together, while the screen plainly shows them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use portable_pty::CommandBuilder;
use pty_harness::PtySession;
use std::time::Duration;

const T: Duration = Duration::from_secs(10);

/// Control bytes the demo binds its triggers to.
const CTRL_C: &[u8] = b"\x03"; // clear the line
const CTRL_N: &[u8] = b"\x0e"; // open the dropdown
const CTRL_P: &[u8] = b"\x10"; // make a worker post a burst
const CTRL_T: &[u8] = b"\x14"; // enter the alternate screen
const CTRL_D: &[u8] = b"\x04"; // quit on an empty line
const ESC: &[u8] = b"\x1b";

const PROMPT: &str = "penguin@r2/0>";

fn demo() -> PtySession {
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_pr-terminal-demo"));
    // The runners have no terminfo database, so the program must not depend on one.
    cmd.env("TERM", "xterm-256color");
    cmd.env("NO_COLOR", "1");
    PtySession::spawn(cmd, 80, 24).expect("spawn the demo on a PTY")
}

/// Spawn the demo and wait until it has printed its pre-terminal banner.
///
/// Separating this from the first prompt is what tells a failing run whether the process
/// started at all — on Windows that was the whole question, and the answer was that `ConPTY`
/// was blocked waiting for a cursor-position report nobody had answered.
fn demo_started() -> PtySession {
    let p = demo();
    p.wait_for_screen("demo starting", T)
        .expect("the demo process started");
    p
}

/// Quit cleanly, so every test also covers the shutdown path.
fn quit(p: &mut PtySession) {
    p.send(CTRL_C).unwrap();
    p.send(CTRL_D).unwrap();
    p.wait_for_screen("demo exited cleanly", T)
        .expect("the demo shut down through its own exit path");
}

#[test]
fn a_real_session_edits_a_line_through_the_coordinator() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).expect("the first prompt");

    p.send(b"HGET player:10001 sta").unwrap();
    p.wait_for_screen("HGET player:10001 sta", T)
        .expect("the typed line is on screen, painted by us rather than echoed by the tty");

    // Exactly once: a key release must not repeat the keystroke, and Windows sends both.
    let screen = p.screen();
    let line = screen
        .lines()
        .into_iter()
        .find(|l| l.contains("HGET"))
        .unwrap();
    assert_eq!(
        line.matches("HGET").count(),
        1,
        "the command appears twice — a key release was acted on: {line:?}"
    );
    assert!(!line.contains("HHGG"), "characters doubled: {line:?}");

    quit(&mut p);
}

#[test]
fn the_dropdown_is_drawn_and_erased_in_place() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(b"HG").unwrap();
    p.wait_for_screen("HG", T).unwrap();

    p.send(CTRL_N).unwrap();
    p.wait_for_screen("HGETDEL", T).expect("the menu appears");

    let s = p.screen();
    let prompt_row = s
        .lines()
        .iter()
        .position(|l| l.contains(PROMPT))
        .expect("the prompt is on screen");
    let menu_row = s
        .lines()
        .iter()
        .position(|l| l.contains("HGETALL"))
        .expect("the menu is on screen");
    assert!(
        menu_row > prompt_row,
        "the dropdown must be drawn below the prompt, not over it"
    );

    // Esc closes it; the rows are erased in place rather than scrolled past.
    p.send(ESC).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    p.send(b"X").unwrap();
    p.wait_for_screen("HGX", T)
        .expect("editing continues after the menu closed");

    let s = p.screen();
    assert!(
        !s.shows("HGETALL"),
        "a closed menu must not still be on screen: {:?}",
        s.text()
    );
    assert!(
        s.scrollback().iter().all(|l| !l.contains("HGETALL")),
        "and it must not have been pushed into history either"
    );

    quit(&mut p);
}

#[test]
fn assist_062_a_push_burst_does_not_corrupt_the_line_being_typed() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(b"HGETALL play").unwrap();
    p.wait_for_screen("HGETALL play", T).unwrap();

    // Twenty notices from a worker thread, while the user keeps typing.
    p.send(CTRL_P).unwrap();
    p.send(b"er:10001").unwrap();
    p.wait_for_screen("news #19 from channel a", T)
        .expect("every notice reached the screen");
    p.wait_for_screen("HGETALL player:10001", T)
        .expect("the line survived the burst intact");

    let s = p.screen();
    // Each notice is its own line, carrying its origin prefix.
    for i in 0..20 {
        assert!(
            s.seen(&format!("[push] news #{i} from channel a")),
            "notice {i} missing or unprefixed"
        );
    }
    // And no notice was written into a prompt line.
    for line in s.history().lines() {
        if let Some((_, rest)) = line.split_once(PROMPT) {
            assert!(
                !rest.contains("news #"),
                "a notice landed inside the prompt line: {line:?}"
            );
        }
    }

    quit(&mut p);
}

#[test]
fn ux_12_the_draft_comes_back_from_the_alternate_screen() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(b"HGET player:10001 conf").unwrap();
    p.wait_for_screen("HGET player:10001 conf", T).unwrap();

    p.send(CTRL_T).unwrap();
    p.wait_for_screen("penguin TUI", T)
        .expect("the TUI takes the screen");
    assert!(
        p.screen().in_alternate(),
        "the alternate screen really was entered"
    );
    p.wait_for_screen("command: HGET player:10001 conf", T)
        .expect("the TUI opens on the draft");
    p.wait_for_screen("@r2 / DB 0 / result #17", T)
        .expect("UX-12: the source label is carried into the TUI");

    // F1 opens help; §15.8 requires the command box to be untouched.
    p.send(b"\x1bOP").unwrap();
    p.wait_for_screen("[help]", T)
        .expect("F1 opened the overlay");
    assert!(
        p.screen().shows("command: HGET player:10001 conf"),
        "help disturbed the command box: {}",
        p.screen().text()
    );

    // Esc closes help, Esc again returns to the REPL with the draft.
    p.send(ESC).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    p.send(ESC).unwrap();
    p.wait_for_screen(&format!("{PROMPT} HGET player:10001 conf"), T)
        .expect("scenario H: the uncommitted draft is back at the REPL");

    let s = p.screen();
    assert!(
        !s.in_alternate(),
        "the alternate screen must have come down"
    );
    assert!(
        !s.seen("submitted: HGET player:10001 conf"),
        "returning from the TUI must not run the draft"
    );

    quit(&mut p);
}

#[test]
fn notices_that_arrive_during_the_tui_are_printed_after_it_comes_down() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(CTRL_T).unwrap();
    p.wait_for_screen("penguin TUI", T).unwrap();

    p.send(CTRL_P).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !p.screen().shows("[push] news #0"),
        "a notice must not be painted into a TUI frame: {}",
        p.screen().text()
    );

    p.send(ESC).unwrap();
    p.wait_for_screen("[push] news #19 from channel a", T)
        .expect("the queue is flushed once the REPL has the screen back");

    p.send(CTRL_D).unwrap();
    p.wait_for_screen("demo exited cleanly", T).unwrap();
}

#[test]
fn a_resize_is_handled_by_the_same_owner() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(b"GET some:key").unwrap();
    p.wait_for_screen("GET some:key", T).unwrap();

    p.resize(40, 12).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    p.send(b"!").unwrap();
    p.wait_for_screen("GET some:key!", T)
        .expect("the line survived the resize");
    assert_eq!(p.size(), (40, 12));

    quit(&mut p);
}

#[test]
fn the_harness_answers_the_terminal_queries_the_child_makes() {
    // On Windows ConPTY asks for the cursor position as it starts and blocks until answered.
    // A zero here alongside a working session would mean the responder had become dead code
    // on this platform; a zero alongside a stalled one is the failure it exists to prevent.
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    if cfg!(windows) {
        assert!(
            p.auto_replies() > 0,
            "ConPTY asks for the cursor position and it must have been answered"
        );
    }
    quit(&mut p);
}

#[test]
fn the_recording_is_replayable_and_the_screen_model_understands_it() {
    let mut p = demo_started();
    p.wait_for_screen(PROMPT, T).unwrap();
    p.send(b"PING").unwrap();
    p.wait_for_screen("PING", T).unwrap();
    p.send(b"\r").unwrap();
    p.wait_for_screen("submitted: PING", T)
        .expect("the submitted line reached the application");
    p.send(CTRL_D).unwrap();
    p.wait_for_screen("demo exited cleanly", T).unwrap();

    let rec = p.recording();
    let golden = rec.to_golden().expect("golden");
    assert!(golden.contains(r#""in""#), "inputs are recorded");
    assert!(golden.contains(r#""out""#), "outputs are recorded");
    assert!(
        !golden.contains("at_ms"),
        "timestamps are dropped so the golden is deterministic"
    );
    // The platform lives on the Recording, not in the golden: a golden that named its OS
    // could never be shared between platforms, which defeats the point of having one.
    assert!(
        rec.platform.contains(std::env::consts::OS),
        "the recording says where it was made"
    );
    assert!(!golden.contains(std::env::consts::OS));

    // Replaying the recorded output through the screen model reproduces the session, and the
    // model understood essentially all of it.
    let mut screen = pty_harness::Screen::new(80, 24);
    screen.feed(&rec.output_bytes());
    assert!(screen.seen("submitted: PING"), "{}", screen.history());
    assert!(screen.seen("demo exited cleanly"));
    assert!(
        screen.unhandled() < 10,
        "the screen model skipped {} sequences; an assertion about the screen would then be \
         about a screen the user does not have",
        screen.unhandled()
    );
}
