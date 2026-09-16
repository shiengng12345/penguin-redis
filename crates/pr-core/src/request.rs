//! `CommandRequest` — the only way a command reaches the kernel (v2.1 §19.2, ADR-008).
//!
//! Every entry point (REPL, one-shot, TUI, recipe, MCP, `--pipe`) constructs one of these and
//! hands it to the kernel. Argument boundaries and raw bytes are preserved exactly; there is
//! no `String` anywhere on this path (ADR-005, ADR-027).

use crate::SafeText;
use bytes::Bytes;

/// Who created this request. Policy is stricter for non-human origins (v2.1 §23.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestOrigin {
    /// Typed by a person at the REPL or as a one-shot argv.
    User,
    /// Issued by a TUI action the user selected.
    Tui,
    /// A step in a recipe.
    Recipe,
    /// An agent over MCP — most restricted.
    Agent,
    /// A frame decoded from `--pipe` stdin (ADR-025).
    Pipe,
    /// Metadata/discovery issued by the application on the user's explicit request.
    Discovery,
}

impl RequestOrigin {
    /// Origins that may never run destructive or unclassified commands without an explicit,
    /// human-signed approval (v2.1 §23.1, §29.4).
    #[must_use]
    pub fn requires_human_approval_for_writes(self) -> bool {
        matches!(self, Self::Agent | Self::Pipe)
    }
}

/// Effect classification, owned by the local signed catalog (ADR-030).
///
/// A server may *add* availability information but may never downgrade these (V-D07).
// A flag set, not a state machine: each bool is an independent catalog-assigned property,
// and collapsing them into an enum would lose commands that are several at once.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Effects {
    /// Reads data.
    pub reads_data: bool,
    /// Writes data.
    pub writes_data: bool,
    /// Consumes/advances state (e.g. `XREADGROUP`).
    pub consumes: bool,
    /// May block the connection.
    pub blocks: bool,
    /// Administrative.
    pub admin: bool,
    /// Destructive at database or instance scope (`FLUSHDB`, `FLUSHALL`).
    pub destructive: bool,
    /// The catalog does not classify this command. Treated as maximum risk outside dev.
    pub unknown: bool,
}

impl Effects {
    /// An unclassified command.
    #[must_use]
    pub fn unknown() -> Self {
        Self { unknown: true, ..Self::default() }
    }
    /// A pure read.
    #[must_use]
    pub fn read() -> Self {
        Self { reads_data: true, ..Self::default() }
    }
    /// A write.
    #[must_use]
    pub fn write() -> Self {
        Self { writes_data: true, ..Self::default() }
    }
    /// True if this may change server state in any way.
    #[must_use]
    pub fn mutates(self) -> bool {
        self.writes_data || self.consumes || self.admin || self.destructive || self.unknown
    }
}

/// Per-request resource budget (v2.1 §24.3, ADR-029 — always process-scoped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestBudget {
    /// Maximum bytes retained in memory for the response.
    pub max_retained_bytes: u64,
    /// Maximum wall-clock time before the request is abandoned.
    pub timeout_ms: u64,
    /// Maximum aggregate nesting depth accepted from the decoder.
    pub max_depth: usize,
}

impl Default for RequestBudget {
    fn default() -> Self {
        // v2.1 §24.3: 16 MiB single-result retention; §12.5 depth guard.
        Self { max_retained_bytes: 16 * 1024 * 1024, timeout_ms: 30_000, max_depth: 64 }
    }
}

/// A command about to be executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRequest {
    /// Exact argv bytes, one entry per argument, boundaries preserved.
    pub args: Vec<Bytes>,
    /// Who asked.
    pub origin: RequestOrigin,
    /// Catalog-assigned effects.
    pub effects: Effects,
    /// Resource budget.
    pub budget: RequestBudget,
}

impl CommandRequest {
    /// Build a request from argv.
    #[must_use]
    pub fn new(args: Vec<Bytes>, origin: RequestOrigin, effects: Effects) -> Self {
        Self { args, origin, effects, budget: RequestBudget::default() }
    }

    /// Uppercased command name (first argument) as a `String`, for catalog lookup.
    /// Returns `None` for an empty argv or a non-UTF-8 command name.
    #[must_use]
    pub fn command_name(&self) -> Option<String> {
        let first = self.args.first()?;
        std::str::from_utf8(first).ok().map(str::to_uppercase)
    }

    /// Canonical hash input: length-prefixed argv, exactly as ADR-024 specifies.
    ///
    /// `len(argc) ‖ Σ (len(arg_i) ‖ arg_i)`, all lengths as little-endian u64. Length
    /// prefixing is what makes `["a","bc"]` and `["ab","c"]` hash differently, which is the
    /// whole point of binding approvals to bytes rather than to a rendered digest.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.args.iter().map(|a| a.len() + 8).sum::<usize>());
        out.extend_from_slice(&(self.args.len() as u64).to_le_bytes());
        for a in &self.args {
            out.extend_from_slice(&(a.len() as u64).to_le_bytes());
            out.extend_from_slice(a);
        }
        out
    }

    /// Display form of the whole command, escaped (v2.1 §23.5).
    /// This is for humans only; never hash or compare this.
    #[must_use]
    pub fn display(&self) -> SafeText {
        let mut joined = Vec::new();
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                joined.push(b' ');
            }
            joined.extend_from_slice(a);
        }
        SafeText::from_bytes(Bytes::from(joined))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn req(args: &[&[u8]]) -> CommandRequest {
        CommandRequest::new(
            args.iter().map(|a| Bytes::copy_from_slice(a)).collect(),
            RequestOrigin::User,
            Effects::read(),
        )
    }

    #[test]
    fn canonical_bytes_distinguish_argument_boundaries() {
        // SEC-10: the exact reason approvals bind to this and not to a rendered string.
        let a = req(&[b"SET", b"a", b"bc"]);
        let b = req(&[b"SET", b"ab", b"c"]);
        assert_ne!(a.canonical_bytes(), b.canonical_bytes());
        // ...even though a naive space-join is identical for these two:
        assert_ne!(a.display().as_display(), b.display().as_display());

        let c = req(&[b"SET", b"a b"]);
        let d = req(&[b"SET", b"a", b"b"]);
        assert_ne!(c.canonical_bytes(), d.canonical_bytes());
        assert_eq!(c.display().as_display(), d.display().as_display(), "display collides; bytes must not");
    }

    #[test]
    fn canonical_bytes_are_stable_and_exact() {
        let r = req(&[b"GET", b"k"]);
        assert_eq!(r.canonical_bytes(), r.canonical_bytes());
        let mut expect = Vec::new();
        expect.extend_from_slice(&2u64.to_le_bytes());
        expect.extend_from_slice(&3u64.to_le_bytes());
        expect.extend_from_slice(b"GET");
        expect.extend_from_slice(&1u64.to_le_bytes());
        expect.extend_from_slice(b"k");
        assert_eq!(r.canonical_bytes(), expect);
    }

    #[test]
    fn empty_argument_is_preserved() {
        let r = req(&[b"SET", b"k", b""]);
        assert_eq!(r.args[2].len(), 0);
        assert_ne!(r.canonical_bytes(), req(&[b"SET", b"k"]).canonical_bytes());
    }

    #[test]
    fn binary_arguments_survive() {
        let r = req(&[b"SET", b"k", b"\0\xff\x1b"]);
        assert_eq!(r.args[2].as_ref(), b"\0\xff\x1b");
        assert!(!r.display().as_display().contains('\x1b'));
    }

    #[test]
    fn command_name_is_uppercased_and_tolerates_binary() {
        assert_eq!(req(&[b"hgetall", b"k"]).command_name().as_deref(), Some("HGETALL"));
        assert_eq!(req(&[b"\xff"]).command_name(), None);
        assert_eq!(CommandRequest::new(vec![], RequestOrigin::User, Effects::unknown()).command_name(), None);
    }

    #[test]
    fn unknown_effects_count_as_mutating() {
        // v2.1 §20.2: unclassified commands are maximum risk, not "probably read-only".
        assert!(Effects::unknown().mutates());
        assert!(!Effects::read().mutates());
        assert!(Effects::write().mutates());
    }

    #[test]
    fn agent_and_pipe_origins_require_human_approval() {
        assert!(RequestOrigin::Agent.requires_human_approval_for_writes());
        assert!(RequestOrigin::Pipe.requires_human_approval_for_writes());
        assert!(!RequestOrigin::User.requires_human_approval_for_writes());
    }
}
