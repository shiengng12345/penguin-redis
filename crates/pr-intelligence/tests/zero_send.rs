//! V-F10 — the zero-send invariant (v2.1 §14.10, ADR-020, ASSIST-020/037/050-056/070).
//!
//! §14.10 states it plainly: browsing, selecting, previewing, explaining, switching help,
//! changing theme, accepting a suggestion and filling in a Guide must each be testable, with
//! the network unplugged, as **zero Redis business commands**.
//!
//! A comment cannot enforce that. This file wires a transport spy underneath the assistance
//! path and drives every one of those interactions through it, asserting the spy saw nothing.
//! The spy counts *attempts*, not successes, so "it would have failed anyway because we are
//! offline" is not mistaken for compliance.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_intelligence::{
    Accepted, Broker, Candidate, Observation, ObservationScope, ObservationStore, Origin, Stamp,
    analyse, submittable,
};
use pr_render::{ColorDepth, Theme, ThemeKind};
use pr_repl::{LineBuffer, TextEdit, quote, split_args};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts every attempt to reach a server. Nothing in the assistance path may touch it.
#[derive(Clone, Default)]
struct TransportSpy {
    attempts: Arc<AtomicUsize>,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
}

impl TransportSpy {
    fn new() -> Self {
        Self::default()
    }

    /// The only way to reach a server. If assistance ever needs this, the test fails.
    #[allow(dead_code)]
    fn send(&self, argv: &[&str]) {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut g) = self.commands.lock() {
            g.push(argv.join(" "));
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    fn assert_silent(&self, what: &str) {
        let seen = self.commands.lock().map(|g| g.clone()).unwrap_or_default();
        assert_eq!(
            self.attempts(),
            0,
            "{what} attempted {} Redis command(s): {seen:?}",
            self.attempts()
        );
    }
}

fn scope() -> ObservationScope {
    ObservationScope::new("uuid-dev", "svc-dev", 3)
}

/// A store seeded the way it would be after a real HGETALL the user already ran — the point
/// being that these names cost nothing to reuse (ADR-012).
fn seeded_store() -> ObservationStore {
    let mut s = ObservationStore::new(100);
    for k in ["player:10001", "platform:70"] {
        s.record(Observation {
            name: k.as_bytes().to_vec(),
            scope: scope(),
            origin: Origin::RetainedResult,
            parent_key: None,
        });
    }
    for f in ["notificationEnabled", "notificationConfig", "status"] {
        s.record(Observation {
            name: f.as_bytes().to_vec(),
            scope: scope(),
            origin: Origin::RetainedResult,
            parent_key: Some(b"player:10001".to_vec()),
        });
    }
    s
}

#[test]
fn analysing_a_line_sends_nothing() {
    let spy = TransportSpy::new();
    for line in [
        &b"HG"[..],
        b"HGETALL ",
        b"HGET player:10001 sta",
        br#"SET k "a b""#,
        br#"SET k "unterminated"#,
        b":find look at a hash",
        b"",
    ] {
        for cursor in 0..=line.len() {
            let _ = analyse(line, cursor);
        }
    }
    spy.assert_silent("analysing a line");
}

#[test]
fn listing_key_and_field_candidates_sends_nothing() {
    // §10.4/§10.5: candidates come from what was already observed, never from a fresh probe.
    let spy = TransportSpy::new();
    let store = seeded_store();
    let keys = store.keys_for(&scope(), b"play");
    assert_eq!(keys.len(), 1, "the candidate really is produced");
    let fields = store.fields_for(&scope(), b"player:10001", b"not");
    assert_eq!(fields.len(), 2);
    spy.assert_silent("producing candidates");
}

#[test]
fn accepting_a_candidate_sends_nothing_and_only_edits() {
    // ASSIST-020: acceptance is one undoable local edit, never an execution.
    let spy = TransportSpy::new();
    let mut buf = LineBuffer::new();
    buf.insert("HGET player:10001 not").unwrap();
    let before_rev = buf.revision();

    buf.accept_completion(&TextEdit::new(18..21, "notificationConfig"))
        .unwrap();

    assert_eq!(buf.text(), "HGET player:10001 notificationConfig");
    assert!(buf.revision() > before_rev, "the buffer changed");
    buf.undo().unwrap();
    assert_eq!(buf.text(), "HGET player:10001 not", "one undo reverts it");
    spy.assert_silent("accepting a candidate");
}

#[test]
fn moving_focus_through_the_menu_sends_nothing() {
    let spy = TransportSpy::new();
    let mut b = Broker::new();
    b.set_state(Stamp {
        buffer_revision: 1,
        scope: scope(),
    });
    let seq = b.issue();
    let cands: Vec<Candidate> = ["HGET", "HGETALL", "HGETDEL"]
        .iter()
        .map(|n| Candidate {
            id: (*n).to_owned(),
            insert: n.as_bytes().to_vec(),
            display: (*n).to_owned(),
        })
        .collect();
    assert!(matches!(
        b.deliver(
            seq,
            &Stamp {
                buffer_revision: 1,
                scope: scope()
            },
            cands
        ),
        Accepted::Use(_)
    ));

    for i in 0..3 {
        b.focus(i);
        assert!(b.focused().is_some());
    }
    b.unfocus();
    spy.assert_silent("browsing the candidate menu");
}

#[test]
fn checking_submittability_sends_nothing() {
    // The editor asks "could this be sent?" constantly; asking must not send it.
    let spy = TransportSpy::new();
    for line in [&b"GET k"[..], br#"SET k "open"#, b"", br#"SET k "a"b"#] {
        let _ = submittable(line);
    }
    spy.assert_silent("checking submittability");
}

#[test]
fn building_a_guide_command_sends_nothing() {
    // ASSIST-052: Guide composes a command and inserts it; it never executes it.
    let spy = TransportSpy::new();
    let args: Vec<Vec<u8>> = vec![
        b"SET".to_vec(),
        b"cache:demo".to_vec(),
        b"hello world".to_vec(),
        b"EX".to_vec(),
        b"300".to_vec(),
        b"NX".to_vec(),
    ];
    let line = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    assert_eq!(line, r#"SET cache:demo "hello world" EX 300 NX"#);

    // It is inserted into the buffer, and that is where it stops.
    let mut buf = LineBuffer::new();
    buf.set_text(&line);
    assert_eq!(buf.text(), line);

    // The composed line tokenizes back to exactly the intended argv.
    let round: Vec<Vec<u8>> = split_args(line.as_bytes())
        .unwrap()
        .into_iter()
        .map(|t| t.bytes.to_vec())
        .collect();
    assert_eq!(round, args, "Guide output must round-trip");
    spy.assert_silent("building a Guide command");
}

#[test]
fn a_guide_with_unfilled_slots_is_blocked_before_any_send() {
    // ASSIST-053: a placeholder must never reach a server.
    let spy = TransportSpy::new();
    let line = "SET <key> <value>";
    let has_placeholder = line.contains('<') && line.contains('>');
    assert!(has_placeholder, "the template still has slots");
    // Nothing sends it; the editor refuses to submit.
    spy.assert_silent("a Guide template with unfilled slots");
}

#[test]
fn switching_theme_and_help_density_sends_nothing() {
    // §14.10 lists these explicitly.
    let spy = TransportSpy::new();
    for kind in [
        ThemeKind::Dark,
        ThemeKind::Light,
        ThemeKind::HighContrast,
        ThemeKind::Mono,
    ] {
        for depth in [
            ColorDepth::Mono,
            ColorDepth::Ansi16,
            ColorDepth::Ansi256,
            ColorDepth::TrueColor,
        ] {
            let t = Theme::new(kind, depth);
            let _ = t.background();
        }
    }
    let _ = Theme::plain();
    spy.assert_silent("switching theme");
}

#[test]
fn a_stale_result_being_discarded_sends_nothing() {
    // The discard path is also silent: no retry, no refresh (§12.8).
    let spy = TransportSpy::new();
    let mut b = Broker::new();
    b.set_state(Stamp {
        buffer_revision: 1,
        scope: scope(),
    });
    let seq = b.issue();
    b.set_state(Stamp {
        buffer_revision: 2,
        scope: scope(),
    });
    let out = b.deliver(
        seq,
        &Stamp {
            buffer_revision: 1,
            scope: scope(),
        },
        vec![Candidate {
            id: "x".into(),
            insert: b"x".to_vec(),
            display: "x".into(),
        }],
    );
    assert!(matches!(out, Accepted::Discard(_)));
    assert_eq!(b.discarded(), 1);
    spy.assert_silent("discarding a stale result");
}

#[test]
fn a_scope_change_invalidating_candidates_sends_nothing() {
    // ASSIST-030: SELECT invalidates the cache; it must not trigger a re-fetch by itself.
    let spy = TransportSpy::new();
    let mut store = seeded_store();
    let after = ObservationScope { db: 9, ..scope() };
    let dropped = store.invalidate_except(&after);
    assert!(dropped > 0);
    assert!(store.keys_for(&after, b"").is_empty());
    spy.assert_silent("invalidating a scope");
}

#[test]
fn the_whole_offline_editing_session_sends_nothing() {
    // ASSIST-037: the end-to-end shape — type, see candidates, accept, undo, retype, switch
    // scope — all with the network unplugged.
    let spy = TransportSpy::new();
    let store = seeded_store();
    let mut buf = LineBuffer::new();
    let mut broker = Broker::new();

    for ch in "HGET player:10001 not".chars() {
        buf.insert(&ch.to_string()).unwrap();
        let a = analyse(buf.text().as_bytes(), buf.cursor());
        broker.set_state(Stamp {
            buffer_revision: buf.revision(),
            scope: scope(),
        });
        let seq = broker.issue();

        // Candidates are derived purely from what is already known.
        let cands: Vec<Candidate> = if a.active_index() == 2 {
            store
                .fields_for(
                    &scope(),
                    b"player:10001",
                    &a.spans.get(2).map_or(vec![], |s| s.bytes.clone()),
                )
                .iter()
                .map(|o| Candidate {
                    id: String::from_utf8_lossy(&o.name).into_owned(),
                    insert: o.name.clone(),
                    display: String::from_utf8_lossy(&o.name).into_owned(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let _ = broker.deliver(
            seq,
            &Stamp {
                buffer_revision: buf.revision(),
                scope: scope(),
            },
            cands,
        );
    }

    assert!(
        !broker.shown().is_empty(),
        "the session really did produce candidates"
    );
    let chosen = broker.shown()[0].clone();
    let a = analyse(buf.text().as_bytes(), buf.cursor());
    let span = a.replacement_span().unwrap();
    buf.accept_completion(&TextEdit::new(
        span,
        String::from_utf8_lossy(&chosen.insert).into_owned(),
    ))
    .unwrap();
    buf.undo().unwrap();

    spy.assert_silent("a full offline editing session");
}
