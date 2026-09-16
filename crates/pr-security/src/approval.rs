//! `ApprovalToken` — approvals bind to exact bytes (v2.1 §23.2, ADR-024, R02).
//!
//! This is the fix for one of the two BLOCKERs the three-way review found. v2.0 bound an
//! approval to "命令与参数摘要" — a *digest of the rendered command*. Redaction, truncation,
//! escaping and Unicode normalisation can all make two genuinely different commands render
//! identically, so a user could approve what they saw and the kernel could execute something
//! else.
//!
//! Here the token binds `blake3` over the length-prefixed argv (see
//! [`pr_core::CommandRequest::canonical_bytes`]) plus the full [`TrustIdentity`], DB, epochs
//! and an expiry. The kernel recomputes and compares before every execution; a single byte
//! of difference is a refusal. Rendered text is for the human only.

use crate::trust::TrustIdentity;
use pr_core::CommandRequest;

/// Monotonic counters that invalidate a token when the world changes underneath it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epochs {
    /// Bumped on AUTH / HELLO / RESET or any identity change.
    pub auth_identity: u64,
    /// Bumped when local policy is edited.
    pub policy: u64,
    /// Bumped on cluster topology change.
    pub topology: u64,
}

/// Who issued an approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issuer {
    /// A person confirmed it interactively.
    Human,
    /// A pre-approved policy rule matched. Restricted to read-only work (ADR-024, R12):
    /// a policy can never authorise a write, a consuming read, an admin command or an
    /// unclassified command.
    Policy {
        /// Rule identifier, for audit.
        id: String,
    },
}

/// A capability to run one exact request (or one exact plan) against one exact target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalToken {
    /// Profile this was issued for.
    pub profile_uuid: String,
    /// Hash of the full trust identity (v2.1 §21.3).
    pub trust_identity_hash: String,
    /// Logical database.
    pub db_index: u32,
    /// Epochs at issue time.
    pub epochs: Epochs,
    /// `blake3` over the length-prefixed argv.
    pub request_hash: String,
    /// For batches: hash over the whole expanded plan. Equals `request_hash` for one command.
    pub plan_hash: String,
    /// Maximum number of actions this token authorises.
    pub max_actions: u32,
    /// Absolute expiry, as milliseconds since the Unix epoch.
    pub expires_at_ms: u64,
    /// Who issued it.
    pub issued_by: Issuer,
}

/// Why a token was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalRefusal {
    /// The argv does not match, byte for byte.
    RequestMismatch,
    /// A different profile.
    ProfileMismatch,
    /// A different trust identity.
    TrustIdentityMismatch,
    /// A different database.
    DbMismatch,
    /// Identity, policy or topology moved since issue.
    EpochChanged,
    /// Past its expiry.
    Expired,
    /// Action budget exhausted.
    ActionBudgetExhausted,
    /// A policy-issued token was used for something only a human may authorise.
    PolicyCannotAuthorise,
}

/// Compute the canonical request hash for a request.
#[must_use]
pub fn request_hash(req: &CommandRequest) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"penguin.request.v1\0");
    h.update(&req.canonical_bytes());
    h.finalize().to_hex().to_string()
}

/// Compute a plan hash over an ordered list of requests.
#[must_use]
pub fn plan_hash(reqs: &[CommandRequest]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"penguin.plan.v1\0");
    h.update(&(reqs.len() as u64).to_le_bytes());
    for r in reqs {
        let c = r.canonical_bytes();
        h.update(&(c.len() as u64).to_le_bytes());
        h.update(&c);
    }
    h.finalize().to_hex().to_string()
}

/// The context the kernel is about to execute in.
#[derive(Clone, Debug)]
pub struct ExecutionContext<'a> {
    /// Profile.
    pub profile_uuid: &'a str,
    /// Resolved identity.
    pub trust_identity: &'a TrustIdentity,
    /// Database.
    pub db_index: u32,
    /// Current epochs.
    pub epochs: Epochs,
    /// Wall clock, ms since Unix epoch.
    pub now_ms: u64,
    /// Actions already spent against this token.
    pub actions_used: u32,
}

impl ApprovalToken {
    /// Issue a token for one request.
    #[must_use]
    pub fn for_request(
        profile_uuid: impl Into<String>,
        identity: &TrustIdentity,
        db_index: u32,
        epochs: Epochs,
        req: &CommandRequest,
        expires_at_ms: u64,
        issued_by: Issuer,
    ) -> Self {
        let h = request_hash(req);
        Self {
            profile_uuid: profile_uuid.into(),
            trust_identity_hash: identity.hash(),
            db_index,
            epochs,
            request_hash: h.clone(),
            plan_hash: h,
            max_actions: 1,
            expires_at_ms,
            issued_by,
        }
    }

    /// Validate this token against the request the kernel is about to send.
    ///
    /// # Errors
    /// Returns the specific [`ApprovalRefusal`]; the caller maps it to exit code 5.
    pub fn authorise(
        &self,
        req: &CommandRequest,
        ctx: &ExecutionContext,
    ) -> Result<(), ApprovalRefusal> {
        if self.profile_uuid != ctx.profile_uuid {
            return Err(ApprovalRefusal::ProfileMismatch);
        }
        if self.trust_identity_hash != ctx.trust_identity.hash() {
            return Err(ApprovalRefusal::TrustIdentityMismatch);
        }
        if self.db_index != ctx.db_index {
            return Err(ApprovalRefusal::DbMismatch);
        }
        if self.epochs != ctx.epochs {
            return Err(ApprovalRefusal::EpochChanged);
        }
        if ctx.now_ms >= self.expires_at_ms {
            return Err(ApprovalRefusal::Expired);
        }
        if ctx.actions_used >= self.max_actions {
            return Err(ApprovalRefusal::ActionBudgetExhausted);
        }
        // Recompute from the bytes about to go on the wire — never trust a rendered digest.
        if request_hash(req) != self.request_hash {
            return Err(ApprovalRefusal::RequestMismatch);
        }
        // A policy may only authorise pure reads by a non-agent origin (R12).
        if matches!(self.issued_by, Issuer::Policy { .. })
            && (req.effects.mutates() || req.origin.requires_human_approval_for_writes())
        {
            return Err(ApprovalRefusal::PolicyCannotAuthorise);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::trust::{AuthIdentity, Endpoint, ServerIdentity, TlsIdentity};
    use bytes::Bytes;
    use pr_core::{Effects, RequestOrigin};

    fn ident() -> TrustIdentity {
        TrustIdentity {
            endpoint: Endpoint::Tcp {
                host: "h".into(),
                port: 6379,
            },
            tls_identity: TlsIdentity::None,
            server_identity: ServerIdentity::Standalone {
                run_id_prefix: None,
            },
            auth_identity: AuthIdentity {
                username: None,
                secret_ref: "credential:1".into(),
            },
        }
    }

    fn req(args: &[&[u8]], effects: Effects) -> CommandRequest {
        CommandRequest::new(
            args.iter().map(|a| Bytes::copy_from_slice(a)).collect(),
            RequestOrigin::User,
            effects,
        )
    }

    fn ctx(id: &TrustIdentity) -> ExecutionContext<'_> {
        ExecutionContext {
            profile_uuid: "p1",
            trust_identity: id,
            db_index: 0,
            epochs: Epochs::default(),
            now_ms: 1_000,
            actions_used: 0,
        }
    }

    fn token(id: &TrustIdentity, r: &CommandRequest) -> ApprovalToken {
        ApprovalToken::for_request("p1", id, 0, Epochs::default(), r, 10_000, Issuer::Human)
    }

    #[test]
    fn approves_the_exact_request_it_was_issued_for() {
        let id = ident();
        let r = req(&[b"SET", b"k", b"v"], Effects::write());
        assert_eq!(token(&id, &r).authorise(&r, &ctx(&id)), Ok(()));
    }

    #[test]
    fn same_display_different_bytes_is_refused() {
        // SEC-10, the BLOCKER: `SET a b` as one arg vs two must not share an approval.
        let id = ident();
        let approved = req(&[b"SET", b"a b"], Effects::write());
        let substituted = req(&[b"SET", b"a", b"b"], Effects::write());
        assert_eq!(
            approved.display().as_display(),
            substituted.display().as_display()
        );
        let t = token(&id, &approved);
        assert_eq!(t.authorise(&approved, &ctx(&id)), Ok(()));
        assert_eq!(
            t.authorise(&substituted, &ctx(&id)),
            Err(ApprovalRefusal::RequestMismatch)
        );
    }

    #[test]
    fn a_single_byte_difference_is_refused() {
        let id = ident();
        let a = req(&[b"DEL", b"player:10001"], Effects::write());
        let b = req(&[b"DEL", b"player:10002"], Effects::write());
        assert_eq!(
            token(&id, &a).authorise(&b, &ctx(&id)),
            Err(ApprovalRefusal::RequestMismatch)
        );
    }

    #[test]
    fn unicode_normalisation_cannot_collide() {
        // Composed vs decomposed é render the same but are different bytes.
        let id = ident();
        let composed = req(&[b"GET", "caf\u{e9}".as_bytes()], Effects::read());
        let decomposed = req(&[b"GET", "cafe\u{301}".as_bytes()], Effects::read());
        assert_eq!(
            token(&id, &composed).authorise(&decomposed, &ctx(&id)),
            Err(ApprovalRefusal::RequestMismatch)
        );
    }

    #[test]
    fn epoch_change_invalidates() {
        let id = ident();
        let r = req(&[b"GET", b"k"], Effects::read());
        let t = token(&id, &r);
        let mut c = ctx(&id);
        c.epochs.auth_identity = 1;
        assert_eq!(t.authorise(&r, &c), Err(ApprovalRefusal::EpochChanged));
        let mut c2 = ctx(&id);
        c2.epochs.topology = 9;
        assert_eq!(t.authorise(&r, &c2), Err(ApprovalRefusal::EpochChanged));
    }

    #[test]
    fn expiry_and_action_budget_are_enforced() {
        let id = ident();
        let r = req(&[b"GET", b"k"], Effects::read());
        let t = token(&id, &r);
        let mut c = ctx(&id);
        c.now_ms = 10_000;
        assert_eq!(t.authorise(&r, &c), Err(ApprovalRefusal::Expired));
        let mut c2 = ctx(&id);
        c2.actions_used = 1;
        assert_eq!(
            t.authorise(&r, &c2),
            Err(ApprovalRefusal::ActionBudgetExhausted)
        );
    }

    #[test]
    fn token_does_not_travel_across_profile_db_or_identity() {
        let id = ident();
        let r = req(&[b"GET", b"k"], Effects::read());
        let t = token(&id, &r);

        let mut c = ctx(&id);
        c.profile_uuid = "p2";
        assert_eq!(t.authorise(&r, &c), Err(ApprovalRefusal::ProfileMismatch));

        let mut c2 = ctx(&id);
        c2.db_index = 3;
        assert_eq!(t.authorise(&r, &c2), Err(ApprovalRefusal::DbMismatch));

        let mut other = ident();
        other.auth_identity.secret_ref = "credential:2".into();
        let c3 = ctx(&other);
        assert_eq!(
            t.authorise(&r, &c3),
            Err(ApprovalRefusal::TrustIdentityMismatch)
        );
    }

    #[test]
    fn policy_issued_tokens_cannot_authorise_writes_or_agents() {
        // R12: pre-approved policy covers read-only work only.
        let id = ident();
        let w = req(&[b"SET", b"k", b"v"], Effects::write());
        let t = ApprovalToken::for_request(
            "p1",
            &id,
            0,
            Epochs::default(),
            &w,
            10_000,
            Issuer::Policy { id: "ro".into() },
        );
        assert_eq!(
            t.authorise(&w, &ctx(&id)),
            Err(ApprovalRefusal::PolicyCannotAuthorise)
        );

        let r = req(&[b"GET", b"k"], Effects::read());
        let t2 = ApprovalToken::for_request(
            "p1",
            &id,
            0,
            Epochs::default(),
            &r,
            10_000,
            Issuer::Policy { id: "ro".into() },
        );
        assert_eq!(t2.authorise(&r, &ctx(&id)), Ok(()));

        let mut agent_read = r.clone();
        agent_read.origin = RequestOrigin::Agent;
        let t3 = ApprovalToken::for_request(
            "p1",
            &id,
            0,
            Epochs::default(),
            &agent_read,
            10_000,
            Issuer::Policy { id: "ro".into() },
        );
        assert_eq!(
            t3.authorise(&agent_read, &ctx(&id)),
            Err(ApprovalRefusal::PolicyCannotAuthorise)
        );
    }

    #[test]
    fn unknown_effects_count_as_mutating_for_policy_tokens() {
        let id = ident();
        let u = req(&[b"NEWCMD", b"x"], Effects::unknown());
        let t = ApprovalToken::for_request(
            "p1",
            &id,
            0,
            Epochs::default(),
            &u,
            10_000,
            Issuer::Policy { id: "ro".into() },
        );
        assert_eq!(
            t.authorise(&u, &ctx(&id)),
            Err(ApprovalRefusal::PolicyCannotAuthorise)
        );
    }

    #[test]
    fn plan_hash_is_order_sensitive_and_boundary_sensitive() {
        let a = req(&[b"SET", b"k", b"1"], Effects::write());
        let b = req(&[b"SET", b"k", b"2"], Effects::write());
        assert_ne!(
            plan_hash(&[a.clone(), b.clone()]),
            plan_hash(&[b.clone(), a.clone()])
        );
        assert_ne!(
            plan_hash(std::slice::from_ref(&a)),
            plan_hash(&[a.clone(), a.clone()])
        );
        assert_eq!(plan_hash(&[a.clone(), b.clone()]), plan_hash(&[a, b]));
    }
}
