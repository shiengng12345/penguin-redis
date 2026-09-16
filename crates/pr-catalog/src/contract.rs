//! The frozen catalog contract (v2.1 §11.11, §20.2, §31.4, ADR-030, V-J02).
//!
//! One of the twelve types V-J02 freezes lives here: [`LocalCommandSpec`].
//!
//! It is deliberately *not* [`crate::spec::CommandSpec`]. `CommandSpec` is the compiled shape
//! with a grammar tree, version gates and a family, and it will keep changing as the compiler
//! learns more. `LocalCommandSpec` is what the **policy** layer compiles against, and ADR-030
//! makes it the single authority for effects, danger and key extraction — so the thing a
//! security decision is made from is a small, frozen, auditable surface rather than whatever
//! the catalog happens to look like this week.

use pr_core::Effects;

/// The version of the contract this crate owns.
///
/// ```
/// assert_eq!(pr_catalog::contract::SCHEMA_VERSION, 1);
/// ```
pub const SCHEMA_VERSION: u32 = 1;

/// What the local signed catalog says about one command (ADR-030).
///
/// > 本地签名 catalog / overlay 是 effects、危险性、审批等级与 key extraction 的唯一权威；
/// > 远端 metadata 只补充 availability 与文档。
///
/// The direction is one-way and the type says so: [`Self::merge_remote`] may take a command
/// from `unknown` to *still* unknown-with-a-note, and it may never widen anything.
///
/// ```
/// use pr_catalog::contract::{Approval, LocalCommandSpec};
/// use pr_core::Effects;
///
/// let flushall = LocalCommandSpec {
///     name: "FLUSHALL".into(),
///     effects: Effects { writes_data: true, destructive: true, ..Effects::default() },
///     approval: Approval::HumanEveryTime,
///     key_positions: Vec::new(),
///     classified: true,
/// };
/// assert!(flushall.effects.destructive);
/// assert_eq!(flushall.approval, Approval::HumanEveryTime);
///
/// // A server claiming it is read-only changes nothing.
/// let mut spec = flushall.clone();
/// spec.merge_remote(Effects::read());
/// assert!(spec.effects.destructive, "a server may not declassify a local judgement");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommandSpec {
    /// Canonical uppercase name, including any container prefix (`CLIENT|KILL`).
    pub name: String,
    /// Locally assigned effects. The policy input.
    pub effects: Effects,
    /// What it takes to run this.
    pub approval: Approval,
    /// Argument positions that hold keys, zero-based within argv.
    ///
    /// Positions rather than extracted keys, so the extraction happens once, over the exact
    /// bytes about to be sent, rather than over a rendering of them.
    pub key_positions: Vec<usize>,
    /// Whether the local catalog recognised this command at all.
    ///
    /// `false` means unknown, which §20.2 treats as maximum risk outside dev — not as safe.
    pub classified: bool,
}

impl LocalCommandSpec {
    /// A command the local catalog does not recognise.
    ///
    /// ```
    /// use pr_catalog::contract::{Approval, LocalCommandSpec};
    /// let s = LocalCommandSpec::unknown("SOMEMODULE.DOTHING");
    /// assert!(!s.classified);
    /// assert!(s.effects.unknown);
    /// // Unknown means a person decides, not that it is harmless.
    /// assert_eq!(s.approval, Approval::HumanEveryTime);
    /// ```
    #[must_use]
    pub fn unknown(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            effects: Effects::unknown(),
            approval: Approval::HumanEveryTime,
            key_positions: Vec::new(),
            classified: false,
        }
    }

    /// Fold in what a server said about this command.
    ///
    /// One-way by construction (ADR-030): remote metadata may add risk and may never remove
    /// it. A poisoned or stale `COMMAND` reply that marks a write read-only changes nothing,
    /// because this method only ever ORs risk in.
    pub fn merge_remote(&mut self, remote: Effects) {
        self.effects.writes_data |= remote.writes_data;
        self.effects.destructive |= remote.destructive;
        self.effects.consumes |= remote.consumes;
        self.effects.admin |= remote.admin;
        self.effects.blocks |= remote.blocks;
        // `reads_data` is the one flag that adds no risk, so it may be learned.
        self.effects.reads_data |= remote.reads_data;
        // `unknown` is never cleared by a remote claim: the local catalog not recognising a
        // command is a fact about the local catalog, and the server cannot settle it.
        if self.effects.writes_data || self.effects.destructive || self.effects.admin {
            self.approval = Approval::HumanEveryTime;
        }
    }
}

/// What it takes to run a command (v2.1 §23.1, §29.4, ADR-024).
///
/// ```
/// use pr_catalog::contract::Approval;
/// // Only the mildest level may be satisfied by a policy rule (§29.4, R12).
/// assert!(Approval::None.policy_may_satisfy());
/// assert!(!Approval::HumanOncePerSession.policy_may_satisfy());
/// assert!(!Approval::HumanEveryTime.policy_may_satisfy());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Approval {
    /// Runs without asking.
    None,
    /// A person confirms once per session for this shape of command.
    HumanOncePerSession,
    /// A person confirms every single time.
    HumanEveryTime,
}

impl Approval {
    /// Whether a pre-approved policy rule can stand in for a person here.
    #[must_use]
    pub fn policy_may_satisfy(self) -> bool {
        matches!(self, Self::None)
    }
}

/// The contracts this crate owns, for the V-J02 test to walk.
pub const OWNED: &[&str] = &["LocalCommandSpec"];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn remote_metadata_can_only_add_risk() {
        // ADR-030's whole point, at the one place it could be lost. A server that reports a
        // destructive command as a harmless read must change nothing.
        let mut s = LocalCommandSpec {
            name: "FLUSHDB".into(),
            effects: Effects {
                writes_data: true,
                destructive: true,
                ..Effects::default()
            },
            approval: Approval::HumanEveryTime,
            key_positions: Vec::new(),
            classified: true,
        };
        let before = s.effects;
        s.merge_remote(Effects::read());
        assert!(s.effects.writes_data && s.effects.destructive);
        assert_eq!(s.approval, Approval::HumanEveryTime);
        assert_eq!(s.effects.writes_data, before.writes_data);

        // And the other direction does work: a server reporting a write on something we
        // thought was read-only raises the level.
        let mut mild = LocalCommandSpec {
            name: "SOMETHING".into(),
            effects: Effects::read(),
            approval: Approval::None,
            key_positions: vec![1],
            classified: true,
        };
        mild.merge_remote(Effects::write());
        assert!(mild.effects.writes_data);
        assert_eq!(mild.approval, Approval::HumanEveryTime);
    }

    #[test]
    fn an_unknown_command_needs_a_person() {
        let s = LocalCommandSpec::unknown("X.Y");
        assert!(s.effects.unknown);
        assert!(!s.approval.policy_may_satisfy());
    }

    #[test]
    fn a_server_cannot_clear_the_unknown_flag() {
        // The local catalog not recognising a command is a fact about the local catalog. A
        // server saying "it is fine" does not settle it, because that is the one claim a
        // compromised server would most want to make.
        let mut s = LocalCommandSpec::unknown("X.Y");
        s.merge_remote(Effects::read());
        assert!(s.effects.unknown, "a remote reply cleared the unknown flag");
    }
}
