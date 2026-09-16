//! V-F07 — the parts of discovery that hold without a server (v2.1 §12.4, §12.5, R08, R09).
//!
//! The live half needs a million keys and Docker; this half needs neither, and covers the
//! rules that are easiest to get quietly wrong: which bytes go on the wire, what an empty page
//! means, and what an empty *result* is allowed to claim.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_intelligence::discovery::{
    Budget, Completeness, Cooldown, Discovery, MatchMode, Step, preview, scan_pattern,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------
// §12.4's table: two modes, byte for byte
// ---------------------------------------------------------------------------------------------

#[test]
fn a_literal_prefix_escapes_every_glob_metacharacter_and_appends_a_star() {
    // §12.4 lists exactly five: `*`, `?`, `[`, `]`, `\`.
    assert_eq!(
        scan_pattern(MatchMode::LiteralPrefix, b"player:"),
        b"player:*".to_vec()
    );
    // The case the issue names.
    assert_eq!(
        scan_pattern(MatchMode::LiteralPrefix, b"player:["),
        b"player:\\[*".to_vec()
    );
    assert_eq!(
        scan_pattern(MatchMode::LiteralPrefix, b"a*b?c[d]e\\f"),
        b"a\\*b\\?c\\[d\\]e\\\\f*".to_vec()
    );
}

#[test]
fn a_raw_glob_is_sent_exactly_as_typed() {
    // Not even a trailing `*`. The user wrote a pattern; adding to it would make it a
    // different pattern, and §12.4 forbids a path where the match condition changes silently.
    for input in [
        &b"player:["[..],
        b"player:*",
        b"h[a-c]llo",
        b"",
        b"*",
        b"\\*",
    ] {
        assert_eq!(scan_pattern(MatchMode::RawGlob, input), input.to_vec());
    }
}

#[test]
fn the_same_input_produces_different_wire_bytes_in_the_two_modes() {
    // The point of having two modes at all, and the thing a user must be able to see.
    let input = b"player:[";
    let p = scan_pattern(MatchMode::LiteralPrefix, input);
    let g = scan_pattern(MatchMode::RawGlob, input);
    assert_ne!(p, g);
    assert_eq!(p, b"player:\\[*".to_vec());
    assert_eq!(g, b"player:[".to_vec());
}

#[test]
fn a_preview_shows_both_what_was_typed_and_what_will_be_sent() {
    // §12.4: 「计划预览中同时显示原字节与转义结果」. Showing only one of them hides the half
    // that matters, and which half that is depends on which mistake was made.
    let p = preview(MatchMode::LiteralPrefix, b"player:[");
    assert!(p.contains("prefix"), "{p}");
    assert!(p.contains("player:["), "the typed bytes are missing: {p}");
    assert!(p.contains("player:\\[*"), "the sent bytes are missing: {p}");

    let g = preview(MatchMode::RawGlob, b"player:[");
    assert!(g.contains("glob"), "{g}");
    assert!(
        !g.contains("\\["),
        "a glob must not be shown as escaped: {g}"
    );
}

#[test]
fn escaping_is_byte_wise_so_a_key_that_is_not_utf8_still_works() {
    // A Redis key is bytes. Escaping per `char` would need a UTF-8 decode that may not exist.
    let input = b"caf\xe9:[";
    assert_eq!(
        scan_pattern(MatchMode::LiteralPrefix, input),
        b"caf\xe9:\\[*".to_vec()
    );
    // And the preview stays readable without lying about the bytes.
    let p = preview(MatchMode::LiteralPrefix, input);
    assert!(p.contains("\\xe9"), "{p}");
}

// ---------------------------------------------------------------------------------------------
// SCAN's real guarantees [R08]
// ---------------------------------------------------------------------------------------------

#[test]
fn an_empty_page_with_a_non_zero_cursor_does_not_end_the_scan() {
    // ASSIST-085 and [R08]. The mistake this prevents is reporting "not found" for a key that
    // is there, because one page happened to be empty.
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget::explicit_default(),
    );
    let t0 = Instant::now();
    assert!(matches!(d.next_step(t0), Step::Scan { cursor: 0, .. }));

    d.feed(17_000, &[], 40);
    let t1 = t0 + Duration::from_millis(100);
    assert!(
        matches!(d.next_step(t1), Step::Scan { cursor: 17_000, .. }),
        "an empty page ended the scan"
    );

    d.feed(0, &[b"player:1".to_vec()], 60);
    assert_eq!(
        d.next_step(t1 + Duration::from_millis(100)),
        Step::Done(Completeness::Exhausted)
    );
    assert_eq!(d.outcome().matches.len(), 1);
}

#[test]
fn a_duplicate_key_is_counted_once() {
    // SCAN may return the same element more than once [R08]. Showing it twice is a bug the
    // user sees; counting it twice is a bug they do not.
    let mut d = Discovery::new(MatchMode::LiteralPrefix, b"p", Budget::explicit_default());
    let t = Instant::now();
    let _ = d.next_step(t);
    d.feed(5, &[b"p:1".to_vec(), b"p:2".to_vec()], 80);
    let _ = d.next_step(t + Duration::from_millis(100));
    d.feed(0, &[b"p:1".to_vec(), b"p:3".to_vec()], 80);

    let o = d.outcome();
    assert_eq!(
        o.matches,
        vec![b"p:1".to_vec(), b"p:2".to_vec(), b"p:3".to_vec()]
    );
    assert_eq!(o.progress.matched, 3);
    assert_eq!(o.progress.keys_seen, 4, "the duplicate was still received");
}

// ---------------------------------------------------------------------------------------------
// Budgets, and what running out of one is allowed to claim
// ---------------------------------------------------------------------------------------------

#[test]
fn the_scan_budget_stops_the_run_and_names_itself() {
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"nothing:",
        Budget {
            max_scans: 3,
            max_requests_per_sec: 1_000_000,
            ..Budget::explicit_default()
        },
    );
    let mut t = Instant::now();
    for _ in 0..3 {
        let Step::Scan { .. } = d.next_step(t) else {
            panic!("expected a scan");
        };
        d.feed(99, &[], 40);
        t += Duration::from_millis(1);
    }
    assert_eq!(
        d.next_step(t),
        Step::Done(Completeness::BudgetReached {
            limit: "SCAN count"
        })
    );
    assert_eq!(d.progress().scans_used, 3);
}

#[test]
fn the_time_budget_stops_the_run_even_if_scans_remain() {
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"x",
        Budget {
            max_duration: Duration::from_secs(10),
            ..Budget::explicit_default()
        },
    );
    let t0 = Instant::now();
    let _ = d.next_step(t0);
    d.feed(7, &[], 40);
    assert_eq!(
        d.next_step(t0 + Duration::from_secs(11)),
        Step::Done(Completeness::BudgetReached { limit: "time" })
    );
    assert!(d.progress().scans_left > 0, "scans were still available");
}

#[test]
fn the_response_size_budget_stops_the_run() {
    // §12.5's 「发现响应处理预算 1 MiB」: a server can send an unexpectedly large page, and the
    // client has to stop somewhere that is not "when memory runs out".
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"x",
        Budget {
            max_response_bytes: 1000,
            max_requests_per_sec: 1_000_000,
            ..Budget::explicit_default()
        },
    );
    let t = Instant::now();
    let _ = d.next_step(t);
    d.feed(7, &[b"k".to_vec()], 1200);
    assert_eq!(
        d.next_step(t + Duration::from_millis(1)),
        Step::Done(Completeness::BudgetReached {
            limit: "response size"
        })
    );
}

#[test]
fn the_rate_limit_is_a_wait_rather_than_a_failure() {
    // The run is still inside its other budgets; it is simply not allowed to ask yet.
    // Reporting "incomplete" here would be wrong and would also make the run stop early.
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"x",
        Budget {
            max_requests_per_sec: 20,
            ..Budget::explicit_default()
        },
    );
    let t0 = Instant::now();
    assert!(matches!(d.next_step(t0), Step::Scan { .. }));
    d.feed(7, &[], 40);

    let Step::Wait(w) = d.next_step(t0 + Duration::from_millis(5)) else {
        panic!("expected a wait");
    };
    assert!(
        w <= Duration::from_millis(50) && w > Duration::ZERO,
        "{w:?}"
    );
    assert!(
        matches!(
            d.next_step(t0 + Duration::from_millis(60)),
            Step::Scan { .. }
        ),
        "the run did not resume after the wait"
    );
}

#[test]
fn an_empty_result_never_claims_the_key_does_not_exist() {
    // §12.4, in as many words: 「不能显示 `key does not exist`」. The client's budget says
    // nothing about the server's contents, and a user told "does not exist" stops looking.
    let unfinished = [
        Completeness::BudgetReached {
            limit: "SCAN count",
        },
        Completeness::BudgetReached { limit: "time" },
        Completeness::Cancelled,
        Completeness::Denied {
            reason: "NOPERM".into(),
        },
    ];
    for c in &unfinished {
        let m = c.empty_message();
        assert!(!c.is_conclusive(), "{c:?} claims to be conclusive");
        assert!(
            !m.to_lowercase().contains("does not exist") && !m.to_lowercase().contains("not found"),
            "{c:?} claims absence: {m}"
        );
    }
    for c in &unfinished[..3] {
        assert!(
            c.empty_message().contains("discovery incomplete"),
            "{c:?} does not say it is incomplete: {}",
            c.empty_message()
        );
    }

    // Exhausted is the one case where "no matches" means something -- and even it says why it
    // is not a snapshot.
    let e = Completeness::Exhausted;
    assert!(e.is_conclusive());
    assert!(
        e.empty_message().contains("no snapshot"),
        "{}",
        e.empty_message()
    );
}

#[test]
fn a_summary_with_matches_still_says_how_much_was_looked_at() {
    // Finding something is not the same as finding everything. A user who saw three results
    // and stopped, because nothing said there might be more, has been misled by omission.
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget {
            max_scans: 1,
            max_requests_per_sec: 1_000_000,
            ..Budget::explicit_default()
        },
    );
    let t = Instant::now();
    let _ = d.next_step(t);
    d.feed(999, &[b"player:1".to_vec()], 60);
    let _ = d.next_step(t + Duration::from_millis(1));

    let s = d.outcome().summary();
    assert!(s.contains("1 match"), "{s}");
    assert!(s.contains("discovery incomplete"), "{s}");
    assert!(s.contains("observed portion"), "{s}");
}

#[test]
fn cancelling_stops_the_run_and_keeps_what_was_found() {
    let mut d = Discovery::new(MatchMode::LiteralPrefix, b"p", Budget::explicit_default());
    let t = Instant::now();
    let _ = d.next_step(t);
    d.feed(5, &[b"p:1".to_vec()], 60);
    d.cancel();
    assert_eq!(
        d.next_step(t + Duration::from_millis(100)),
        Step::Done(Completeness::Cancelled)
    );
    let o = d.outcome();
    assert_eq!(o.matches.len(), 1, "cancelling threw away a real result");
    assert!(!o.completeness.is_conclusive());
}

// ---------------------------------------------------------------------------------------------
// NOPERM cooldown (ASSIST-084)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_refusal_starts_a_cooldown_scoped_to_the_profile_and_database() {
    // An ACL rule's scope is the connection's identity and database, so the cooldown's is too.
    // A global cooldown would punish a profile that is allowed to scan.
    let mut c = Cooldown::new();
    let t = Instant::now();
    assert!(c.check("prod", 0, t).is_ok());

    c.refused("prod", 0, t);
    let left = c.check("prod", 0, t + Duration::from_secs(1)).unwrap_err();
    assert!(
        left <= Duration::from_secs(59) && left > Duration::ZERO,
        "{left:?}"
    );

    // A different database, and a different profile, are unaffected.
    assert!(c.check("prod", 1, t).is_ok());
    assert!(c.check("dev", 0, t).is_ok());

    // And it expires.
    assert!(c.check("prod", 0, t + Duration::from_secs(61)).is_ok());
}

#[test]
fn a_person_can_clear_a_cooldown_because_they_may_know_the_acl_changed() {
    // The cooldown exists to stop *automatic* retries walking into an ACL wall. It is not
    // there to argue with someone who just fixed the permission.
    let mut c = Cooldown::new();
    let t = Instant::now();
    c.refused("prod", 0, t);
    assert!(c.check("prod", 0, t).is_err());
    c.cleared_by_user("prod", 0);
    assert!(c.check("prod", 0, t).is_ok());
}

#[test]
fn a_denied_run_concludes_nothing_about_the_keyspace() {
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget::explicit_default(),
    );
    let t = Instant::now();
    let _ = d.next_step(t);
    d.deny("NOPERM this user has no permissions to run the 'scan' command");

    let o = d.outcome();
    assert!(!o.completeness.is_conclusive());
    let m = o.summary();
    assert!(m.contains("refused"), "{m}");
    assert!(
        m.contains("nothing can be concluded"),
        "a refusal must not read as an absence: {m}"
    );
}

// ---------------------------------------------------------------------------------------------
// The automatic budget is deliberately tiny
// ---------------------------------------------------------------------------------------------

#[test]
fn the_automatic_budget_is_far_smaller_than_the_explicit_one() {
    // §12.5: automatic discovery is 2 requests/s, ≤ 2 SCANs, ≤ 1 s. Typing must not turn into
    // a scan of somebody's production keyspace, so the two budgets are not variations of each
    // other -- they are different orders of magnitude, and this asserts they stay that way.
    let a = Budget::automatic_default();
    let e = Budget::explicit_default();
    assert!(
        a.max_scans * 10 <= e.max_scans,
        "{} vs {}",
        a.max_scans,
        e.max_scans
    );
    assert!(a.max_duration * 5 <= e.max_duration);
    assert!(a.max_requests_per_sec * 5 <= e.max_requests_per_sec);
    assert_eq!(a.max_scans, 2);
    assert_eq!(e.count_hint, 1000);
}

#[test]
fn the_explicit_scan_budget_matches_what_its_rate_and_duration_allow() {
    // ADR-033, as an invariant rather than a number. §12.5 shipped `max_scans = 50` with
    // 20 requests/second and 10 seconds — but 20 × 10 is 200, so the scan count stopped the
    // run at a quarter of the time budget and the limit that bit was not the one the user
    // experiences. V-F07 measured exactly that against 10^6 keys.
    //
    // Written as a relationship so that changing the rate or the duration without changing the
    // count fails here, rather than silently reintroducing the same mismatch.
    let e = Budget::explicit_default();
    let allowed = u128::from(e.max_requests_per_sec) * e.max_duration.as_millis() / 1000;
    assert_eq!(
        u128::from(e.max_scans),
        allowed,
        "the SCAN budget ({}) and what {} req/s for {:?} allows ({allowed}) disagree",
        e.max_scans,
        e.max_requests_per_sec,
        e.max_duration
    );
}
