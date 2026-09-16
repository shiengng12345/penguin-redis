//! V-C02 — the single terminal owner (v2.1 §14.4, §15.8, ADR-022).
//!
//! The acceptance criteria are ASSIST-062, UX-12 and scenario H, plus "no stdin/stdout
//! contention". The last one is not a behaviour that can be observed after the fact — by the
//! time two writers have interleaved, the evidence is a corrupted screen. So it is checked
//! structurally: the terminal is a token, and a second claimant is refused.
//!
//! The rest is checked on the exact bytes painted, because the failures §14.4 is about are
//! byte-level: a notice landing inside the user's line, a menu row left on screen after the
//! menu closed, an alternate screen that never came down.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_terminal::{
    Action, Capture, Coordinator, Input, Key, Mailbox, Notice, NoticeOrigin, OwnershipError,
    Scripted, Source, Surface, testing::claim,
};
use std::time::Duration;

fn coordinator() -> Coordinator<Capture> {
    let mut co = Coordinator::new(Capture::new(), 80, 24).expect("claim the terminal");
    co.set_prompt("penguin> ");
    co.set_origin("@r2 / DB 0 / result #17");
    co
}

fn type_str(co: &mut Coordinator<Capture>, s: &str) {
    for c in s.chars() {
        co.handle(Input::Key(Key::Char(c)));
    }
}

/// What is left on the screen, with our own cursor movements removed — an approximation of
/// what the user sees, good enough to assert that a notice is not inside the prompt line.
fn visible(cap: &Capture) -> String {
    let mut out = String::new();
    let s = cap.text();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b {
            // Skip a CSI sequence entirely.
            let mut j = i + 1;
            if b.get(j) == Some(&b'[') {
                j += 1;
                while j < b.len() && !b[j].is_ascii_alphabetic() {
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

// ================================================ no second reader, no second writer
#[test]
fn a_second_coordinator_cannot_be_built() {
    let _c = claim();
    let first = coordinator();
    // This is the enforcement of §14.4. A Tokio worker, a TUI painter or a line editor that
    // tried to take the terminal for itself gets this error rather than a race.
    let second = Coordinator::new(Capture::new(), 80, 24);
    assert!(matches!(second, Err(OwnershipError::AlreadyOwned)));
    drop(first);
    // And the terminal is reclaimable, so a TUI really can hand it back.
    assert!(Coordinator::new(Capture::new(), 80, 24).is_ok());
}

#[test]
fn a_worker_can_only_post_never_paint() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    let mailbox = Mailbox::new();

    // Ten threads all "sending to the terminal" at once. None of them can: the only API they
    // have is `post`, and the coordinator drains it on its own thread.
    let handles: Vec<_> = (0..10)
        .map(|t| {
            let m = mailbox.clone();
            std::thread::spawn(move || {
                for i in 0..10 {
                    m.notify(NoticeOrigin::Push, format!("t{t} #{i}"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker");
    }
    assert_eq!(mailbox.len(), 100, "every message queued, none painted");

    let before = co.frames();
    while let Some(input) = mailbox.take() {
        co.handle(input);
    }
    assert_eq!(co.printed(), 100, "all 100 reached the screen, in order");
    assert!(co.frames() > before);
}

// ================================================ ASSIST-062: flood while typing
#[test]
fn assist_062_a_push_flood_never_lands_inside_the_typed_line() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HGET player:10001 sta");
    assert_eq!(co.text(), "HGET player:10001 sta");
    let cursor_before = co.cursor();

    co.sink_mut().clear();
    for i in 0..25 {
        co.handle(Input::Message(Notice {
            text: format!("channel a: message {i}"),
            origin: NoticeOrigin::Push,
        }));
    }

    // The buffer and cursor are untouched: a notice is printed *around* the line, never into
    // it.
    assert_eq!(co.text(), "HGET player:10001 sta");
    assert_eq!(co.cursor(), cursor_before);

    let seen = visible(co.sink());
    // Every message reached scrollback, each on its own line.
    for i in 0..25 {
        assert!(
            seen.contains(&format!("channel a: message {i}")),
            "message {i} was lost"
        );
    }
    // And no message was written into the middle of the prompt line: every occurrence of the
    // prompt is followed by the user's text, never by a notice.
    for line in seen.split("\r\n") {
        if let Some(rest) = line.strip_prefix("penguin> ") {
            assert!(
                !rest.contains("channel a:"),
                "a notice landed inside the prompt line: {line:?}"
            );
        }
    }
    assert_eq!(co.printed(), 25);
}

#[test]
fn assist_062_typing_still_works_during_a_flood() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    // Interleave a keystroke with a burst, the way the event loop actually would.
    for (i, ch) in "HGETALL k".chars().enumerate() {
        co.handle(Input::Key(Key::Char(ch)));
        for j in 0..3 {
            co.handle(Input::Message(Notice {
                text: format!("burst {i}.{j}"),
                origin: NoticeOrigin::Push,
            }));
        }
    }
    assert_eq!(co.text(), "HGETALL k", "not one keystroke was lost");
    assert_eq!(co.printed(), 27);
}

#[test]
fn assist_062_the_menu_survives_a_notice() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HG");
    co.open_menu(vec!["HGET".into(), "HGETALL".into(), "HGETDEL".into()]);
    co.handle(Input::Key(Key::Down));
    co.handle(Input::Key(Key::Down));
    assert_eq!(co.menu().unwrap().focused, Some(1));

    co.sink_mut().clear();
    co.handle(Input::Message(Notice {
        text: "a push arrived".into(),
        origin: NoticeOrigin::Push,
    }));

    // The selection is exactly where it was — the user may be about to press Enter.
    assert_eq!(co.menu().unwrap().focused, Some(1));
    let seen = visible(co.sink());
    assert!(seen.contains("a push arrived"));
    assert!(seen.contains("> HGETALL"), "the menu was redrawn: {seen:?}");
    assert!(seen.contains("  HGET"), "and the unfocused rows too");
}

// ================================================ the self-drawn dropdown
#[test]
fn the_menu_is_drawn_below_the_prompt_and_erased_when_it_closes() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HG");

    co.sink_mut().clear();
    co.open_menu(vec!["HGET".into(), "HGETALL".into()]);
    let opened = co.sink().text();
    assert!(
        opened.contains("\x1b[J"),
        "the block is erased before redrawing, not appended to"
    );
    assert!(opened.contains("HGETALL"));
    assert!(
        opened.contains("\x1b[2A"),
        "the cursor returns to the prompt row above the two menu rows: {opened:?}"
    );

    co.sink_mut().clear();
    co.close_menu();
    let closed = visible(co.sink());
    assert!(
        !closed.contains("HGETALL"),
        "a closed menu must not still be on screen: {closed:?}"
    );
    assert!(
        co.sink().text().contains("\x1b[J"),
        "closing erases the rows rather than scrolling past them"
    );
    assert!(closed.contains("penguin> HG"), "the prompt is intact");
}

#[test]
fn accepting_a_candidate_edits_the_line_and_does_not_submit() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HGET player:10001 not");
    co.open_menu(vec![
        "notificationConfig".into(),
        "notificationEnabled".into(),
    ]);
    co.handle(Input::Key(Key::Down));

    let action = co.handle(Input::Key(Key::Enter));
    assert_eq!(
        action,
        Action::None,
        "accepting a candidate must never execute (ASSIST-020)"
    );
    assert_eq!(co.text(), "HGET player:10001 notificationConfig");
    assert!(co.menu().is_none(), "the menu closes on acceptance");

    // And *then* Enter submits.
    assert_eq!(
        co.handle(Input::Key(Key::Enter)),
        Action::Submit("HGET player:10001 notificationConfig".into())
    );
    assert_eq!(co.text(), "", "the line is cleared after submission");
}

#[test]
fn typing_invalidates_a_menu_computed_for_older_text() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HG");
    co.open_menu(vec!["HGET".into()]);
    assert!(co.menu().is_some());
    co.handle(Input::Key(Key::Char('E')));
    assert!(
        co.menu().is_none(),
        "a candidate list for `HG` must not stay open over `HGE` (§12.8)"
    );
}

// ================================================ UX-12 / scenario H
#[test]
fn scenario_h_the_draft_survives_a_tui_round_trip() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "HGET player:10001 conf");
    let text_before = co.text().to_owned();
    let cursor_before = co.cursor();
    let origin_before = co.origin().to_owned();

    co.enter_alternate();
    assert_eq!(co.surface(), Surface::Alternate);
    assert_eq!(
        co.tui_box(),
        text_before,
        "the TUI command box opens on the draft the user was writing"
    );

    // F1 opens help; §15.8 requires the box and its cursor to be untouched.
    co.handle(Input::Key(Key::Function(1)));
    assert!(co.tui_help_open());
    assert_eq!(co.tui_box(), text_before, "help disturbed the command box");
    co.handle(Input::Key(Key::Function(1)));
    assert!(!co.tui_help_open());
    assert_eq!(co.tui_box(), text_before);

    // Esc with help closed returns to the REPL, carrying the uncommitted draft.
    co.handle(Input::Key(Key::Esc));
    assert_eq!(co.surface(), Surface::Repl);
    assert_eq!(co.text(), text_before, "UX-12: the original input is back");
    assert_eq!(co.cursor(), cursor_before, "and so is the cursor");
    assert_eq!(co.origin(), origin_before, "and the source label");
}

#[test]
fn scenario_h_returning_from_the_tui_does_not_execute_the_draft() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "FLUSHALL");
    co.enter_alternate();
    // Leaving must produce no Submit — §15.8 is explicit that the draft is carried, not run.
    let action = co.handle(Input::Key(Key::Esc));
    assert_eq!(action, Action::None);
    assert_eq!(co.text(), "FLUSHALL", "still only a draft");
    assert_eq!(co.surface(), Surface::Repl);
}

#[test]
fn escape_inside_the_tui_closes_help_before_it_closes_the_tui() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.enter_alternate();
    co.handle(Input::Key(Key::Function(1)));
    assert!(co.tui_help_open());
    co.handle(Input::Key(Key::Esc));
    assert_eq!(
        co.surface(),
        Surface::Alternate,
        "the first Esc closed the overlay, not the whole TUI"
    );
    assert!(!co.tui_help_open());
    co.handle(Input::Key(Key::Esc));
    assert_eq!(co.surface(), Surface::Repl);
}

#[test]
fn the_alternate_screen_is_entered_and_left_exactly_once() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.sink_mut().clear();
    co.enter_alternate();
    co.enter_alternate(); // idempotent — a second request must not nest
    let up = co.sink().text();
    assert_eq!(
        up.matches("\x1b[?1049h").count(),
        1,
        "entering twice would need leaving twice to restore scrollback"
    );

    co.sink_mut().clear();
    co.leave_alternate();
    co.leave_alternate();
    let down = co.sink().text();
    assert_eq!(down.matches("\x1b[?1049l").count(), 1);
    assert!(
        down.contains("\x1b[?25h"),
        "the cursor is shown again on the way out"
    );
}

#[test]
fn notices_wait_while_the_tui_holds_the_screen_and_arrive_in_order_afterwards() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.enter_alternate();

    for i in 0..5 {
        co.handle(Input::Message(Notice {
            text: format!("queued {i}"),
            origin: NoticeOrigin::Push,
        }));
    }
    assert_eq!(
        co.pending(),
        5,
        "printing into a TUI frame would corrupt it"
    );
    assert_eq!(co.printed(), 0);

    co.sink_mut().clear();
    co.leave_alternate();
    assert_eq!(co.pending(), 0);
    assert_eq!(co.printed(), 5);

    let seen = visible(co.sink());
    let mut at = 0usize;
    for i in 0..5 {
        let needle = format!("queued {i}");
        let found = seen[at..]
            .find(&needle)
            .unwrap_or_else(|| panic!("`{needle}` missing or out of order in {seen:?}"));
        at += found + needle.len();
    }
}

#[test]
fn shutdown_takes_the_alternate_screen_down() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.enter_alternate();
    co.sink_mut().clear();
    co.shutdown();
    let out = co.sink().text();
    assert!(
        out.contains("\x1b[?1049l"),
        "exiting from the TUI must restore the main screen: {out:?}"
    );
    assert!(out.contains("\x1b[?2004l"), "bracketed paste is turned off");
    assert!(out.contains("\x1b[?25h"), "the cursor is visible again");
}

// ================================================ the rest of the surface
#[test]
fn a_resize_redraws_from_a_known_position() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "GET k");
    co.sink_mut().clear();
    co.handle(Input::Resize { cols: 40, rows: 12 });
    assert_eq!(co.size(), (40, 12));
    assert_eq!(co.text(), "GET k", "a resize must not disturb the line");
    let out = co.sink().text();
    assert!(out.contains("\x1b[J"), "the block is redrawn, not patched");
    assert!(visible(co.sink()).contains("penguin> GET k"));
}

#[test]
fn a_multiline_paste_goes_to_review_and_runs_nothing() {
    // §14.1 / ASSIST-082 / UX-11. Three pasted lines in a terminal that treats newlines as
    // Enter is three commands the user has not read.
    let _c = claim();
    let mut co = coordinator();
    co.start();
    let action = co.handle(Input::Paste(b"SET a 1\nFLUSHALL\nSET b 2\n".to_vec()));
    assert_eq!(action, Action::None, "nothing may run on arrival");
    let st = co.staging().expect("the paste is under review");
    assert_eq!(st.lines()[..3], ["SET a 1", "FLUSHALL", "SET b 2"]);
    assert_eq!(st.decision(), None);
    assert_eq!(co.text(), "", "and nothing leaked into the edit buffer");
}

#[test]
fn a_single_line_paste_goes_into_the_buffer_without_a_modal() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    assert_eq!(co.handle(Input::Paste(b"GET mykey".to_vec())), Action::None);
    assert!(co.staging().is_none(), "one line does not need review");
    assert_eq!(co.text(), "GET mykey", "it is editable, and unsubmitted");
}

#[test]
fn enter_does_nothing_in_the_review_view() {
    // The key a user presses by reflex is the one that must not run a pasted block.
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.handle(Input::Paste(b"FLUSHALL\nFLUSHDB\n".to_vec()));
    for k in [Key::Enter, Key::Tab, Key::Char('x'), Key::Right] {
        assert_eq!(
            co.handle(Input::Key(k)),
            Action::None,
            "{k:?} ran something"
        );
        assert!(co.staging().is_some(), "{k:?} dismissed the review");
    }
}

#[test]
fn the_three_choices_are_the_only_ways_out() {
    let _c = claim();

    // [1] one by one.
    let mut co = coordinator();
    co.start();
    co.handle(Input::Paste(b"GET a\nGET b\n".to_vec()));
    assert_eq!(
        co.handle(Input::Key(Key::Char('1'))),
        Action::SubmitMany(vec!["GET a".into(), "GET b".into()])
    );
    assert!(co.staging().is_none());
    drop(co);

    // [2] as a single command.
    let mut co = coordinator();
    co.start();
    co.handle(Input::Paste(b"EVAL \"return 1\"\n0\n".to_vec()));
    assert_eq!(
        co.handle(Input::Key(Key::Char('2'))),
        Action::Submit("EVAL \"return 1\"\n0".into())
    );
    drop(co);

    // [Esc] cancel.
    let mut co = coordinator();
    co.start();
    co.handle(Input::Paste(b"FLUSHALL\nFLUSHDB\n".to_vec()));
    assert_eq!(co.handle(Input::Key(Key::Esc)), Action::None);
    assert!(co.staging().is_none());
    assert_eq!(co.text(), "", "cancelling leaves nothing behind");
}

#[test]
fn a_dangerous_line_can_be_removed_before_anything_runs() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.handle(Input::Paste(b"GET a\nFLUSHALL\nGET b\n".to_vec()));
    co.handle(Input::Key(Key::Down)); // focus FLUSHALL
    assert_eq!(co.staging().unwrap().focused(), 1);
    co.handle(Input::Key(Key::Backspace));
    assert_eq!(
        co.handle(Input::Key(Key::Char('1'))),
        Action::SubmitMany(vec!["GET a".into(), "GET b".into()])
    );
}

#[test]
fn the_review_view_says_plainly_that_nothing_has_run() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    co.sink_mut().clear();
    co.handle(Input::Paste(b"SET a 1\nSET b 2\n".to_vec()));
    let seen = visible(co.sink());
    assert!(seen.contains("nothing has run"), "{seen:?}");
    assert!(seen.contains("pasted 2 line(s)"), "{seen:?}");
    assert!(seen.contains("[1] run one by one"), "{seen:?}");
    assert!(seen.contains("[2] run as one command"), "{seen:?}");
    assert!(seen.contains("[Esc] cancel"), "{seen:?}");
    assert!(seen.contains("SET a 1") && seen.contains("SET b 2"));
}

#[test]
fn ctrl_c_clears_the_line_and_ctrl_d_quits_only_when_it_is_empty() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "FLUSHALL");
    assert_eq!(co.handle(Input::Key(Key::Ctrl('d'))), Action::None);
    assert_eq!(
        co.text(),
        "FLUSHALL",
        "Ctrl-D on a written line does nothing"
    );
    assert_eq!(co.handle(Input::Key(Key::Ctrl('c'))), Action::None);
    assert_eq!(co.text(), "");
    assert_eq!(co.handle(Input::Key(Key::Ctrl('d'))), Action::Quit);
}

#[test]
fn the_merged_source_serves_the_keyboard_before_the_backlog() {
    let _c = claim();
    let mailbox = Mailbox::new();
    for i in 0..50 {
        mailbox.notify(NoticeOrigin::Push, format!("flood {i}"));
    }
    let mut src = pr_terminal::Merged::new(Scripted::new(Scripted::typing("GET")), mailbox);
    for expected in ['G', 'E', 'T'] {
        assert_eq!(
            src.next(Duration::from_millis(0)),
            Some(Input::Key(Key::Char(expected))),
            "the keyboard must not queue behind the flood"
        );
    }
}

#[test]
fn a_submitted_line_enters_scrollback_before_the_next_prompt() {
    let _c = claim();
    let mut co = coordinator();
    co.start();
    type_str(&mut co, "PING");
    co.sink_mut().clear();
    assert_eq!(
        co.handle(Input::Key(Key::Enter)),
        Action::Submit("PING".into())
    );
    let seen = visible(co.sink());
    let submitted = seen
        .find("penguin> PING")
        .expect("the submitted line is committed to history");
    let next_prompt = seen[submitted + 1..]
        .find("penguin> ")
        .expect("a fresh prompt follows");
    assert!(
        seen[submitted..submitted + next_prompt].contains("\r\n"),
        "the new prompt must be on its own line, not overwriting the old one"
    );
}
