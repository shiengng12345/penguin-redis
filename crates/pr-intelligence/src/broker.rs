//! Async candidate broker (v2.1 §12.8, ADR-017, V-F05).
//!
//! Suggestions arrive out of band: the user keeps typing while a lookup is in flight. §12.8
//! says a result that no longer matches the buffer revision or the target epochs must be
//! **discarded**, not applied — and the failure it prevents is precise: a stale candidate
//! landing on text the user has since retyped, or a dev name appearing after a switch to
//! prod (ASSIST-026 to ASSIST-033).
//!
//! The rule is checked on *arrival*, not on request, because everything can change between
//! the two.

use crate::scope::ObservationScope;

/// The stamp a request carries and a result must still match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// Editor buffer revision at request time.
    pub buffer_revision: u64,
    /// Scope at request time.
    pub scope: ObservationScope,
}

/// A candidate offered to the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Stable identity, so a re-sort cannot move the focused row onto a different entry
    /// (§11.10, ASSIST-028).
    pub id: String,
    /// Text inserted when accepted.
    pub insert: Vec<u8>,
    /// Text displayed. May differ: a binary name shows escaped but inserts raw
    /// (ASSIST-019/083).
    pub display: String,
}

/// Why a late result was dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// The buffer moved on.
    StaleRevision,
    /// The target, identity, DB, policy or topology changed.
    ScopeChanged,
    /// A newer request for the same slot has already been issued.
    Superseded,
}

/// What the broker decided about an arriving result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Accepted {
    /// Use these candidates.
    Use(Vec<Candidate>),
    /// Discard, with the reason (shown in `:assist status`, never as an error).
    Discard(Dropped),
}

/// Tracks in-flight requests and admits only results that still apply.
#[derive(Debug, Default)]
pub struct Broker {
    current: Option<Stamp>,
    /// Sequence of the most recent request; anything older is superseded.
    issued: u64,
    /// Candidates currently shown.
    shown: Vec<Candidate>,
    /// Index of the focused candidate, if the menu has focus.
    focused: Option<usize>,
    /// Count of results discarded, for `:assist status`.
    discarded: usize,
}

impl Broker {
    /// A broker with nothing in flight.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Note that the editor state changed. Any in-flight result is now stale.
    pub fn set_state(&mut self, stamp: Stamp) {
        self.current = Some(stamp);
    }

    /// Issue a request, returning the sequence number to quote when the result arrives.
    pub fn issue(&mut self) -> u64 {
        self.issued += 1;
        self.issued
    }

    /// Number of results discarded so far.
    #[must_use]
    pub fn discarded(&self) -> usize {
        self.discarded
    }

    /// Candidates currently displayed.
    #[must_use]
    pub fn shown(&self) -> &[Candidate] {
        &self.shown
    }

    /// The focused candidate, if the menu has focus.
    #[must_use]
    pub fn focused(&self) -> Option<&Candidate> {
        self.focused.and_then(|i| self.shown.get(i))
    }

    /// Move focus into the menu, or within it.
    pub fn focus(&mut self, index: usize) {
        if index < self.shown.len() {
            self.focused = Some(index);
        }
    }

    /// Leave the menu.
    pub fn unfocus(&mut self) {
        self.focused = None;
    }

    /// Offer a result that was computed against `stamp` and issued as `seq`.
    ///
    /// Returns what the editor should do. The checks are deliberately in this order: a scope
    /// change is reported even if the revision also moved, because that is the one the user
    /// most needs to understand.
    pub fn deliver(&mut self, seq: u64, stamp: &Stamp, candidates: Vec<Candidate>) -> Accepted {
        let Some(current) = &self.current else {
            self.discarded += 1;
            return Accepted::Discard(Dropped::ScopeChanged);
        };
        if stamp.scope != current.scope {
            self.discarded += 1;
            return Accepted::Discard(Dropped::ScopeChanged);
        }
        if stamp.buffer_revision != current.buffer_revision {
            self.discarded += 1;
            return Accepted::Discard(Dropped::StaleRevision);
        }
        if seq < self.issued {
            self.discarded += 1;
            return Accepted::Discard(Dropped::Superseded);
        }
        self.merge(candidates.clone());
        Accepted::Use(candidates)
    }

    /// Replace the shown list, keeping focus on the same candidate **id** rather than the
    /// same index, so an async re-sort cannot move focus onto a different entry
    /// (ASSIST-028).
    fn merge(&mut self, candidates: Vec<Candidate>) {
        let focused_id = self.focused().map(|c| c.id.clone());
        self.shown = candidates;
        self.focused = focused_id.and_then(|id| self.shown.iter().position(|c| c.id == id));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn scope(db: u32) -> ObservationScope {
        ObservationScope::new("uuid-dev", "svc", db)
    }

    fn stamp(rev: u64, db: u32) -> Stamp {
        Stamp {
            buffer_revision: rev,
            scope: scope(db),
        }
    }

    fn cand(id: &str) -> Candidate {
        Candidate {
            id: id.to_owned(),
            insert: id.as_bytes().to_vec(),
            display: id.to_owned(),
        }
    }

    #[test]
    fn a_result_matching_the_current_state_is_used() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        let out = b.deliver(seq, &stamp(1, 0), vec![cand("HGET")]);
        assert_eq!(out, Accepted::Use(vec![cand("HGET")]));
        assert_eq!(b.shown().len(), 1);
        assert_eq!(b.discarded(), 0);
    }

    #[test]
    fn a_stale_revision_is_discarded() {
        // ASSIST-026: the user kept typing while the lookup was in flight.
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.set_state(stamp(2, 0)); // another keystroke
        let out = b.deliver(seq, &stamp(1, 0), vec![cand("HGET")]);
        assert_eq!(out, Accepted::Discard(Dropped::StaleRevision));
        assert!(
            b.shown().is_empty(),
            "nothing must be shown from a stale result"
        );
        assert_eq!(b.discarded(), 1);
    }

    #[test]
    fn a_scope_change_discards_even_at_the_same_revision() {
        // ASSIST-029/030: switching DB or target must not let old names through.
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.set_state(stamp(1, 3)); // SELECT 3
        let out = b.deliver(seq, &stamp(1, 0), vec![cand("dev:key")]);
        assert_eq!(out, Accepted::Discard(Dropped::ScopeChanged));
        assert!(b.shown().is_empty());
    }

    #[test]
    fn a_scope_change_is_reported_in_preference_to_a_stale_revision() {
        // When both moved, the scope change is the one the user needs to understand.
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.set_state(stamp(5, 3));
        assert_eq!(
            b.deliver(seq, &stamp(1, 0), vec![cand("x")]),
            Accepted::Discard(Dropped::ScopeChanged)
        );
    }

    #[test]
    fn an_older_request_is_superseded_by_a_newer_one() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let first = b.issue();
        let second = b.issue();
        // The newer one lands first.
        assert!(matches!(
            b.deliver(second, &stamp(1, 0), vec![cand("B")]),
            Accepted::Use(_)
        ));
        // The older one arrives late and must not overwrite it.
        assert_eq!(
            b.deliver(first, &stamp(1, 0), vec![cand("A")]),
            Accepted::Discard(Dropped::Superseded)
        );
        assert_eq!(b.shown()[0].id, "B", "the newer result must stand");
    }

    #[test]
    fn a_result_arriving_before_any_state_is_discarded() {
        let mut b = Broker::new();
        let out = b.deliver(1, &stamp(1, 0), vec![cand("x")]);
        assert_eq!(out, Accepted::Discard(Dropped::ScopeChanged));
    }

    #[test]
    fn focus_follows_the_candidate_id_not_the_index() {
        // ASSIST-028: an async append must not move focus onto a different entry, because
        // the user may be about to press Enter.
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.deliver(seq, &stamp(1, 0), vec![cand("HGET"), cand("HGETALL")]);
        b.focus(1);
        assert_eq!(b.focused().unwrap().id, "HGETALL");

        // A later result inserts an entry above the focused one.
        let seq2 = b.issue();
        b.deliver(
            seq2,
            &stamp(1, 0),
            vec![cand("HDEL"), cand("HGET"), cand("HGETALL")],
        );
        assert_eq!(
            b.focused().unwrap().id,
            "HGETALL",
            "focus must still be on the same command, not whatever is now at index 1"
        );
    }

    #[test]
    fn focus_is_dropped_when_its_candidate_disappears() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.deliver(seq, &stamp(1, 0), vec![cand("HGET"), cand("HGETALL")]);
        b.focus(1);
        let seq2 = b.issue();
        b.deliver(seq2, &stamp(1, 0), vec![cand("HGET")]);
        assert!(
            b.focused().is_none(),
            "focus must not silently land elsewhere"
        );
    }

    #[test]
    fn unfocus_clears_the_selection() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.deliver(seq, &stamp(1, 0), vec![cand("A")]);
        b.focus(0);
        assert!(b.focused().is_some());
        b.unfocus();
        assert!(b.focused().is_none());
    }

    #[test]
    fn focusing_out_of_range_is_ignored() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let seq = b.issue();
        b.deliver(seq, &stamp(1, 0), vec![cand("A")]);
        b.focus(99);
        assert!(b.focused().is_none());
    }

    #[test]
    fn display_and_insert_may_differ() {
        // ASSIST-019/083: a binary name is shown escaped but inserted as an escape sequence
        // that reproduces the exact bytes — never as the truncated display text.
        let c = Candidate {
            id: "bin".into(),
            insert: br#""\xff\x00""#.to_vec(),
            display: "\\xff\\x00".into(),
        };
        assert_ne!(c.insert, c.display.as_bytes());
        assert!(
            !c.display.contains('\u{0}'),
            "the display form carries no raw bytes"
        );
    }

    #[test]
    fn many_late_results_are_all_counted_and_none_applied() {
        let mut b = Broker::new();
        b.set_state(stamp(1, 0));
        let stale: Vec<u64> = (0..20).map(|_| b.issue()).collect();
        b.set_state(stamp(99, 0));
        for s in stale {
            assert!(matches!(
                b.deliver(s, &stamp(1, 0), vec![cand("x")]),
                Accepted::Discard(_)
            ));
        }
        assert_eq!(b.discarded(), 20);
        assert!(b.shown().is_empty());
    }
}
