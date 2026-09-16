//! Observation scope isolation (v2.1 §12.6, ADR-017, R13, V-F06).
//!
//! A suggestion is built from names the user has already seen. That makes the cache key a
//! security boundary, not a performance detail: a key observed on prod must never appear
//! while the user is pointed at dev (ASSIST-029).
//!
//! v2.0 keyed on `host:port` plus a profile name. The review found two holes and R13 closed
//! them: the topology epoch was named in §12.6 but missing from the config's scope string,
//! and a renamed profile would have changed its own key. So the key is the profile **UUID**
//! (stable across renames) plus service identity, DB, auth epoch, policy epoch, and topology
//! epoch.

use std::fmt;

/// Everything that must match before an observed name may be reused.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObservationScope {
    /// Stable profile id. Renaming a profile keeps its observations; re-pointing it does not.
    pub profile_uuid: String,
    /// Hash of the trust identity (`pr_security::TrustIdentity::hash`).
    pub service_identity: String,
    /// Logical database.
    pub db: u32,
    /// Bumped by AUTH / HELLO / RESET.
    pub auth_epoch: u64,
    /// Bumped when local policy changes.
    pub policy_epoch: u64,
    /// Bumped on cluster reshard or failover. R13 added this; it was missing from the
    /// configuration schema even though §12.6 required it.
    pub topology_epoch: u64,
}

impl fmt::Display for ObservationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/db{}/a{}/p{}/t{}",
            self.profile_uuid,
            self.service_identity,
            self.db,
            self.auth_epoch,
            self.policy_epoch,
            self.topology_epoch
        )
    }
}

impl ObservationScope {
    /// A scope for tests and for the simple standalone case.
    #[must_use]
    pub fn new(profile_uuid: &str, service_identity: &str, db: u32) -> Self {
        Self {
            profile_uuid: profile_uuid.to_owned(),
            service_identity: service_identity.to_owned(),
            db,
            auth_epoch: 0,
            policy_epoch: 0,
            topology_epoch: 0,
        }
    }

    /// What one copy of this scope costs to hold.
    ///
    /// Counted because 5,000 copies of it is not a rounding error: at V-F09's measurement it
    /// was 1.2 MiB, more than half of the observed-name store's whole budget.
    #[must_use]
    pub fn byte_cost(&self) -> usize {
        std::mem::size_of::<Self>() + self.profile_uuid.len() + self.service_identity.len()
    }

    /// Whether a name observed in `other` may be offered while in `self`.
    #[must_use]
    pub fn accepts(&self, other: &Self) -> bool {
        self == other
    }
}

/// Where a candidate name came from, which the UI must show (§10.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Extracted from a reply the user already received. Costs nothing to reuse.
    RetainedResult,
    /// From the user's own allowed history.
    History,
    /// From an explicitly configured project schema. A suggestion, not a fact (§13.10).
    SchemaHint,
    /// From an explicit discovery action the user started.
    Discovery,
}

impl Origin {
    /// The label shown beside a candidate, so its provenance is never implied.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::RetainedResult => "observed on this connection",
            Self::History => "from history",
            Self::SchemaHint => "schema suggestion",
            Self::Discovery => "discovered",
        }
    }

    /// Whether this origin asserts the name currently exists on the server.
    ///
    /// A schema hint does not: §13.10 is explicit that a configured field name is a team
    /// convention, not evidence the field is there.
    #[must_use]
    pub fn asserts_existence(self) -> bool {
        matches!(self, Self::RetainedResult | Self::Discovery)
    }
}

/// A name the engine may offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// The name, exactly as it appeared on the wire.
    pub name: Vec<u8>,
    /// Scope it was captured in.
    pub scope: ObservationScope,
    /// Where it came from.
    pub origin: Origin,
    /// For field names: the key they belong to. Fields from one key must never be offered
    /// for another (§10.5, ASSIST-005).
    pub parent_key: Option<Vec<u8>>,
}

/// A bounded store of observed names.
///
/// §12.5: 「已观察 key 名缓存 | **5,000 个或 2 MiB，先到者为准**」. Both halves, because either
/// alone is the wrong limit: 5,000 names of 400 bytes is 2 MiB, and 5,000 names of 40 KiB is
/// 200 MB. V-F09 found this store enforcing only the entry count.
#[derive(Debug, Default)]
pub struct ObservationStore {
    items: Vec<Observation>,
    bytes: usize,
    max_items: usize,
    max_bytes: usize,
    evicted: u64,
}

/// §12.5's byte half of the observed-name budget.
pub const MAX_OBSERVED_BYTES: usize = 2 * 1024 * 1024;

/// What one observation costs to hold.
///
/// The name and the parent key are the variable part; the scope is three owned strings plus a
/// `u32`, and it is counted because 5,000 copies of it is not a rounding error — it was 1.2 MiB
/// in V-F09's measurement, which is more than half of this store's whole budget.
fn observation_cost(o: &Observation) -> usize {
    o.name.len() + o.parent_key.as_ref().map_or(0, Vec::len) + o.scope.byte_cost()
}

impl ObservationStore {
    /// A store holding at most `max_items` names, or [`MAX_OBSERVED_BYTES`], whichever first.
    #[must_use]
    pub fn new(max_items: usize) -> Self {
        Self::with_budget(max_items, MAX_OBSERVED_BYTES)
    }

    /// A store with both limits given explicitly.
    #[must_use]
    pub fn with_budget(max_items: usize, max_bytes: usize) -> Self {
        Self {
            items: Vec::new(),
            bytes: 0,
            max_items,
            max_bytes,
            evicted: 0,
        }
    }

    /// Bytes currently held, **including the container's own overhead**.
    ///
    /// The `Vec`'s reserved capacity counts, not only what is in use: V-F09 measured a cache
    /// costing 2.5x its stated budget, and all of the difference was bookkeeping the budget
    /// did not count. Reserved-but-unused capacity is resident memory whether or not the
    /// cache admits to it.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes + self.items.capacity() * std::mem::size_of::<Observation>()
    }

    /// How many names were dropped to stay inside the budget.
    ///
    /// Exposed rather than hidden: a cache that silently forgets looks, from the outside,
    /// exactly like a server that lost the key.
    #[must_use]
    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    /// Record a name, evicting oldest-first until both limits hold.
    ///
    /// A single name larger than the whole byte budget is refused rather than emptying the
    /// store to make room for it: 「超长名字受单项限制，仍可手输」.
    pub fn record(&mut self, o: Observation) {
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|e| e.name == o.name && e.scope == o.scope && e.parent_key == o.parent_key)
        {
            existing.origin = o.origin;
            return;
        }
        let cost = observation_cost(&o);
        if cost > self.max_bytes {
            self.evicted += 1;
            return;
        }
        while !self.items.is_empty()
            && (self.items.len() + 1 > self.max_items || self.bytes() + cost > self.max_bytes)
        {
            let gone = self.items.remove(0);
            self.bytes -= observation_cost(&gone);
            self.evicted += 1;
        }
        self.bytes += cost;
        self.items.push(o);
    }

    /// Number of names held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }
    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Candidates for a key position in `scope` matching `prefix`.
    #[must_use]
    pub fn keys_for(&self, scope: &ObservationScope, prefix: &[u8]) -> Vec<&Observation> {
        self.items
            .iter()
            .filter(|o| {
                o.parent_key.is_none() && scope.accepts(&o.scope) && o.name.starts_with(prefix)
            })
            .collect()
    }

    /// Candidates for a field position: bound to one key, in one scope.
    #[must_use]
    pub fn fields_for(
        &self,
        scope: &ObservationScope,
        key: &[u8],
        prefix: &[u8],
    ) -> Vec<&Observation> {
        self.items
            .iter()
            .filter(|o| {
                o.parent_key.as_deref() == Some(key)
                    && scope.accepts(&o.scope)
                    && o.name.starts_with(prefix)
            })
            .collect()
    }

    /// Drop everything outside `keep`, e.g. after AUTH or a reshard.
    pub fn invalidate_except(&mut self, keep: &ObservationScope) -> usize {
        let before = self.items.len();
        self.items.retain(|o| keep.accepts(&o.scope));
        before - self.items.len()
    }

    /// Drop everything. `:complete clear`.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn dev() -> ObservationScope {
        ObservationScope::new("uuid-dev", "svc-dev", 3)
    }
    fn prod() -> ObservationScope {
        ObservationScope::new("uuid-prod", "svc-prod", 0)
    }

    fn key(name: &str, scope: ObservationScope) -> Observation {
        Observation {
            name: name.as_bytes().to_vec(),
            scope,
            origin: Origin::RetainedResult,
            parent_key: None,
        }
    }

    fn field(name: &str, parent: &str, scope: ObservationScope) -> Observation {
        Observation {
            name: name.as_bytes().to_vec(),
            scope,
            origin: Origin::RetainedResult,
            parent_key: Some(parent.as_bytes().to_vec()),
        }
    }

    #[test]
    fn a_prod_name_never_appears_on_dev() {
        // ASSIST-029: the reason this is a security boundary and not a cache detail.
        let mut s = ObservationStore::new(100);
        s.record(key("prod:secret", prod()));
        s.record(key("dev:thing", dev()));
        let on_dev = s.keys_for(&dev(), b"");
        assert_eq!(on_dev.len(), 1);
        assert_eq!(on_dev[0].name, b"dev:thing");
    }

    #[test]
    fn every_epoch_is_part_of_the_key() {
        let base = dev();
        let variants = [
            ObservationScope {
                db: 9,
                ..base.clone()
            },
            ObservationScope {
                auth_epoch: 1,
                ..base.clone()
            },
            ObservationScope {
                policy_epoch: 1,
                ..base.clone()
            },
            // R13: this one was missing from the config schema in v2.0.
            ObservationScope {
                topology_epoch: 1,
                ..base.clone()
            },
            ObservationScope {
                service_identity: "other".into(),
                ..base.clone()
            },
            ObservationScope {
                profile_uuid: "other".into(),
                ..base.clone()
            },
        ];
        let mut s = ObservationStore::new(100);
        s.record(key("k", base.clone()));
        for v in variants {
            assert!(
                s.keys_for(&v, b"").is_empty(),
                "a name leaked across a changed scope: {v}"
            );
        }
        assert_eq!(
            s.keys_for(&base, b"").len(),
            1,
            "the original scope still sees it"
        );
    }

    #[test]
    fn a_topology_change_invalidates_observations() {
        // ASSIST-088: after a reshard the old node's names may be wrong.
        let before = dev();
        let after = ObservationScope {
            topology_epoch: 1,
            ..before.clone()
        };
        let mut s = ObservationStore::new(100);
        s.record(key("player:1", before));
        assert!(s.keys_for(&after, b"").is_empty());
        assert_eq!(s.invalidate_except(&after), 1);
        assert!(s.is_empty());
    }

    #[test]
    fn fields_are_bound_to_their_key() {
        // ASSIST-005 / §10.5: player:10002's fields must not be offered for player:10001.
        let mut s = ObservationStore::new(100);
        s.record(field("notificationEnabled", "player:10001", dev()));
        s.record(field("notForYou", "player:10002", dev()));

        let for_1 = s.fields_for(&dev(), b"player:10001", b"");
        assert_eq!(for_1.len(), 1);
        assert_eq!(for_1[0].name, b"notificationEnabled");

        let for_2 = s.fields_for(&dev(), b"player:10002", b"");
        assert_eq!(for_2.len(), 1);
        assert_eq!(for_2[0].name, b"notForYou");

        assert!(s.fields_for(&dev(), b"player:99999", b"").is_empty());
    }

    #[test]
    fn a_field_is_not_offered_as_a_key() {
        let mut s = ObservationStore::new(100);
        s.record(field("status", "player:1", dev()));
        assert!(s.keys_for(&dev(), b"").is_empty(), "a field is not a key");
    }

    #[test]
    fn prefix_matching_is_byte_exact() {
        // §11.10: key candidates match by bytes, not fuzzily, unless the user opts in.
        let mut s = ObservationStore::new(100);
        s.record(key("player:10001", dev()));
        s.record(key("platform:70", dev()));
        assert_eq!(s.keys_for(&dev(), b"play").len(), 1);
        assert_eq!(s.keys_for(&dev(), b"plat").len(), 1);
        assert_eq!(s.keys_for(&dev(), b"pl").len(), 2);
        assert!(
            s.keys_for(&dev(), b"PLAY").is_empty(),
            "case matters for key names"
        );
    }

    #[test]
    fn binary_names_are_stored_and_matched_as_bytes() {
        let mut s = ObservationStore::new(100);
        s.record(Observation {
            name: vec![0xff, 0x00, b'a'],
            scope: dev(),
            origin: Origin::RetainedResult,
            parent_key: None,
        });
        assert_eq!(s.keys_for(&dev(), &[0xff]).len(), 1);
        assert!(s.keys_for(&dev(), &[0xfe]).is_empty());
    }

    #[test]
    fn a_schema_hint_does_not_claim_the_name_exists() {
        // §13.10: a configured field is a team convention, not evidence.
        assert!(!Origin::SchemaHint.asserts_existence());
        assert!(Origin::RetainedResult.asserts_existence());
        assert!(Origin::Discovery.asserts_existence());
        assert!(
            !Origin::History.asserts_existence(),
            "history is a memory, not a fact"
        );
        assert_eq!(Origin::SchemaHint.label(), "schema suggestion");
    }

    #[test]
    fn the_store_is_bounded_and_evicts_oldest_first() {
        // §12.5: the observation index cannot grow without limit.
        let mut s = ObservationStore::new(3);
        for i in 0..10 {
            s.record(key(&format!("k{i}"), dev()));
        }
        assert_eq!(s.len(), 3);
        let names: Vec<String> = s
            .keys_for(&dev(), b"")
            .iter()
            .map(|o| String::from_utf8_lossy(&o.name).into_owned())
            .collect();
        assert_eq!(names, vec!["k7", "k8", "k9"]);
    }

    #[test]
    fn recording_the_same_name_twice_does_not_grow_the_store() {
        let mut s = ObservationStore::new(10);
        for _ in 0..5 {
            s.record(key("player:1", dev()));
        }
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn the_same_name_in_two_scopes_is_two_observations() {
        let mut s = ObservationStore::new(10);
        s.record(key("shared", dev()));
        s.record(key("shared", prod()));
        assert_eq!(s.len(), 2, "they must not merge");
        assert_eq!(s.keys_for(&dev(), b"").len(), 1);
        assert_eq!(s.keys_for(&prod(), b"").len(), 1);
    }

    #[test]
    fn clear_empties_everything() {
        let mut s = ObservationStore::new(10);
        s.record(key("a", dev()));
        s.record(key("b", prod()));
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn the_scope_string_names_every_component() {
        // The config value in Appendix A must list all six; R13 added topology.
        let s = ObservationScope {
            auth_epoch: 7,
            policy_epoch: 2,
            topology_epoch: 4,
            ..dev()
        };
        let text = s.to_string();
        for part in ["uuid-dev", "svc-dev", "db3", "a7", "p2", "t4"] {
            assert!(text.contains(part), "scope string missing {part}: {text}");
        }
    }
}
