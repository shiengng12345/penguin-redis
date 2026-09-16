//! The frozen assistance contracts (v2.1 §11.11, §12.9, §31.4, V-J02).
//!
//! Three of the twelve types V-J02 freezes live here: [`CompletionRequest`],
//! [`CompletionCandidate`] and [`AssistanceSnapshot`].
//!
//! They are separate types from the ones the broker uses internally on purpose. The broker's
//! [`crate::broker::Candidate`] is an implementation detail that will change; this is what
//! `pr-repl`, `pr-tui` and `pr-mcp` compile against, and §31.4 points the dependency arrow
//! *into* it.

use crate::scope::ObservationScope;

/// The version of the contracts this crate owns.
///
/// ```
/// assert_eq!(pr_intelligence::contract::SCHEMA_VERSION, 1);
/// ```
pub const SCHEMA_VERSION: u32 = 1;

/// One request for candidates (v2.1 §11.11, §12.9).
///
/// Carries the **buffer revision** and the **observation scope**, because a completion that
/// arrives after the buffer moved on, or from a different target, must be discarded rather
/// than shown — §12.8 and ADR-017. Putting both in the request rather than in ambient state is
/// what makes that checkable at the point of delivery.
///
/// ```
/// use pr_intelligence::contract::{CompletionRequest, Trigger};
/// use pr_intelligence::ObservationScope;
///
/// let scope = ObservationScope::new("profile-uuid", "service-hash", 0);
/// let req = CompletionRequest {
///     buffer: b"HGET player:10001 no".to_vec(),
///     cursor_bytes: 20,
///     buffer_revision: 42,
///     scope: scope.clone(),
///     trigger: Trigger::Typing,
///     max_candidates: 128,
/// };
/// assert_eq!(req.cursor_bytes, 20);
/// // A result for revision 41 is stale by construction, not by convention.
/// assert!(req.buffer_revision > 41);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    /// The whole input buffer, exact bytes. Not a string: §3.2 allows bytes that are not UTF-8.
    pub buffer: Vec<u8>,
    /// Cursor position as a **byte** offset into `buffer`.
    ///
    /// Bytes rather than characters because the buffer is bytes; a character offset would need
    /// a decode that may not be possible.
    pub cursor_bytes: usize,
    /// Which revision of the buffer this asks about (§12.8, ASSIST-026).
    pub buffer_revision: u64,
    /// Target, identity, DB and epochs (ADR-017). A candidate from another scope is a leak.
    pub scope: ObservationScope,
    /// What asked for candidates. §14 treats an automatic popup and a Tab differently.
    pub trigger: Trigger,
    /// §12.5's per-request ceiling. Sorting happens before the viewport is taken; the whole
    /// keyspace is never materialised.
    pub max_candidates: usize,
}

/// What caused a completion request (v2.1 §14).
///
/// ```
/// use pr_intelligence::contract::Trigger;
/// // Typing waits out the debounce; an explicit Tab does not (§12.5).
/// assert!(Trigger::Typing.debounces());
/// assert!(!Trigger::Explicit.debounces());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A keystroke. Subject to §12.5's 75 ms debounce.
    Typing,
    /// Tab, F6 or the palette. Immediate.
    Explicit,
    /// A late result being re-evaluated against the current buffer.
    Revalidation,
}

impl Trigger {
    /// Whether this trigger waits out the automatic debounce.
    #[must_use]
    pub fn debounces(self) -> bool {
        matches!(self, Self::Typing)
    }
}

/// One candidate, as the editor receives it (v2.1 §10.3, §11.10, ASSIST-019/028/083).
///
/// `insert` and `display` are separate fields, not one string. A key whose name contains a
/// control byte must **display** escaped and **insert** raw; collapsing them would mean either
/// inserting an escape sequence into the buffer or writing a raw control byte to the terminal,
/// and §23.6 forbids the second.
///
/// ```
/// use pr_intelligence::contract::{CompletionCandidate, Provenance};
///
/// let c = CompletionCandidate {
///     id: "key:player:10001".into(),
///     insert: b"player:10001".to_vec(),
///     display: "player:10001".into(),
///     provenance: Provenance::RetainedResult,
///     detail: String::new(),
/// };
/// assert_eq!(c.insert, c.display.as_bytes());
///
/// // A name that is not printable inserts its bytes and displays an escape of them.
/// let odd = CompletionCandidate {
///     id: "key:odd".into(),
///     insert: vec![b'a', 0x07, b'b'],
///     display: "a\\x07b".into(),
///     provenance: Provenance::RetainedResult,
///     detail: String::new(),
/// };
/// assert_ne!(odd.insert, odd.display.as_bytes());
/// assert!(odd.provenance.asserts_existence());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Stable identity across re-sorts, so the focused row cannot move onto a different entry.
    pub id: String,
    /// Exact bytes inserted into the buffer when accepted.
    pub insert: Vec<u8>,
    /// What the user sees. May be an escaped rendering of `insert`.
    pub display: String,
    /// Where this came from. §10.3 requires it to be visible: a name observed in a result is
    /// evidence the key existed then, and a name from the catalog is not evidence of anything.
    pub provenance: Provenance,
    /// One line of explanation, already safe to display.
    pub detail: String,
}

/// Where a candidate came from (v2.1 §10.3).
///
/// ```
/// use pr_intelligence::contract::Provenance;
/// // Only names actually seen on this target assert that the thing exists.
/// assert!(Provenance::RetainedResult.asserts_existence());
/// assert!(Provenance::Discovery.asserts_existence());
/// assert!(!Provenance::Catalog.asserts_existence());
/// assert!(!Provenance::History.asserts_existence());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// From the local signed catalog: a command or option name.
    Catalog,
    /// Seen in a result retained from this target and scope.
    RetainedResult,
    /// Found by an explicit discovery run.
    Discovery,
    /// Typed before, on this target.
    History,
    /// Written by the user in a profile or snippet.
    UserDefined,
}

impl Provenance {
    /// Whether this source is evidence the named thing exists on the server.
    ///
    /// The distinction §10.3 insists on: a catalog name means "this command is spelled that
    /// way", not "this key is there".
    #[must_use]
    pub fn asserts_existence(self) -> bool {
        matches!(self, Self::RetainedResult | Self::Discovery)
    }
}

/// Everything the UI needs to draw the assistance state at one instant (v2.1 §12.9).
///
/// A snapshot rather than a stream of edits: the TUI and the CLI render the same thing, and a
/// half-applied edit is a class of bug neither should be able to have.
///
/// ```
/// use pr_intelligence::contract::{AssistanceSnapshot, Completeness};
///
/// let s = AssistanceSnapshot {
///     buffer_revision: 42,
///     candidates: Vec::new(),
///     focused: None,
///     completeness: Completeness::Complete,
///     discarded: 3,
///     note: String::new(),
/// };
/// // Nothing focused means Enter submits the buffer, never a candidate (ADR-016).
/// assert!(s.focused.is_none());
/// assert_eq!(s.discarded, 3);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistanceSnapshot {
    /// The buffer revision these candidates were computed for.
    pub buffer_revision: u64,
    /// Candidates, already ordered and truncated to the viewport.
    pub candidates: Vec<CompletionCandidate>,
    /// Index into `candidates`, or `None`.
    ///
    /// `None` is the default and it matters: ADR-016 says Enter never "completes and sends",
    /// so a snapshot with no focus is the only state in which Enter may submit.
    pub focused: Option<usize>,
    /// Whether the candidate list is everything there is.
    pub completeness: Completeness,
    /// How many late results were discarded, shown in `:assist status` and never as an error.
    pub discarded: u64,
    /// One line for the user, already safe to display.
    pub note: String,
}

/// Whether a candidate list is everything (v2.1 §12.4).
///
/// ```
/// use pr_intelligence::contract::Completeness;
/// assert!(Completeness::Complete.is_conclusive());
/// assert!(!Completeness::BudgetReached.is_conclusive());
/// // An empty list from an incomplete search never says the thing is absent.
/// assert!(!Completeness::BudgetReached.empty_message().contains("does not exist"));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completeness {
    /// Everything that matches is here.
    Complete,
    /// A discovery budget stopped the search.
    BudgetReached,
    /// The user cancelled.
    Cancelled,
    /// The server refused to search.
    Denied,
}

impl Completeness {
    /// Whether an empty list means there is nothing to find.
    #[must_use]
    pub fn is_conclusive(self) -> bool {
        matches!(self, Self::Complete)
    }

    /// The line shown beside an empty list.
    #[must_use]
    pub fn empty_message(self) -> &'static str {
        match self {
            Self::Complete => "No matches.",
            Self::BudgetReached => {
                "No matches in the observed portion; discovery incomplete (budget reached)"
            }
            Self::Cancelled => {
                "No matches in the observed portion; discovery incomplete (cancelled)"
            }
            Self::Denied => {
                "The server refused the search; nothing can be concluded about what exists"
            }
        }
    }
}

/// The contracts this crate owns, for the V-J02 test to walk.
pub const OWNED: &[&str] = &[
    "CompletionRequest",
    "CompletionCandidate",
    "AssistanceSnapshot",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_display_are_separate_because_they_have_to_be() {
        // ASSIST-019/083: a binary name inserts raw and displays escaped. One field could not
        // do both, and whichever it chose would be a bug -- an escape sequence in the buffer,
        // or a raw control byte on the terminal.
        let c = CompletionCandidate {
            id: "k".into(),
            insert: vec![0x1b, b'[', b'2', b'J'],
            display: "\\x1b[2J".into(),
            provenance: Provenance::RetainedResult,
            detail: String::new(),
        };
        assert!(c.insert.iter().any(|b| *b < 0x20));
        assert!(!c.display.chars().any(|ch| (ch as u32) < 0x20));
    }

    #[test]
    fn only_observed_names_assert_that_something_exists() {
        for p in [
            Provenance::Catalog,
            Provenance::History,
            Provenance::UserDefined,
        ] {
            assert!(
                !p.asserts_existence(),
                "{p:?} must not imply the key is there"
            );
        }
        for p in [Provenance::RetainedResult, Provenance::Discovery] {
            assert!(p.asserts_existence());
        }
    }

    #[test]
    fn an_incomplete_search_never_claims_absence() {
        for c in [
            Completeness::BudgetReached,
            Completeness::Cancelled,
            Completeness::Denied,
        ] {
            let m = c.empty_message().to_lowercase();
            assert!(!c.is_conclusive());
            assert!(
                !m.contains("does not exist") && !m.contains("not found"),
                "{c:?}: {m}"
            );
        }
    }
}
