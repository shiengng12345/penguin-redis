//! V-C02 — the same coordinator, on a real terminal (v2.1 §14.4, ADR-022).
//!
//! `tests/owner.rs` drives the state machine through a capture sink. That proves the logic,
//! but not that it survives raw mode, a real stdout and crossterm's own event decoding —
//! which is where §14.4's failures actually happen.
//!
//! So these run the `pr-terminal-demo` binary on a PTY (V-A01) and assert on what a terminal
//! would really have received.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use portable_pty::CommandBuilder;
use pty_harness::PtySession;
use std::time::Duration;

const T: Duration = Duration::from_secs(10);

/// Control bytes the demo binds its triggers to.
const CTRL_N: &[u8] = b"\x0e"; // open the dropdown
const CTRL_P: &[u8] = b"\x10"; // make a worker post a burst
const CTRL_T: &[u8] = b"\x14"; // enter the alternate screen
const CTRL_D: &[u8] = b"\x04"; // quit on an empty line
const ESC: &[u8] = b"\x1b";

/// Spawn the demo and wait until it has printed its pre-terminal banner.
///
/// Separating this from the first prompt is what tells a failing run whether the process
/// started at all — on Windows that was the whole question.
fn demo_started() -> PtySession {
    let p = demo();
    p.wait_for(b"demo starting", T)
        .expect("the demo process started");
    p
}

fn demo() -> PtySession {
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_pr-terminal-demo"));
    // The runners have no terminfo database, so the program must not depend on one. A fixed
    // TERM keeps the recording identical across platforms.
    cmd.env("TERM", "xterm-256color");
    cmd.env("NO_COLOR", "1");
    PtySession::spawn(cmd, 80, 24).expect("spawn the demo on a PTY")
}

/// Everything received, as text.
fn seen(p: &PtySession) -> String {
    String::from_utf8_lossy(&p.output()).into_owned()
}

/// Everything received with CSI sequences removed — roughly what the user reads.
fn visible(p: &PtySession) -> String {
    let raw = p.output();
    let mut out = String::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == 0x1b {
            let mut j = i + 1;
            if raw.get(j) == Some(&b'[') || raw.get(j) == Some(&b']') {
                j += 1;
                while j < raw.len() && !raw[j].is_ascii_alphabetic() {
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(raw[i] as char);
        i += 1;
    }
    out
}

#[test]
fn a_real_session_edits_a_line_through_the_coordinator() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).expect("the first prompt");
    // Bracketed paste is enabled by the owner, not by the editor (§14.1).
    assert!(
        seen(&p).contains("\x1b[?2004h"),
        "the coordinator turns bracketed paste on"
    );

    p.send(b"HGET player:10001 sta").unwrap();
    p.wait_for(b"HGET player:10001 sta", T)
        .expect("the typed line is echoed by us, not by the tty");

    p.send(CTRL_D).unwrap(); // does nothing on a written line
    p.send(b"\x03").unwrap(); // Ctrl-C clears it
    p.send(CTRL_D).unwrap(); // now it quits
    p.wait_for(b"demo exited cleanly", T)
        .expect("the demo shut down through its own exit path");

    let out = seen(&p);
    assert!(
        out.contains("\x1b[?2004l"),
        "bracketed paste is turned off again on the way out"
    );
    assert!(out.contains("\x1b[?25h"), "the cursor is restored");
}

#[test]
fn the_dropdown_is_drawn_and_erased_in_place() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(b"HG").unwrap();
    p.wait_for(b"HG", T).unwrap();

    p.send(CTRL_N).unwrap();
    p.wait_for(b"HGETDEL", T).expect("the menu appears");
    let with_menu = seen(&p);
    assert!(
        with_menu.contains("\x1b[3A"),
        "the cursor comes back up over the three menu rows: {with_menu:?}"
    );

    // Esc closes it, and the rows are erased rather than scrolled past.
    p.send(ESC).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    p.send(b"X").unwrap();
    p.wait_for(b"HGX", T)
        .expect("editing continues after the menu closed");

    p.send(b"\x03").unwrap();
    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();
}

#[test]
fn assist_062_a_push_burst_does_not_corrupt_the_line_being_typed() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(b"HGETALL play").unwrap();
    p.wait_for(b"HGETALL play", T).unwrap();

    // Twenty notices from a worker thread, while the user keeps typing.
    p.send(CTRL_P).unwrap();
    p.send(b"er:10001").unwrap();
    p.wait_for(b"news #19 from channel a", T)
        .expect("every notice reached the screen");
    p.wait_for(b"HGETALL player:10001", T)
        .expect("the line survived the burst intact");

    let text = visible(&p);
    // No notice was written into a prompt line.
    for line in text.split("\r\n") {
        if let Some(rest) = line.strip_prefix("penguin@r2/0> ") {
            assert!(
                !rest.contains("news #"),
                "a notice landed inside the prompt line: {line:?}"
            );
        }
    }
    // Each notice is on its own line with its origin prefix.
    for i in 0..20 {
        assert!(
            text.contains(&format!("[push] news #{i} from channel a")),
            "notice {i} missing or unprefixed"
        );
    }

    p.send(b"\x03").unwrap();
    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();
}

#[test]
fn ux_12_the_draft_comes_back_from_the_alternate_screen() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(b"HGET player:10001 conf").unwrap();
    p.wait_for(b"HGET player:10001 conf", T).unwrap();

    p.send(CTRL_T).unwrap();
    p.wait_for(b"penguin TUI", T)
        .expect("the TUI takes the screen");
    assert!(
        seen(&p).contains("\x1b[?1049h"),
        "the alternate screen really was entered"
    );
    p.wait_for(b"command: HGET player:10001 conf", T)
        .expect("the TUI opens on the draft");
    p.wait_for(b"@r2 / DB 0 / result #17", T)
        .expect("UX-12: the source label is carried into the TUI");

    // F1 opens help; the command box must not move.
    p.send(b"\x1bOP").unwrap();
    p.wait_for(b"[help]", T).expect("F1 opened the overlay");
    let after_help = visible(&p);
    assert!(
        after_help.contains("command: HGET player:10001 conf"),
        "help disturbed the command box"
    );

    // Esc closes help, Esc again returns to the REPL with the draft.
    p.send(ESC).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    p.send(ESC).unwrap();
    p.wait_for(b"\x1b[?1049l", T)
        .expect("the alternate screen came down");
    p.wait_for(b"penguin@r2/0> HGET player:10001 conf", T)
        .expect("scenario H: the uncommitted draft is back at the REPL");

    // And it was carried, not executed.
    assert!(
        !visible(&p).contains("submitted: HGET player:10001 conf"),
        "returning from the TUI must not run the draft"
    );

    p.send(b"\x03").unwrap();
    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();
}

#[test]
fn notices_that_arrive_during_the_tui_are_printed_after_it_comes_down() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(CTRL_T).unwrap();
    p.wait_for(b"penguin TUI", T).unwrap();

    let before = seen(&p);
    assert!(
        !before.contains("[push]"),
        "nothing should have been printed yet"
    );
    p.send(CTRL_P).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !seen(&p).contains("[push] news #0"),
        "a notice must not be painted into a TUI frame"
    );

    p.send(ESC).unwrap();
    p.wait_for(b"[push] news #19 from channel a", T)
        .expect("the queue is flushed once the REPL has the screen back");

    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();
}

#[test]
fn a_resize_is_handled_by_the_same_owner() {
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(b"GET some:key").unwrap();
    p.wait_for(b"GET some:key", T).unwrap();

    p.resize(40, 12).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    p.send(b"!").unwrap();
    p.wait_for(b"GET some:key!", T)
        .expect("the line survived the resize");
    assert_eq!(p.size(), (40, 12));

    p.send(b"\x03").unwrap();
    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();
}

#[test]
fn the_recording_is_replayable_and_names_the_platform() {
    // V-A01 gave us a deterministic recording format; V-C02 is the first real user of it, so
    // the golden really does describe a session rather than a synthetic script.
    let mut p = demo_started();
    p.wait_for(b"penguin@r2/0>", T).unwrap();
    p.send(b"PING").unwrap();
    p.wait_for(b"PING", T).unwrap();
    p.send(b"\r").unwrap();
    p.wait_for(b"submitted: PING", T)
        .expect("the submitted line reached the application");
    p.send(CTRL_D).unwrap();
    p.wait_for(b"demo exited cleanly", T).unwrap();

    let rec = p.recording();
    let golden = rec.to_golden().expect("golden");
    assert!(golden.contains("\"in\""), "inputs are recorded");
    assert!(golden.contains("\"out\""), "outputs are recorded");
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
    assert!(
        !golden.contains(std::env::consts::OS),
        "but the golden itself stays platform-neutral"
    );
    // And the recorded output really is the session, not a synthetic script.
    let replayed = String::from_utf8_lossy(&rec.output_bytes()).into_owned();
    assert!(replayed.contains("submitted: PING"));
    assert!(replayed.contains("demo exited cleanly"));
}
