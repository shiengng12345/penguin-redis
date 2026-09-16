//! Connection session state (v2.1 §22.4–22.6, ADR-007, R42).
//!
//! A Redis connection is not stateless, and the review found several places where treating
//! it as such produces wrong answers:
//! - `QUEUED` inside `MULTI` is not "applied" (STATE-03)
//! - `CLIENT REPLY OFF|SKIP` means the next reply belongs to a *different* command, so an
//!   assistance probe injected here would desynchronise the stream (§22.6, ASSIST-061)
//! - after a reconnect the transaction, the reply mode and any one-shot authorisation are
//!   **gone**; silently restoring them is how a client replays a write (STATE-06)
//!
//! This models exactly those transitions so the kernel can refuse the unsafe ones.

use crate::outcome::{Delivery, EffectsCertainty, ExecutionOutcome, RenderStatus, Reply};

/// RESP protocol in force.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    /// RESP2 — the base mode (v2.1 §20.1).
    #[default]
    Resp2,
    /// RESP3, entered by `HELLO 3`.
    Resp3,
}

/// Whether the server is currently sending replies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReplyMode {
    /// Normal.
    #[default]
    On,
    /// `CLIENT REPLY OFF`: nothing comes back until `ON`.
    Off,
    /// `CLIENT REPLY SKIP`: exactly the next command's reply is suppressed.
    SkipNext,
}

/// Transaction state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TxState {
    /// No transaction.
    #[default]
    None,
    /// `WATCH` issued, no `MULTI` yet.
    Watching {
        /// Number of watched keys.
        keys: usize,
    },
    /// Inside `MULTI`; commands are being queued, not applied.
    Queuing {
        /// Commands queued so far.
        queued: usize,
        /// Whether a queued command was rejected, which makes `EXEC` abort.
        had_error: bool,
    },
}

/// Why a command cannot be sent on this session right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionRefusal {
    /// An assistance/discovery probe was aimed at a session that is mid-transaction.
    #[error("cannot inject a background request inside a transaction")]
    ProbeInsideTransaction,
    /// An assistance/discovery probe was aimed at a reply-suppressed session.
    #[error("cannot inject a background request while replies are suppressed")]
    ProbeWhileReplySuppressed,
    /// `EXEC`/`DISCARD` without `MULTI`.
    #[error("no transaction in progress")]
    NoTransaction,
}

/// The state of one connection.
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    protocol: Protocol,
    db: u32,
    reply_mode: ReplyMode,
    tx: TxState,
    subscribed: bool,
    /// Bumped whenever the authenticated identity changes (AUTH/HELLO/RESET).
    identity_epoch: u64,
    /// Bumped on anything that invalidates observed names (§12.6).
    scope_epoch: u64,
}

impl SessionState {
    /// A fresh RESP2 session on DB 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current protocol.
    #[must_use]
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }
    /// Selected database.
    #[must_use]
    pub fn db(&self) -> u32 {
        self.db
    }
    /// Reply mode.
    #[must_use]
    pub fn reply_mode(&self) -> ReplyMode {
        self.reply_mode
    }
    /// Transaction state.
    #[must_use]
    pub fn tx(&self) -> &TxState {
        &self.tx
    }
    /// Whether a subscription is active.
    #[must_use]
    pub fn subscribed(&self) -> bool {
        self.subscribed
    }
    /// Identity epoch (§12.6 / ADR-017).
    #[must_use]
    pub fn identity_epoch(&self) -> u64 {
        self.identity_epoch
    }
    /// Observation-scope epoch.
    #[must_use]
    pub fn scope_epoch(&self) -> u64 {
        self.scope_epoch
    }

    /// Whether the kernel may inject a metadata/discovery request here.
    ///
    /// §22.6 and ASSIST-060/061: never inside a transaction (it would be queued into the
    /// user's `MULTI`), and never while replies are suppressed (it would consume the slot of
    /// a later reply).
    ///
    /// # Errors
    /// [`SessionRefusal`] naming which invariant forbids it.
    pub fn may_probe(&self) -> Result<(), SessionRefusal> {
        if matches!(self.tx, TxState::Queuing { .. }) {
            return Err(SessionRefusal::ProbeInsideTransaction);
        }
        if self.reply_mode != ReplyMode::On {
            return Err(SessionRefusal::ProbeWhileReplySuppressed);
        }
        Ok(())
    }

    /// Whether a reply is expected for the command about to be sent, and advance `SkipNext`.
    pub fn expect_reply_for_next(&mut self) -> bool {
        match self.reply_mode {
            ReplyMode::On => true,
            ReplyMode::Off => false,
            ReplyMode::SkipNext => {
                // SKIP applies to exactly one command, then normal service resumes.
                self.reply_mode = ReplyMode::On;
                false
            }
        }
    }

    /// Apply a state-changing command the user issued.
    pub fn observe_command(&mut self, argv: &[&[u8]]) {
        let Some(name) = argv.first() else { return };
        let upper = name.to_ascii_uppercase();
        match upper.as_slice() {
            b"HELLO" => {
                if let Some(v) = argv.get(1)
                    && *v == b"3"
                {
                    self.protocol = Protocol::Resp3;
                } else if argv.len() > 1 {
                    self.protocol = Protocol::Resp2;
                }
                // HELLO may carry AUTH, so treat it as an identity change (§22.6).
                if argv.iter().any(|a| a.eq_ignore_ascii_case(b"AUTH")) {
                    self.bump_identity();
                }
            }
            b"AUTH" => self.bump_identity(),
            b"RESET" => self.reset(),
            b"SELECT" => {
                if let Some(v) = argv.get(1)
                    && let Ok(n) = std::str::from_utf8(v).unwrap_or("").parse::<u32>()
                {
                    self.db = n;
                    self.scope_epoch += 1; // observed names are per-DB (§12.6)
                }
            }
            b"MULTI" => {
                self.tx = TxState::Queuing {
                    queued: 0,
                    had_error: false,
                }
            }
            b"EXEC" | b"DISCARD" => {
                self.tx = TxState::None;
            }
            b"WATCH" => {
                if matches!(self.tx, TxState::None) {
                    self.tx = TxState::Watching {
                        keys: argv.len().saturating_sub(1),
                    };
                }
            }
            b"UNWATCH" => {
                if matches!(self.tx, TxState::Watching { .. }) {
                    self.tx = TxState::None;
                }
            }
            b"SUBSCRIBE" | b"PSUBSCRIBE" | b"SSUBSCRIBE" => self.subscribed = true,
            b"UNSUBSCRIBE" | b"PUNSUBSCRIBE" | b"SUNSUBSCRIBE" => self.subscribed = false,
            b"CLIENT" => {
                if argv
                    .get(1)
                    .is_some_and(|s| s.eq_ignore_ascii_case(b"REPLY"))
                {
                    match argv.get(2).map(|s| s.to_ascii_uppercase()) {
                        Some(m) if m == b"OFF" => self.reply_mode = ReplyMode::Off,
                        Some(m) if m == b"SKIP" => self.reply_mode = ReplyMode::SkipNext,
                        Some(m) if m == b"ON" => self.reply_mode = ReplyMode::On,
                        _ => {}
                    }
                }
            }
            _ => {
                // Any other command inside MULTI is queued, not applied.
                if let TxState::Queuing { queued, had_error } = self.tx {
                    self.tx = TxState::Queuing {
                        queued: queued + 1,
                        had_error,
                    };
                }
            }
        }
    }

    /// Record that a queued command was rejected, which makes `EXEC` abort.
    pub fn note_queue_error(&mut self) {
        if let TxState::Queuing { queued, .. } = self.tx {
            self.tx = TxState::Queuing {
                queued,
                had_error: true,
            };
        }
    }

    /// The outcome for a command that was accepted into a transaction.
    #[must_use]
    pub fn queued_outcome() -> ExecutionOutcome {
        ExecutionOutcome {
            delivery: Delivery::Sent,
            reply: Reply::Queued,
            // STATE-03: queued is emphatically not applied, and EXEC may still abort.
            effects: EffectsCertainty::EffectsPossible,
            render: RenderStatus::Rendered,
        }
    }

    fn bump_identity(&mut self) {
        self.identity_epoch += 1;
        self.scope_epoch += 1;
    }

    /// `RESET`: back to a clean connection.
    pub fn reset(&mut self) {
        let id = self.identity_epoch + 1;
        let scope = self.scope_epoch + 1;
        *self = Self::default();
        self.identity_epoch = id;
        self.scope_epoch = scope;
    }

    /// The connection dropped. Returns what must NOT be silently restored.
    ///
    /// ADR-007 / STATE-06/08: a reconnect gives a clean connection. The transaction is gone,
    /// the reply mode is back to normal, and a subscription that was active has a gap.
    pub fn on_disconnect(&mut self) -> ReconnectLosses {
        let losses = ReconnectLosses {
            had_transaction: !matches!(self.tx, TxState::None),
            had_subscription: self.subscribed,
            reply_mode_was: self.reply_mode,
            db_was: self.db,
            protocol_was: self.protocol,
        };
        let id = self.identity_epoch + 1;
        let scope = self.scope_epoch + 1;
        *self = Self::default();
        self.identity_epoch = id;
        self.scope_epoch = scope;
        losses
    }
}

/// What a reconnect destroyed. The caller must surface these rather than paper over them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectLosses {
    /// A transaction was in progress and is gone. Never rebuild it (STATE-06).
    pub had_transaction: bool,
    /// A subscription was active; messages during the gap are lost (STATE-08, §26.2).
    pub had_subscription: bool,
    /// Reply mode before the drop; it is back to `On` now.
    pub reply_mode_was: ReplyMode,
    /// Database before the drop; re-selecting it is a visible action.
    pub db_was: u32,
    /// Protocol before the drop.
    pub protocol_was: Protocol,
}

impl ReconnectLosses {
    /// Whether anything needs to be shown to the user.
    #[must_use]
    pub fn needs_disclosure(&self) -> bool {
        self.had_transaction || self.had_subscription || self.reply_mode_was != ReplyMode::On
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn cmd(s: &mut SessionState, argv: &[&[u8]]) {
        s.observe_command(argv);
    }

    #[test]
    fn starts_in_resp2_on_db_zero() {
        let s = SessionState::new();
        assert_eq!(s.protocol(), Protocol::Resp2);
        assert_eq!(s.db(), 0);
        assert_eq!(s.reply_mode(), ReplyMode::On);
        assert_eq!(*s.tx(), TxState::None);
    }

    #[test]
    fn hello_three_switches_protocol() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"HELLO", b"3"]);
        assert_eq!(s.protocol(), Protocol::Resp3);
        cmd(&mut s, &[b"HELLO", b"2"]);
        assert_eq!(s.protocol(), Protocol::Resp2);
    }

    #[test]
    fn hello_with_auth_changes_identity() {
        // §22.6: HELLO can carry AUTH, so it is an identity change, not a query.
        let mut s = SessionState::new();
        let before = s.identity_epoch();
        cmd(&mut s, &[b"HELLO", b"3", b"AUTH", b"user", b"pw"]);
        assert!(s.identity_epoch() > before);
        assert!(
            s.scope_epoch() > 0,
            "observed names must be invalidated too"
        );
    }

    #[test]
    fn auth_and_reset_bump_the_identity_epoch() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"AUTH", b"pw"]);
        assert_eq!(s.identity_epoch(), 1);
        cmd(&mut s, &[b"RESET"]);
        assert_eq!(s.identity_epoch(), 2);
        assert_eq!(
            s.protocol(),
            Protocol::Resp2,
            "RESET returns to a clean connection"
        );
        assert_eq!(s.db(), 0);
    }

    #[test]
    fn select_changes_db_and_invalidates_scope() {
        // ASSIST-030: key candidates are per-DB.
        let mut s = SessionState::new();
        let before = s.scope_epoch();
        cmd(&mut s, &[b"SELECT", b"3"]);
        assert_eq!(s.db(), 3);
        assert!(s.scope_epoch() > before);
    }

    #[test]
    fn multi_queues_rather_than_applies() {
        // STATE-03.
        let mut s = SessionState::new();
        cmd(&mut s, &[b"MULTI"]);
        cmd(&mut s, &[b"SET", b"k", b"1"]);
        cmd(&mut s, &[b"SET", b"k", b"2"]);
        assert_eq!(
            *s.tx(),
            TxState::Queuing {
                queued: 2,
                had_error: false
            }
        );

        let o = SessionState::queued_outcome();
        assert_eq!(o.reply, Reply::Queued);
        assert!(o.is_uncertain(), "QUEUED must never read as applied");
    }

    #[test]
    fn a_rejected_queue_entry_is_remembered() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"MULTI"]);
        cmd(&mut s, &[b"BADCMD"]);
        s.note_queue_error();
        assert_eq!(
            *s.tx(),
            TxState::Queuing {
                queued: 1,
                had_error: true
            }
        );
    }

    #[test]
    fn exec_and_discard_end_the_transaction() {
        for ending in [&b"EXEC"[..], &b"DISCARD"[..]] {
            let mut s = SessionState::new();
            cmd(&mut s, &[b"MULTI"]);
            cmd(&mut s, &[b"SET", b"k", b"1"]);
            cmd(&mut s, &[ending]);
            assert_eq!(
                *s.tx(),
                TxState::None,
                "{ending:?} must clear the transaction"
            );
        }
    }

    #[test]
    fn watch_then_unwatch() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"WATCH", b"a", b"b"]);
        assert_eq!(*s.tx(), TxState::Watching { keys: 2 });
        cmd(&mut s, &[b"UNWATCH"]);
        assert_eq!(*s.tx(), TxState::None);
    }

    #[test]
    fn a_probe_is_refused_inside_a_transaction() {
        // ASSIST-060: an assistance request here would be queued into the user's MULTI.
        let mut s = SessionState::new();
        assert!(s.may_probe().is_ok());
        cmd(&mut s, &[b"MULTI"]);
        assert_eq!(s.may_probe(), Err(SessionRefusal::ProbeInsideTransaction));
        cmd(&mut s, &[b"DISCARD"]);
        assert!(s.may_probe().is_ok());
    }

    #[test]
    fn a_probe_is_refused_while_replies_are_suppressed() {
        // ASSIST-061: it would consume the slot belonging to a later reply.
        let mut s = SessionState::new();
        cmd(&mut s, &[b"CLIENT", b"REPLY", b"OFF"]);
        assert_eq!(s.reply_mode(), ReplyMode::Off);
        assert_eq!(
            s.may_probe(),
            Err(SessionRefusal::ProbeWhileReplySuppressed)
        );
        cmd(&mut s, &[b"CLIENT", b"REPLY", b"ON"]);
        assert!(s.may_probe().is_ok());
    }

    #[test]
    fn client_reply_skip_applies_to_exactly_one_command() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"CLIENT", b"REPLY", b"SKIP"]);
        assert_eq!(s.reply_mode(), ReplyMode::SkipNext);
        assert!(
            !s.expect_reply_for_next(),
            "the skipped command gets no reply"
        );
        assert_eq!(s.reply_mode(), ReplyMode::On, "normal service resumes");
        assert!(s.expect_reply_for_next(), "the one after does get a reply");
    }

    #[test]
    fn client_reply_off_suppresses_every_reply() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"CLIENT", b"REPLY", b"OFF"]);
        assert!(!s.expect_reply_for_next());
        assert!(!s.expect_reply_for_next());
        assert_eq!(s.reply_mode(), ReplyMode::Off, "OFF does not decay");
    }

    #[test]
    fn subscription_state_tracks_both_directions() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"SUBSCRIBE", b"ch"]);
        assert!(s.subscribed());
        cmd(&mut s, &[b"UNSUBSCRIBE"]);
        assert!(!s.subscribed());
    }

    #[test]
    fn disconnect_reports_losses_and_never_restores_them() {
        // STATE-06 / STATE-08: the whole point of ADR-007.
        let mut s = SessionState::new();
        cmd(&mut s, &[b"SELECT", b"3"]);
        cmd(&mut s, &[b"HELLO", b"3"]);
        cmd(&mut s, &[b"SUBSCRIBE", b"ch"]);
        cmd(&mut s, &[b"MULTI"]);
        cmd(&mut s, &[b"SET", b"k", b"1"]);

        let lost = s.on_disconnect();
        assert!(lost.had_transaction, "must report the lost transaction");
        assert!(lost.had_subscription, "must report the subscription gap");
        assert_eq!(lost.db_was, 3);
        assert_eq!(lost.protocol_was, Protocol::Resp3);
        assert!(lost.needs_disclosure());

        // The new connection is clean: nothing silently carried over.
        assert_eq!(*s.tx(), TxState::None);
        assert!(!s.subscribed());
        assert_eq!(s.db(), 0);
        assert_eq!(s.protocol(), Protocol::Resp2);
        assert_eq!(s.reply_mode(), ReplyMode::On);
    }

    #[test]
    fn disconnect_invalidates_identity_and_scope() {
        let mut s = SessionState::new();
        let id = s.identity_epoch();
        let scope = s.scope_epoch();
        s.on_disconnect();
        assert!(s.identity_epoch() > id);
        assert!(
            s.scope_epoch() > scope,
            "observed names must not survive a reconnect"
        );
    }

    #[test]
    fn a_quiet_disconnect_needs_no_disclosure() {
        let mut s = SessionState::new();
        let lost = s.on_disconnect();
        assert!(!lost.needs_disclosure());
    }

    #[test]
    fn command_names_are_case_insensitive() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"multi"]);
        assert!(matches!(*s.tx(), TxState::Queuing { .. }));
        cmd(&mut s, &[b"DiScArD"]);
        assert_eq!(*s.tx(), TxState::None);
    }

    #[test]
    fn a_malformed_select_does_not_change_the_db() {
        let mut s = SessionState::new();
        cmd(&mut s, &[b"SELECT", b"abc"]);
        assert_eq!(s.db(), 0);
        cmd(&mut s, &[b"SELECT"]);
        assert_eq!(s.db(), 0);
    }
}
