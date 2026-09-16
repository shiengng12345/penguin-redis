//! Bounded result retention (v2.1 §24.3, §24.4, §8.2, ADR-009, ADR-012, R31, V-E03).
//!
//! Two separate promises the review forced apart:
//! - **retention** is bounded (64 MiB total, 16 MiB per result). When a result is evicted the
//!   answer to `:inspect` is `not retained` — never a silent re-fetch and never a pretence
//!   that it is still there (UX-10, ADR-012)
//! - **output** is not retention. A reply larger than the budget can still be printed in
//!   full; the cache limit must not silently become an output truncation (§24.3)
//!
//! And the ordering §8.2 fixes, because v2.0 left it ambiguous: a prompt is only possible
//! *before the first byte is written*, so it may only be offered when the length is known up
//! front. Once streaming has begun the answer is to keep streaming and mark what was kept.

use pr_core::ResponseHandle;
use std::collections::HashMap;

/// Retention budgets (§24.3).
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Total bytes retained across all results.
    pub total_bytes: u64,
    /// Largest single result kept in memory.
    pub single_bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            total_bytes: 64 * 1024 * 1024,
            single_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Why a result is not available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// Never stored: it exceeded the single-result budget.
    TooLarge,
    /// Stored, then evicted to make room. The user is told, and offered a re-read.
    Evicted,
    /// No such id.
    Unknown,
    /// Belongs to a scope that has since been invalidated (§10.11).
    ScopeExpired,
}

/// What a retained result carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retained {
    /// Handle it was stored under.
    pub handle: ResponseHandle,
    /// The bytes.
    pub bytes: Vec<u8>,
    /// The scope epoch it was captured in.
    pub scope_epoch: u64,
}

/// What to do about a reply that will not fit (§8.2 ordering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OversizeAction {
    /// The declared length is known before the first byte, and this is an interactive TTY:
    /// the user can still be asked.
    Prompt {
        /// Declared size.
        declared: u64,
    },
    /// Length unknown, or output already begun: stream it and mark what was kept. Never
    /// interrupt a half-written reply with a question.
    StreamAndMark,
    /// Non-interactive: never prompt. Write it out or fail with a non-zero status.
    StreamOnly,
}

/// Context for the oversize decision.
#[derive(Clone, Copy, Debug)]
pub struct OutputContext {
    /// Whether stdout is a terminal.
    pub interactive: bool,
    /// Declared length, when the protocol gave one up front.
    pub declared_len: Option<u64>,
    /// Whether any byte of this reply has already been written.
    pub output_started: bool,
}

/// Decide how to handle a reply that exceeds the retention budget.
///
/// This encodes R31: a prompt is only reachable when nothing has been printed and the size
/// is known, because a question cannot be asked halfway through a table.
#[must_use]
pub fn oversize_action(ctx: OutputContext, budget: Budget) -> OversizeAction {
    if !ctx.interactive {
        return OversizeAction::StreamOnly;
    }
    match ctx.declared_len {
        Some(n) if !ctx.output_started && n > budget.single_bytes => {
            OversizeAction::Prompt { declared: n }
        }
        _ => OversizeAction::StreamAndMark,
    }
}

/// A bounded, scope-aware result store.
#[derive(Debug)]
pub struct ResultStore {
    budget: Budget,
    /// Insertion order, oldest first — the eviction order.
    order: Vec<u64>,
    items: HashMap<u64, Retained>,
    /// Results that were stored and then evicted, so we can say so rather than say "unknown".
    evicted: Vec<u64>,
    /// Results refused for being too large.
    too_large: Vec<u64>,
    used: u64,
    next_id: u64,
}

impl Default for ResultStore {
    fn default() -> Self {
        Self::new(Budget::default())
    }
}

impl ResultStore {
    /// A store with the given budget.
    #[must_use]
    pub fn new(budget: Budget) -> Self {
        Self {
            budget,
            order: Vec::new(),
            items: HashMap::new(),
            evicted: Vec::new(),
            too_large: Vec::new(),
            used: 0,
            next_id: 1,
        }
    }

    /// Bytes currently retained.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        self.used
    }

    /// Number of retained results.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Store a result, evicting older ones if necessary.
    ///
    /// Returns the handle. A result larger than the single-result budget is **not** stored;
    /// the handle is still returned so the command can be referred to, and looking it up
    /// reports [`Unavailable::TooLarge`] rather than pretending it was kept.
    pub fn put(&mut self, bytes: Vec<u8>, scope_epoch: u64) -> ResponseHandle {
        let id = self.next_id;
        self.next_id += 1;
        let size = bytes.len() as u64;

        if size > self.budget.single_bytes {
            self.too_large.push(id);
            return ResponseHandle(id);
        }
        while self.used + size > self.budget.total_bytes && !self.order.is_empty() {
            let oldest = self.order.remove(0);
            if let Some(old) = self.items.remove(&oldest) {
                self.used -= old.bytes.len() as u64;
                self.evicted.push(oldest);
            }
        }
        // If it still does not fit, the budget is smaller than this one result.
        if self.used + size > self.budget.total_bytes {
            self.too_large.push(id);
            return ResponseHandle(id);
        }
        self.used += size;
        self.order.push(id);
        self.items.insert(
            id,
            Retained {
                handle: ResponseHandle(id),
                bytes,
                scope_epoch,
            },
        );
        ResponseHandle(id)
    }

    /// Look a result up.
    ///
    /// # Errors
    /// [`Unavailable`] saying *why*, which is the part UX-10 cares about: "not retained" and
    /// "never existed" are different answers and the user is owed the right one.
    pub fn get(
        &self,
        handle: ResponseHandle,
        current_scope: u64,
    ) -> Result<&Retained, Unavailable> {
        if let Some(r) = self.items.get(&handle.0) {
            if r.scope_epoch != current_scope {
                // §10.11: switching connection, DB or identity invalidates the context.
                return Err(Unavailable::ScopeExpired);
            }
            return Ok(r);
        }
        if self.evicted.contains(&handle.0) {
            return Err(Unavailable::Evicted);
        }
        if self.too_large.contains(&handle.0) {
            return Err(Unavailable::TooLarge);
        }
        Err(Unavailable::Unknown)
    }

    /// Drop everything captured in an older scope (§10.11).
    pub fn invalidate_scope(&mut self, keep_epoch: u64) -> usize {
        let doomed: Vec<u64> = self
            .items
            .iter()
            .filter(|(_, r)| r.scope_epoch != keep_epoch)
            .map(|(id, _)| *id)
            .collect();
        for id in &doomed {
            if let Some(r) = self.items.remove(id) {
                self.used -= r.bytes.len() as u64;
            }
            self.order.retain(|o| o != id);
            self.evicted.push(*id);
        }
        doomed.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn small_budget() -> Budget {
        Budget {
            total_bytes: 1000,
            single_bytes: 400,
        }
    }

    #[test]
    fn a_stored_result_comes_back() {
        let mut s = ResultStore::new(small_budget());
        let h = s.put(b"hello".to_vec(), 0);
        assert_eq!(s.get(h, 0).unwrap().bytes, b"hello");
        assert_eq!(s.used_bytes(), 5);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn eviction_says_evicted_not_unknown() {
        // UX-10: "not retained" and "never existed" are different answers.
        let mut s = ResultStore::new(small_budget());
        let first = s.put(vec![0u8; 300], 0);
        let _second = s.put(vec![0u8; 300], 0);
        let _third = s.put(vec![0u8; 300], 0);
        // 900 fits; adding another 300 forces the oldest out.
        let _fourth = s.put(vec![0u8; 300], 0);
        assert_eq!(s.get(first, 0), Err(Unavailable::Evicted));
        assert_ne!(s.get(first, 0), Err(Unavailable::Unknown));
    }

    #[test]
    fn an_unknown_handle_is_unknown() {
        let s = ResultStore::new(small_budget());
        assert_eq!(s.get(ResponseHandle(99), 0), Err(Unavailable::Unknown));
    }

    #[test]
    fn a_result_over_the_single_budget_is_never_stored_and_says_so() {
        let mut s = ResultStore::new(small_budget());
        let h = s.put(vec![0u8; 500], 0);
        assert_eq!(s.get(h, 0), Err(Unavailable::TooLarge));
        assert_eq!(
            s.used_bytes(),
            0,
            "an over-budget result must not consume the cache"
        );
        assert!(s.is_empty());
    }

    #[test]
    fn eviction_is_oldest_first() {
        let mut s = ResultStore::new(Budget {
            total_bytes: 250,
            single_bytes: 200,
        });
        let a = s.put(vec![0u8; 100], 0);
        let b = s.put(vec![0u8; 100], 0);
        let c = s.put(vec![0u8; 100], 0);
        assert_eq!(s.get(a, 0), Err(Unavailable::Evicted));
        assert!(s.get(b, 0).is_ok(), "b is newer than a and should survive");
        assert!(s.get(c, 0).is_ok());
    }

    #[test]
    fn the_budget_is_never_exceeded() {
        let mut s = ResultStore::new(small_budget());
        for _ in 0..50 {
            s.put(vec![0u8; 250], 0);
            assert!(
                s.used_bytes() <= small_budget().total_bytes,
                "used {} exceeded budget",
                s.used_bytes()
            );
        }
    }

    #[test]
    fn a_scope_change_expires_retained_results() {
        // §10.11 / ASSIST-030: a result captured on dev must not be readable after switching.
        let mut s = ResultStore::new(small_budget());
        let h = s.put(b"dev data".to_vec(), 1);
        assert!(s.get(h, 1).is_ok());
        assert_eq!(s.get(h, 2), Err(Unavailable::ScopeExpired));
    }

    #[test]
    fn invalidating_a_scope_frees_its_bytes_and_reports_evicted() {
        let mut s = ResultStore::new(small_budget());
        let old = s.put(vec![0u8; 200], 1);
        let new = s.put(vec![0u8; 200], 2);
        assert_eq!(s.invalidate_scope(2), 1);
        assert_eq!(s.used_bytes(), 200, "the old result's bytes are released");
        assert_eq!(s.get(old, 2), Err(Unavailable::Evicted));
        assert!(s.get(new, 2).is_ok());
    }

    // ---------------------------------------------------------------- §8.2 ordering (R31)
    #[test]
    fn a_prompt_is_only_offered_before_the_first_byte() {
        // The ambiguity R31 resolved: you cannot ask a question halfway through a table.
        let b = Budget::default();
        let known_before = OutputContext {
            interactive: true,
            declared_len: Some(b.single_bytes + 1),
            output_started: false,
        };
        assert_eq!(
            oversize_action(known_before, b),
            OversizeAction::Prompt {
                declared: b.single_bytes + 1
            }
        );

        let already_writing = OutputContext {
            output_started: true,
            ..known_before
        };
        assert_eq!(
            oversize_action(already_writing, b),
            OversizeAction::StreamAndMark
        );
    }

    #[test]
    fn an_unknown_length_never_prompts() {
        // An aggregate has no declared total, so the size is only known once it is too late.
        let b = Budget::default();
        let ctx = OutputContext {
            interactive: true,
            declared_len: None,
            output_started: false,
        };
        assert_eq!(oversize_action(ctx, b), OversizeAction::StreamAndMark);
    }

    #[test]
    fn a_non_interactive_sink_is_never_prompted() {
        // §24.4: a script must not stop on a menu.
        let b = Budget::default();
        for declared in [None, Some(b.single_bytes + 1)] {
            for started in [false, true] {
                let ctx = OutputContext {
                    interactive: false,
                    declared_len: declared,
                    output_started: started,
                };
                assert_eq!(
                    oversize_action(ctx, b),
                    OversizeAction::StreamOnly,
                    "declared={declared:?} started={started}"
                );
            }
        }
    }

    #[test]
    fn a_reply_within_budget_needs_no_special_handling() {
        let b = Budget::default();
        let ctx = OutputContext {
            interactive: true,
            declared_len: Some(1024),
            output_started: false,
        };
        assert_eq!(oversize_action(ctx, b), OversizeAction::StreamAndMark);
    }

    #[test]
    fn the_cache_limit_is_not_an_output_limit() {
        // §24.3: a reply bigger than the cache can still be printed in full; it simply is not
        // retained. The store models exactly that — the bytes are refused, not truncated.
        let mut s = ResultStore::new(small_budget());
        let big = vec![7u8; 5000];
        let h = s.put(big.clone(), 0);
        assert_eq!(s.get(h, 0), Err(Unavailable::TooLarge));
        // The caller still holds the full reply and may write all of it.
        assert_eq!(big.len(), 5000);
    }

    #[test]
    fn spec_defaults_match_the_document() {
        let b = Budget::default();
        assert_eq!(b.total_bytes, 64 * 1024 * 1024);
        assert_eq!(b.single_bytes, 16 * 1024 * 1024);
    }
}
