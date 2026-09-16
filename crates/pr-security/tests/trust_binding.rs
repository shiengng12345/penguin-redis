//! V-D04 — what a credential is bound to, and what it survives (v2.1 §21.3, R14, SEC-05).
//!
//! §21.3 makes one decision that everything else follows from: the credential is bound to
//! `(tls_identity, server_identity)` and **not** to the endpoint. The reason is that addresses
//! move for reasons that have nothing to do with trust — DNS, a port mapping, a Sentinel
//! failover to a node already in the set — and a client that re-asks on every address change
//! teaches the user to confirm re-binds without reading them. At that point the confirmation
//! protects nothing, which is worse than not having it.
//!
//! The other half is the one that must never be waved through: a redirect to a node outside
//! the profile's declared bounds. Following a `MOVED` to an arbitrary host and authenticating
//! there is how a compromised or misconfigured cluster collects a password — the *server*
//! chooses the address, and the client would be trusting it because it asked.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_security::trust::{
    Advisory, AuthIdentity, DiscoveryBounds, Endpoint, KnownNodes, RebindVerdict, RedirectDecision,
    RotationVerdict, RotationWindow, ServerIdentity, TlsIdentity, TrustIdentity,
};
use std::collections::BTreeSet;

fn pin(sha: &str) -> TlsIdentity {
    TlsIdentity::SpkiPin {
        sha256: sha.to_owned(),
    }
}

fn ca(server_name: &str, ca_set: &str) -> TlsIdentity {
    TlsIdentity::Ca {
        ca_set_hash: ca_set.to_owned(),
        server_name: server_name.to_owned(),
    }
}

fn sentinel(master: &str, set: &str) -> ServerIdentity {
    ServerIdentity::Sentinel {
        master_name: master.to_owned(),
        sentinel_set_hash: set.to_owned(),
    }
}

fn identity(host: &str, port: u16, tls: TlsIdentity, server: ServerIdentity) -> TrustIdentity {
    TrustIdentity {
        endpoint: Endpoint::Tcp {
            host: host.to_owned(),
            port,
        },
        tls_identity: tls,
        server_identity: server,
        auth_identity: AuthIdentity {
            username: Some("deployer".into()),
            secret_ref: "credential:uuid-prod".into(),
        },
    }
}

fn bounds() -> DiscoveryBounds {
    DiscoveryBounds {
        allowed_hosts: BTreeSet::from(["10.0.0.9:6379".to_owned()]),
        allowed_suffixes: vec![".redis-prod.internal".to_owned()],
    }
}

// ================================================ failover
#[test]
fn a_sentinel_failover_to_a_node_in_the_set_does_not_rebind() {
    // The everyday case. The master moved; the service did not.
    let before = identity(
        "redis-a.redis-prod.internal",
        6379,
        pin("aa"),
        sentinel("mymaster", "sset-1"),
    );
    let after = identity(
        "redis-b.redis-prod.internal",
        6379,
        pin("aa"),
        sentinel("mymaster", "sset-1"),
    );
    let verdict = after.compare(&before);
    assert_eq!(verdict, RebindVerdict::AddressOnly);
    assert!(
        !verdict.requires_rebind(),
        "a failover must not make the user re-approve the credential"
    );
    // And the credential binding really is the same value, not merely judged equal.
    assert_eq!(
        after.credential_binding_hash(),
        before.credential_binding_hash()
    );
}

#[test]
fn the_same_failover_across_a_port_change_is_still_only_an_address_change() {
    let before = identity("redis-a", 6379, pin("aa"), sentinel("mymaster", "sset-1"));
    let after = identity("redis-a", 6380, pin("aa"), sentinel("mymaster", "sset-1"));
    assert!(!after.compare(&before).requires_rebind());
}

#[test]
fn a_different_master_is_a_different_service_and_does_rebind() {
    // `master_name` is the service. Two masters behind one Sentinel set are two services, and
    // a credential for one is not a credential for the other.
    let before = identity("redis-a", 6379, pin("aa"), sentinel("mymaster", "sset-1"));
    let after = identity(
        "redis-a",
        6379,
        pin("aa"),
        sentinel("othermaster", "sset-1"),
    );
    assert!(after.compare(&before).requires_rebind());
    assert_ne!(
        after.credential_binding_hash(),
        before.credential_binding_hash()
    );
}

#[test]
fn adding_a_sentinel_warns_but_does_not_rebind() {
    // §21.3: the sentinel set changing is worth saying; it is not a different service.
    let before = identity("redis-a", 6379, pin("aa"), sentinel("mymaster", "sset-1"));
    let after = identity("redis-a", 6379, pin("aa"), sentinel("mymaster", "sset-2"));
    let verdict = after.compare(&before);
    assert!(
        !verdict.requires_rebind(),
        "a new sentinel must not invalidate the credential: {verdict:?}"
    );
    assert_ne!(
        verdict,
        RebindVerdict::NoChange,
        "but it must not be silent either"
    );
    // And the warning is actually sayable, which is the point: a verdict the caller cannot
    // turn into a sentence is the same as no warning at all.
    let advisory = verdict.advisory().expect("the verdict carries an advisory");
    assert_eq!(advisory, Advisory::SentinelSetChanged);
    assert!(advisory.message().contains("Sentinel set"));
    assert!(
        advisory.message().contains("still"),
        "and it says the credential still applies, so the user is not alarmed: {}",
        advisory.message()
    );
}

#[test]
fn telling_the_user_and_stopping_to_ask_are_different_obligations() {
    // Collapsing them is how a warning becomes either a prompt nobody reads or a silence
    // nobody notices.
    let advisory = RebindVerdict::AdvisoryChange(Advisory::SentinelSetChanged);
    assert!(!advisory.requires_rebind());
    assert!(advisory.advisory().is_some());

    for quiet in [RebindVerdict::NoChange, RebindVerdict::AddressOnly] {
        assert!(!quiet.requires_rebind());
        assert!(quiet.advisory().is_none(), "{quiet:?} has nothing to say");
    }
    for blocking in [
        RebindVerdict::TlsIdentityChanged,
        RebindVerdict::ServerIdentityChanged,
        RebindVerdict::AuthIdentityChanged,
    ] {
        assert!(blocking.requires_rebind());
        assert!(
            blocking.advisory().is_none(),
            "{blocking:?} is a stop, not a note"
        );
    }
}

// ================================================ certificate rotation
#[test]
fn a_rotation_window_accepts_both_identities_while_it_is_open() {
    // A fleet does not swap certificates atomically. During a rollout some nodes answer with
    // the old identity and some with the new; demanding a re-bind on every second connection
    // teaches the user to click through them.
    let w = RotationWindow {
        current: pin("new"),
        previous: Some(pin("old")),
        previous_until_ms: 10_000,
    };
    assert_eq!(w.judge(&pin("new"), 1), RotationVerdict::Current);
    assert_eq!(
        w.judge(&pin("old"), 9_999),
        RotationVerdict::PreviousWithinWindow
    );
    assert!(w.judge(&pin("old"), 9_999).accepts());
}

#[test]
fn the_window_closes_and_the_old_identity_stops_being_accepted() {
    // One-directional and bounded: a rotation that never finishes is one nobody completed.
    let w = RotationWindow {
        current: pin("new"),
        previous: Some(pin("old")),
        previous_until_ms: 10_000,
    };
    assert_eq!(
        w.judge(&pin("old"), 10_000),
        RotationVerdict::PreviousExpired
    );
    assert!(!w.judge(&pin("old"), 10_001).accepts());
    // The current identity never expires.
    assert_eq!(
        w.judge(&pin("new"), u64::MAX),
        RotationVerdict::Current,
        "the identity being rotated *to* has no deadline"
    );
}

#[test]
fn an_unrelated_certificate_is_refused_whether_the_window_is_open_or_not() {
    // The window widens what is accepted by exactly one identity, not by "anything during a
    // rotation".
    let w = RotationWindow {
        current: pin("new"),
        previous: Some(pin("old")),
        previous_until_ms: 10_000,
    };
    assert_eq!(w.judge(&pin("attacker"), 1), RotationVerdict::Unknown);
    assert!(!w.judge(&pin("attacker"), 1).accepts());
    assert!(!w.judge(&TlsIdentity::None, 1).accepts());
}

#[test]
fn a_settled_profile_accepts_only_its_current_identity() {
    let w = RotationWindow::settled(ca("redis.internal", "ca-1"));
    assert!(w.judge(&ca("redis.internal", "ca-1"), 1).accepts());
    assert!(
        !w.judge(&ca("redis.internal", "ca-2"), 1).accepts(),
        "a new CA set is a new identity when no rotation was declared"
    );
    assert!(
        !w.judge(&ca("evil.internal", "ca-1"), 1).accepts(),
        "and so is a different server name"
    );
}

#[test]
fn a_certificate_renewed_under_the_same_key_needs_no_rotation_window_at_all() {
    // This is why `SpkiPin` exists: renewing a certificate without changing the key is the
    // common case, and it should not be an event at all.
    let before = identity("redis-a", 6379, pin("spki-1"), sentinel("m", "s"));
    let after = identity("redis-a", 6379, pin("spki-1"), sentinel("m", "s"));
    assert_eq!(after.compare(&before), RebindVerdict::NoChange);
}

#[test]
fn turning_tls_off_is_always_a_rebind_however_convenient() {
    // §21.3: "不能为了可达就关闭 TLS 校验".
    let before = identity("redis-a", 6379, pin("spki-1"), sentinel("m", "s"));
    let after = identity("redis-a", 6379, TlsIdentity::None, sentinel("m", "s"));
    assert!(after.compare(&before).requires_rebind());
}

// ================================================ redirects
#[test]
fn a_redirect_to_a_known_node_is_followed_without_asking() {
    let known = KnownNodes::new(vec![("10.0.0.1".into(), 6379), ("10.0.0.2".into(), 6379)]);
    assert_eq!(
        known.decide(&bounds(), "10.0.0.2", 6379),
        RedirectDecision::Known
    );
}

#[test]
fn a_new_node_inside_the_bounds_is_followed_and_recorded() {
    // §21.3 allows the node set to grow inside the declared bounds without invalidating
    // anything — a cluster that adds a shard is not a different service.
    let mut known = KnownNodes::new(vec![("10.0.0.1".into(), 6379)]);
    let d = known.decide(&bounds(), "redis-c.redis-prod.internal", 6379);
    assert_eq!(d, RedirectDecision::WithinBounds);
    assert!(known.accept(d, "redis-c.redis-prod.internal", 6379));
    assert_eq!(known.len(), 2);
    assert_eq!(
        known.decide(&bounds(), "redis-c.redis-prod.internal", 6379),
        RedirectDecision::Known,
        "recorded, so the next redirect is silent"
    );
}

#[test]
fn a_redirect_outside_the_bounds_stops_and_asks() {
    // The one that matters. The server chose this address.
    let mut known = KnownNodes::new(vec![("10.0.0.1".into(), 6379)]);
    let d = known.decide(&bounds(), "attacker.example.com", 6379);
    assert_eq!(d, RedirectDecision::ConfirmationRequired);
    assert!(
        !known.accept(d, "attacker.example.com", 6379),
        "it must not be usable on the strength of the redirect alone"
    );
    assert_eq!(known.len(), 1, "and it must not have been recorded");
}

#[test]
fn an_out_of_bounds_node_is_usable_only_after_an_explicit_confirmation() {
    let mut known = KnownNodes::new(vec![]);
    assert_eq!(
        known.decide(&bounds(), "new-dc.example.com", 6379),
        RedirectDecision::ConfirmationRequired
    );
    known.accept_confirmed("new-dc.example.com", 6379);
    assert_eq!(
        known.decide(&bounds(), "new-dc.example.com", 6379),
        RedirectDecision::Known,
        "§21.3: once confirmed, it is written into the profile's known nodes"
    );
}

#[test]
fn a_near_miss_hostname_is_not_inside_the_bounds() {
    // Suffix matching is where this kind of check usually goes wrong.
    let known = KnownNodes::new(vec![]);
    for host in [
        "evil-redis-prod.internal",
        "redis-prod.internal.evil.com",
        "redis-prod.internalx",
        "x.redis-prod.internal.attacker.net",
    ] {
        assert_eq!(
            known.decide(&bounds(), host, 6379),
            RedirectDecision::ConfirmationRequired,
            "{host} was treated as inside the bounds"
        );
    }
    // The genuine suffix is.
    assert_eq!(
        known.decide(&bounds(), "any.redis-prod.internal", 6379),
        RedirectDecision::WithinBounds
    );
}

#[test]
fn an_allowed_host_is_matched_on_the_port_too() {
    let known = KnownNodes::new(vec![]);
    assert_eq!(
        known.decide(&bounds(), "10.0.0.9", 6379),
        RedirectDecision::WithinBounds
    );
    assert_eq!(
        known.decide(&bounds(), "10.0.0.9", 6380),
        RedirectDecision::ConfirmationRequired,
        "a different port on an allowed host is a different endpoint"
    );
}

#[test]
fn empty_bounds_mean_nothing_is_automatic() {
    // A profile that declared no bounds has not declared "everything".
    let known = KnownNodes::new(vec![]);
    let none = DiscoveryBounds::default();
    assert_eq!(
        known.decide(&none, "10.0.0.1", 6379),
        RedirectDecision::ConfirmationRequired
    );
}

// ================================================ the two halves together
#[test]
fn a_failover_during_a_rotation_is_still_only_an_address_change() {
    // Both things happening at once is the realistic case during a maintenance window, and
    // the two rules must not compound into a spurious re-bind.
    let w = RotationWindow {
        current: pin("new"),
        previous: Some(pin("old")),
        previous_until_ms: 10_000,
    };
    let before = identity("redis-a", 6379, pin("old"), sentinel("mymaster", "sset-1"));
    let after = identity("redis-b", 6379, pin("old"), sentinel("mymaster", "sset-1"));

    assert!(w.judge(&pin("old"), 5_000).accepts(), "the window is open");
    assert!(
        !after.compare(&before).requires_rebind(),
        "and the failover is only an address change"
    );
}

#[test]
fn a_rebind_is_still_demanded_when_the_key_actually_changes() {
    // The rotation window must not become a way to accept anything for N days.
    let before = identity("redis-a", 6379, pin("old"), sentinel("mymaster", "sset-1"));
    let after = identity("redis-a", 6379, pin("new"), sentinel("mymaster", "sset-1"));
    assert_eq!(
        after.compare(&before),
        RebindVerdict::TlsIdentityChanged,
        "the identity comparison is independent of any declared window"
    );
}
