//! Who decides what a command does (v2.1 §11.2, §20.2, ADR-030, R19, V-D07).
//!
//! The review found §11.2 allowed `COMMAND` / `COMMAND DOCS` / module metadata to "supplement
//! capability" without ever saying what they may **not** override. Taken literally, a server
//! that reports a write command as readonly could relax the client's own policy — which makes
//! the guardrail controllable by the thing it guards against.
//!
//! So the direction of override is one-way and enforced here:
//!
//! | property | authority | remote metadata may |
//! |---|---|---|
//! | effects, danger, approval level, key extraction | signed local catalog | only annotate `unknown` |
//! | availability on this target | remote introspection | set observed-supported / unsupported |
//! | documentation text | local catalog | supplement, marked `from server` |
//!
//! A poisoned or stale server reply can therefore make the display less accurate. It can
//! never make the policy more permissive.

use pr_core::Effects;

/// What the local signed catalog knows about a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSpec {
    /// Canonical name, uppercase.
    pub name: String,
    /// Locally assigned effects. This is the policy input (ADR-030).
    pub effects: Effects,
    /// Whether the local catalog has an entry at all.
    pub documented: bool,
}

impl LocalSpec {
    /// A documented command.
    #[must_use]
    pub fn known(name: &str, effects: Effects) -> Self {
        Self {
            name: name.to_uppercase(),
            effects,
            documented: true,
        }
    }
    /// A command the local catalog has never heard of.
    #[must_use]
    pub fn unknown(name: &str) -> Self {
        Self {
            name: name.to_uppercase(),
            effects: Effects::unknown(),
            documented: false,
        }
    }
}

/// What the server said about a command. Untrusted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteReport {
    /// The server lists this command.
    pub present: bool,
    /// Flags the server attached, e.g. `readonly`, `write`, `admin`.
    pub flags: Vec<String>,
    /// Free text from `COMMAND DOCS`. Untrusted; must be rendered through `SafeText`.
    pub summary: Option<String>,
}

/// Availability of a command on the connected target (§11.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// The server lists it.
    ObservedSupported,
    /// The server does not list it. Distinct from "unsupported by the product".
    ObservedUnsupported,
    /// No introspection has been done, or it was refused.
    Unknown,
}

/// The resolved view a UI and a policy engine consume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// Canonical name.
    pub name: String,
    /// Effects. **Always** the local catalog's, never the server's (ADR-030).
    pub effects: Effects,
    /// Availability, which is the one thing the server genuinely knows.
    pub availability: Availability,
    /// Server-reported flags, kept for display only.
    pub server_flags: Vec<String>,
    /// True when the server's claim disagrees with the local classification, so the UI can
    /// say `server reports ...` instead of quietly adopting it (SEC-11).
    pub server_disagrees: bool,
}

/// Parse a server flag list into the effects it would imply, for comparison only.
fn effects_from_flags(flags: &[String]) -> Option<Effects> {
    let has = |f: &str| flags.iter().any(|x| x.eq_ignore_ascii_case(f));
    if !has("readonly") && !has("write") && !has("admin") {
        return None;
    }
    Some(Effects {
        reads_data: has("readonly") || has("write"),
        writes_data: has("write"),
        admin: has("admin"),
        blocks: has("blocking"),
        ..Effects::default()
    })
}

/// Combine local authority with remote observation.
///
/// The only way remote data changes `effects` is by annotating a command the local catalog
/// does not classify — and even then it stays `unknown`, because an unclassified command is
/// maximum risk (§23.1) and a server's word is not enough to lower that.
#[must_use]
pub fn resolve(local: &LocalSpec, remote: Option<&RemoteReport>) -> Resolved {
    let availability = match remote {
        None => Availability::Unknown,
        Some(r) if r.present => Availability::ObservedSupported,
        Some(_) => Availability::ObservedUnsupported,
    };

    let server_flags = remote.map(|r| r.flags.clone()).unwrap_or_default();

    // Compare, but never adopt.
    let server_disagrees = match (local.documented, effects_from_flags(&server_flags)) {
        (true, Some(claimed)) => {
            claimed.writes_data != local.effects.writes_data || claimed.admin != local.effects.admin
        }
        _ => false,
    };

    Resolved {
        name: local.name.clone(),
        // The single most important line in this module.
        effects: local.effects,
        availability,
        server_flags,
        server_disagrees,
    }
}

impl Resolved {
    /// Whether policy must treat this as mutating.
    #[must_use]
    pub fn mutates(&self) -> bool {
        self.effects.mutates()
    }

    /// The label a UI shows next to a disagreement, so the server's claim is visible without
    /// being believed.
    #[must_use]
    pub fn disagreement_note(&self) -> Option<String> {
        if !self.server_disagrees {
            return None;
        }
        Some(format!("server reports: {}", self.server_flags.join(", ")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn report(present: bool, flags: &[&str]) -> RemoteReport {
        RemoteReport {
            present,
            flags: flags.iter().map(|s| (*s).to_owned()).collect(),
            summary: None,
        }
    }

    #[test]
    fn a_server_cannot_downgrade_a_write_to_readonly() {
        // SEC-11 / ASSIST-086: the whole reason this module exists.
        let local = LocalSpec::known("SET", Effects::write());
        let lying = report(true, &["readonly", "fast"]);
        let r = resolve(&local, Some(&lying));

        assert!(r.effects.writes_data, "the local classification must stand");
        assert!(r.mutates(), "policy must still treat it as mutating");
        assert!(r.server_disagrees, "but the disagreement is surfaced");
        assert_eq!(
            r.disagreement_note().as_deref(),
            Some("server reports: readonly, fast")
        );
    }

    #[test]
    fn a_server_cannot_strip_the_admin_classification() {
        let local = LocalSpec::known(
            "FLUSHALL",
            Effects {
                admin: true,
                destructive: true,
                writes_data: true,
                ..Effects::default()
            },
        );
        let r = resolve(&local, Some(&report(true, &["readonly"])));
        assert!(r.effects.admin);
        assert!(r.effects.destructive);
        assert!(r.server_disagrees);
    }

    #[test]
    fn an_agreeing_server_produces_no_disagreement_note() {
        let local = LocalSpec::known("GET", Effects::read());
        let r = resolve(&local, Some(&report(true, &["readonly", "fast"])));
        assert!(!r.server_disagrees);
        assert!(r.disagreement_note().is_none());
        assert_eq!(r.availability, Availability::ObservedSupported);
    }

    #[test]
    fn availability_is_the_one_thing_the_server_decides() {
        let local = LocalSpec::known("HGETALL", Effects::read());
        assert_eq!(resolve(&local, None).availability, Availability::Unknown);
        assert_eq!(
            resolve(&local, Some(&report(true, &[]))).availability,
            Availability::ObservedSupported
        );
        assert_eq!(
            resolve(&local, Some(&report(false, &[]))).availability,
            Availability::ObservedUnsupported
        );
    }

    #[test]
    fn unsupported_and_unknown_are_different_answers() {
        // §11.9: "the server does not have it" and "we never asked" must not be merged.
        let local = LocalSpec::known("HGETEX", Effects::write());
        let never_asked = resolve(&local, None);
        let asked_and_absent = resolve(&local, Some(&report(false, &[])));
        assert_ne!(never_asked.availability, asked_and_absent.availability);
    }

    #[test]
    fn an_unclassified_command_stays_unknown_even_if_the_server_calls_it_readonly() {
        // §23.1: unclassified is maximum risk, and a server's word cannot lower it.
        let local = LocalSpec::unknown("NEWMODULE.CMD");
        let r = resolve(&local, Some(&report(true, &["readonly", "fast"])));
        assert!(r.effects.unknown, "must remain unclassified");
        assert!(r.mutates(), "unknown counts as mutating for policy");
        assert_eq!(
            r.availability,
            Availability::ObservedSupported,
            "availability is still learned"
        );
    }

    #[test]
    fn a_missing_remote_report_changes_nothing_about_effects() {
        let local = LocalSpec::known("SET", Effects::write());
        let a = resolve(&local, None);
        let b = resolve(&local, Some(&report(true, &["write"])));
        assert_eq!(a.effects, b.effects);
    }

    #[test]
    fn server_flags_are_kept_for_display_only() {
        let local = LocalSpec::known("GET", Effects::read());
        let r = resolve(&local, Some(&report(true, &["readonly", "\u{1b}[2J"])));
        // They are carried, but they are data — the renderer escapes them via SafeText.
        assert_eq!(r.server_flags.len(), 2);
        assert!(r.effects.reads_data);
        assert!(!r.effects.writes_data);
    }

    #[test]
    fn a_flagless_report_never_counts_as_disagreement() {
        // Many commands report no effect flags at all; silence is not a contradiction.
        let local = LocalSpec::known("SET", Effects::write());
        let r = resolve(&local, Some(&report(true, &["fast", "denyoom"])));
        assert!(!r.server_disagrees);
    }

    #[test]
    fn effects_are_byte_identical_to_the_local_spec_in_every_combination() {
        // The invariant stated once, checked exhaustively over the interesting shapes.
        let locals = [
            LocalSpec::known("GET", Effects::read()),
            LocalSpec::known("SET", Effects::write()),
            LocalSpec::unknown("X.Y"),
            LocalSpec::known(
                "XREADGROUP",
                Effects {
                    reads_data: true,
                    consumes: true,
                    ..Effects::default()
                },
            ),
        ];
        let remotes = [
            None,
            Some(report(true, &[])),
            Some(report(true, &["readonly"])),
            Some(report(true, &["write"])),
            Some(report(true, &["admin"])),
            Some(report(false, &["readonly"])),
        ];
        for l in &locals {
            for rem in &remotes {
                let r = resolve(l, rem.as_ref());
                assert_eq!(
                    r.effects, l.effects,
                    "remote metadata altered effects for {} with {rem:?}",
                    l.name
                );
            }
        }
    }
}
