//! V-D03 — approval binds bytes, not a summary (v2.1 §23.2, R02, SEC-10).
//!
//! SEC-10 states the attack in one line: *two different argv whose displayed summary is the
//! same; one approval token may run only one of them.* It is not a hypothetical. Every way a
//! client makes a command readable — Unicode normalisation, escaping, truncation, masking a
//! secret — is a many-to-one function, and a token bound to the readable form is a token that
//! authorises everything that reduces to it.
//!
//! So §23.2 binds the token to `blake3` over the **length-prefixed argv**, and requires the
//! approver's preview to be expandable to the exact bytes. This file attacks both halves:
//!
//! - a corpus of pairs that collide under a plausible summary, checked to produce different
//!   hashes and to be refused by each other's token
//! - the preview, checked to *not* collide, because a preview that hides the difference makes
//!   the byte binding useless to the person actually deciding

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use bytes::Bytes;
use pr_core::{CommandRequest, Effects, RequestOrigin};
use pr_security::approval::{
    ApprovalRefusal, ApprovalToken, Epochs, ExecutionContext, Issuer, preview, request_hash,
};
use pr_security::trust::{AuthIdentity, Endpoint, ServerIdentity, TlsIdentity, TrustIdentity};

fn req(args: &[&[u8]]) -> CommandRequest {
    CommandRequest::new(
        args.iter().map(|a| Bytes::copy_from_slice(a)).collect(),
        RequestOrigin::User,
        Effects::write(),
    )
}

/// A production identity. `master_name` is what distinguishes two services here, because that
/// is what §21.3 says identity is: the logical service, not the address answering today.
fn identity(master: &str) -> TrustIdentity {
    TrustIdentity {
        endpoint: Endpoint::Tcp {
            host: "redis.internal".into(),
            port: 6379,
        },
        tls_identity: TlsIdentity::SpkiPin {
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        },
        server_identity: ServerIdentity::Sentinel {
            master_name: master.into(),
            sentinel_set_hash: "sset-1".into(),
        },
        auth_identity: AuthIdentity {
            username: Some("deployer".into()),
            secret_ref: "credential:uuid-prod".into(),
        },
    }
}

fn epochs() -> Epochs {
    Epochs {
        auth_identity: 7,
        policy: 3,
        topology: 11,
    }
}

fn token_for(r: &CommandRequest) -> ApprovalToken {
    ApprovalToken::for_request(
        "uuid-prod",
        &identity("redis-prod-a"),
        0,
        epochs(),
        r,
        9_999_999_999_999,
        Issuer::Human,
    )
}

fn ctx(identity: &TrustIdentity, db: u32, e: Epochs) -> ExecutionContext<'_> {
    ExecutionContext {
        profile_uuid: "uuid-prod",
        trust_identity: identity,
        db_index: db,
        epochs: e,
        now_ms: 1_000,
        actions_used: 0,
    }
}

/// A plausible summary: join the arguments with spaces, mask anything that looks like a
/// secret, truncate what is long, and show it as text.
///
/// This is not a straw man — it is what a careful implementation does, and every one of those
/// four steps is many-to-one.
fn naive_summary(r: &CommandRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (i, a) in r.args.iter().enumerate() {
        let text = String::from_utf8_lossy(a).into_owned();
        // Mask the value of an AUTH or a password-ish option.
        let previous = i.checked_sub(1).map(|p| &r.args[p]);
        let masked = if previous
            .is_some_and(|p| p.eq_ignore_ascii_case(b"AUTH") || p.eq_ignore_ascii_case(b"PASSWORD"))
        {
            "****".to_owned()
        } else {
            text
        };
        // Truncate for the width of a prompt.
        let shown: String = if masked.chars().count() > 12 {
            masked.chars().take(12).chain("…".chars()).collect()
        } else {
            masked
        };
        parts.push(shown);
    }
    // Join, which loses the argument boundaries — a reader sees a command line, not an argv.
    let line = parts.join(" ");
    line
        // Compose accents, which is what a terminal and a font do anyway.
        .replace("e\u{301}", "\u{e9}")
        // Drop zero-width characters, which is what a font does whether we like it or not.
        .replace('\u{200b}', "")
        // Show control characters as their escape, which is what any sane display does.
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        // Trim, because trailing space is invisible.
        .trim()
        .to_owned()
}

/// Pairs that a summary cannot tell apart.
fn colliding_pairs() -> Vec<(&'static str, CommandRequest, CommandRequest)> {
    vec![
        (
            "one argument containing a space against two arguments",
            req(&[b"SET", b"a b"]),
            req(&[b"SET", b"a", b"b"]),
        ),
        (
            "argument boundary moved",
            req(&[b"SET", b"a", b"bc"]),
            req(&[b"SET", b"ab", b"c"]),
        ),
        (
            "an empty trailing argument",
            req(&[b"SET", b"k", b""]),
            req(&[b"SET", b"k"]),
        ),
        (
            "composed vs decomposed accent",
            req(&[b"SET", "caf\u{e9}".as_bytes(), b"1"]),
            req(&[b"SET", "cafe\u{301}".as_bytes(), b"1"]),
        ),
        (
            "a zero-width space inside the key",
            req(&[b"DEL", b"session", b"1"]),
            req(&[b"DEL", "session\u{200b}".as_bytes(), b"1"]),
        ),
        (
            "two keys sharing a truncated prefix",
            req(&[b"DEL", b"user:profile:alpha"]),
            req(&[b"DEL", b"user:profile:beta"]),
        ),
        (
            "a masked secret differing behind the mask",
            req(&[b"AUTH", b"correct-horse"]),
            req(&[b"AUTH", b"battery-staple"]),
        ),
        (
            "trailing whitespace",
            req(&[b"DEL", b"queue"]),
            req(&[b"DEL", b"queue "]),
        ),
        (
            "an escaped newline against a real one",
            req(&[b"SET", b"k", b"a\\nb"]),
            req(&[b"SET", b"k", b"a\nb"]),
        ),
    ]
}

// ================================================ SEC-10
#[test]
fn sec_10_a_token_for_one_command_refuses_the_other() {
    // The acceptance case itself, for every pair in the corpus.
    for (name, a, b) in colliding_pairs() {
        let token = token_for(&a);
        let id = identity("redis-prod-a");
        assert_eq!(
            token.authorise(&a, &ctx(&id, 0, epochs())),
            Ok(()),
            "{name}: the approved command must still run"
        );
        assert_eq!(
            token.authorise(&b, &ctx(&id, 0, epochs())),
            Err(ApprovalRefusal::RequestMismatch),
            "{name}: the other command was authorised by the same token"
        );
    }
}

#[test]
fn the_corpus_really_does_collide_under_a_plausible_summary() {
    // Without this, the test above would be proving that different commands hash differently,
    // which is not interesting. The point is that these pairs are the ones a summary *cannot*
    // separate.
    let mut collided = 0;
    for (name, a, b) in colliding_pairs() {
        if naive_summary(&a) == naive_summary(&b) {
            collided += 1;
            println!("{name}: both summarise as {:?}", naive_summary(&a));
        }
    }
    assert!(
        collided >= 6,
        "only {collided} of the pairs collide under the summary, so the corpus is not \
         exercising SEC-10"
    );
}

#[test]
fn every_pair_hashes_differently() {
    for (name, a, b) in colliding_pairs() {
        assert_ne!(request_hash(&a), request_hash(&b), "{name}");
    }
}

// ================================================ the preview
#[test]
fn the_preview_separates_every_pair_the_summary_cannot() {
    // §23.2: the approver's preview must expand to the exact argv, so that "what was seen" and
    // "what was hashed" are the same data. A preview that collides would make the byte binding
    // protect a decision nobody could have made correctly.
    for (name, a, b) in colliding_pairs() {
        assert_ne!(
            preview(&a),
            preview(&b),
            "{name}: the byte view cannot tell them apart either"
        );
    }
}

#[test]
fn the_preview_shows_lengths_and_escapes_what_a_font_would_hide() {
    let r = req(&[b"SET", "k\u{200b}".as_bytes(), b"a\nb"]);
    let p = preview(&r);
    assert!(p.starts_with("argc=3\n"), "{p}");
    assert!(p.contains("argv[0] len=3 \"SET\""), "{p}");
    // A zero-width space is six bytes of escape, not an invisible one.
    assert!(p.contains("\\xe2\\x80\\x8b"), "{p}");
    assert!(
        p.contains("argv[1] len=4"),
        "the length gives it away too: {p}"
    );
    // A newline is shown, not acted on.
    assert!(p.contains("\\x0a"), "{p}");
    assert!(
        !p.contains("a\nb"),
        "the preview must not contain a raw newline"
    );
}

#[test]
fn the_preview_cannot_be_driven_by_a_hostile_argument() {
    // The approver reads this in a terminal. A value carrying OSC 52 or a cursor move would
    // otherwise rewrite the very prompt asking for approval.
    let r = req(&[b"SET", b"k", b"\x1b]52;c;ZXZpbA==\x07\x1b[2J"]);
    let p = preview(&r);
    for bad in ['\u{1b}', '\u{7}'] {
        assert!(!p.contains(bad), "{bad:?} reached the preview: {p:?}");
    }
    assert!(p.contains("\\x1b"), "it is shown as text: {p}");
}

// ================================================ context invalidation
#[test]
fn a_token_does_not_survive_a_change_of_identity_db_or_epoch() {
    // §23.2: "令牌不可跨 profile、DB、身份 epoch 或 TrustIdentity 使用；任一变化即失效."
    let r = req(&[b"FLUSHALL"]);
    let token = token_for(&r);
    let good = identity("redis-prod-a");
    assert_eq!(token.authorise(&r, &ctx(&good, 0, epochs())), Ok(()));

    // A different server behind the same profile — the failover case that must not silently
    // reuse an approval.
    let other = identity("redis-prod-b");
    assert_eq!(
        token.authorise(&r, &ctx(&other, 0, epochs())),
        Err(ApprovalRefusal::TrustIdentityMismatch)
    );

    // A different database.
    assert_eq!(
        token.authorise(&r, &ctx(&good, 1, epochs())),
        Err(ApprovalRefusal::DbMismatch)
    );

    // Each epoch on its own.
    for bump in [
        Epochs {
            auth_identity: 8,
            ..epochs()
        },
        Epochs {
            policy: 4,
            ..epochs()
        },
        Epochs {
            topology: 12,
            ..epochs()
        },
    ] {
        assert_eq!(
            token.authorise(&r, &ctx(&good, 0, bump)),
            Err(ApprovalRefusal::EpochChanged),
            "{bump:?} did not invalidate the token"
        );
    }

    // A different profile entirely.
    let mut wrong_profile = ctx(&good, 0, epochs());
    wrong_profile.profile_uuid = "uuid-dev";
    assert_eq!(
        token.authorise(&r, &wrong_profile),
        Err(ApprovalRefusal::ProfileMismatch)
    );
}

#[test]
fn an_expired_token_is_refused_even_for_the_exact_bytes() {
    let r = req(&[b"DEL", b"k"]);
    let mut token = token_for(&r);
    token.expires_at_ms = 500;
    let id = identity("redis-prod-a");
    let mut c = ctx(&id, 0, epochs());
    c.now_ms = 501;
    assert_eq!(token.authorise(&r, &c), Err(ApprovalRefusal::Expired));
    c.now_ms = 499;
    assert_eq!(token.authorise(&r, &c), Ok(()));
}

#[test]
fn a_token_is_spent_rather_than_reusable() {
    let r = req(&[b"DEL", b"k"]);
    let token = token_for(&r);
    let id = identity("redis-prod-a");
    let mut c = ctx(&id, 0, epochs());
    assert_eq!(token.authorise(&r, &c), Ok(()));
    c.actions_used = token.max_actions;
    assert_eq!(
        token.authorise(&r, &c),
        Err(ApprovalRefusal::ActionBudgetExhausted),
        "one approval must not authorise a second run"
    );
}

#[test]
fn the_refusal_says_which_check_failed() {
    // Not for the user's benefit — for the operator reading an audit log. "Denied" without a
    // reason turns every policy investigation into a guess.
    let reasons = [
        ApprovalRefusal::RequestMismatch,
        ApprovalRefusal::ProfileMismatch,
        ApprovalRefusal::TrustIdentityMismatch,
        ApprovalRefusal::DbMismatch,
        ApprovalRefusal::EpochChanged,
        ApprovalRefusal::Expired,
        ApprovalRefusal::ActionBudgetExhausted,
        ApprovalRefusal::PolicyCannotAuthorise,
    ];
    let mut seen: Vec<String> = reasons.iter().map(|r| format!("{r:?}")).collect();
    let before = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(before, seen.len(), "two refusals share a name");
}

// ================================================ the hash itself
#[test]
fn the_hash_is_over_length_prefixed_arguments_not_their_concatenation() {
    // The reason `["a","bc"]` and `["ab","c"]` differ at all.
    let a = req(&[b"SET", b"a", b"bc"]);
    let b = req(&[b"SET", b"ab", b"c"]);
    assert_ne!(request_hash(&a), request_hash(&b));

    // And the same argv always hashes the same, so a token is reproducible.
    assert_eq!(request_hash(&a), request_hash(&req(&[b"SET", b"a", b"bc"])));
}

#[test]
fn an_empty_argument_is_not_the_same_as_no_argument() {
    let with = req(&[b"SET", b"k", b""]);
    let without = req(&[b"SET", b"k"]);
    assert_ne!(request_hash(&with), request_hash(&without));
    assert!(preview(&with).contains("argc=3"));
    assert!(preview(&without).contains("argc=2"));
}

#[test]
fn case_differences_in_the_command_name_are_preserved() {
    // Redis accepts either, but the approval is over bytes: if the token was issued for what
    // the user typed, that is what must run.
    assert_ne!(
        request_hash(&req(&[b"flushall"])),
        request_hash(&req(&[b"FLUSHALL"]))
    );
}
