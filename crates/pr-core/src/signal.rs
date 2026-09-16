//! Interrupts, and what each one means (v2.1 §22.3, §35.1, WIN-01, V-C07).
//!
//! §22.3 gives Ctrl+C six different meanings depending on what is happening, and the reason it
//! bothers is in the section's own words: it "不能笼统当'撤销服务器操作'". Stopping a wait is
//! not undoing a write. A client that says "cancelled" when it means "I stopped listening" is
//! telling the user the write did not happen, and it may well have.
//!
//! Windows adds one more: Ctrl+Break, which §35.1 defines as equivalent to two Ctrl+C. It
//! arrives through `SetConsoleCtrlHandler` rather than as a keystroke, so the *delivery*
//! differs by platform while the meaning does not — which is exactly why the meaning lives
//! here, in a crate that knows about neither terminals nor consoles.

/// What the session is doing when an interrupt arrives (§22.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Editing a line that has something in it.
    Editing,
    /// At an empty prompt.
    Idle,
    /// A request is composed but no byte has left.
    Unsent,
    /// A request is on the wire and a reply is expected.
    AwaitingReply,
    /// A blocking command is in flight (`BLPOP`, `XREAD BLOCK`).
    Blocked,
    /// A subscription or `MONITOR` is streaming.
    Streaming,
    /// A batch is part-way through.
    Batch,
}

/// The interrupt that arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// Ctrl+C, or `SIGINT`.
    Interrupt,
    /// Ctrl+Break. Windows only; §35.1 defines it as two Ctrl+C.
    Break,
}

/// What the client should do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Discard the line being edited. The session stays open.
    ClearInput,
    /// Drop a request that never left. Nothing reached the server.
    CancelUnsent,
    /// Stop waiting, and mark the request's effects **unknown** — bytes were sent.
    StopWaiting,
    /// Close the request's session; a blocking command needs `CLIENT UNBLOCK` from elsewhere
    /// to be released properly, and that is a privileged, separate thing.
    CloseRequestSession,
    /// End the stream and report where the observation stopped.
    EndStream,
    /// Issue no more requests; let the ones already sent settle.
    StopIssuing,
    /// Leave the REPL with exit code 130.
    Exit,
}

impl Response {
    /// Whether this response leaves the effects of an in-flight request unknown.
    ///
    /// The distinction §22.3 turns the exit code on: `130` may only be used when nothing is
    /// uncertain, so a caller must be able to ask.
    #[must_use]
    pub fn leaves_effects_unknown(self) -> bool {
        matches!(
            self,
            Self::StopWaiting | Self::CloseRequestSession | Self::StopIssuing
        )
    }
}

/// Tracks consecutive interrupts so the second one at an empty prompt can exit (§22.3).
#[derive(Clone, Debug, Default)]
pub struct Interrupts {
    /// How many interrupts have arrived at an idle prompt with nothing in between.
    consecutive_idle: usize,
}

impl Interrupts {
    /// A fresh counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many interrupts have arrived consecutively at an idle prompt.
    #[must_use]
    pub fn consecutive(&self) -> usize {
        self.consecutive_idle
    }

    /// Anything the user does that is not an interrupt clears the run.
    ///
    /// Without this, a Ctrl+C now and another one ten minutes later would quit — which is not
    /// what "twice" means to a person.
    pub fn activity(&mut self) {
        self.consecutive_idle = 0;
    }

    /// Decide what one interrupt means, given what the session is doing.
    pub fn on(&mut self, signal: Signal, activity: Activity) -> Response {
        // §35.1: Ctrl+Break is two Ctrl+C. Applied by *running the rule twice* rather than by
        // special-casing the outcome, so the two spellings cannot drift apart.
        if signal == Signal::Break {
            let first = self.on(Signal::Interrupt, activity);
            if first == Response::Exit {
                return first;
            }
            return self.on(Signal::Interrupt, Activity::Idle);
        }

        match activity {
            Activity::Idle => {
                self.consecutive_idle += 1;
                if self.consecutive_idle >= 2 {
                    Response::Exit
                } else {
                    // The first one is not nothing: it clears whatever the editor was showing
                    // and tells the user that another will leave.
                    Response::ClearInput
                }
            }
            Activity::Editing => {
                self.consecutive_idle = 0;
                Response::ClearInput
            }
            Activity::Unsent => {
                self.consecutive_idle = 0;
                Response::CancelUnsent
            }
            Activity::AwaitingReply => {
                self.consecutive_idle = 0;
                Response::StopWaiting
            }
            Activity::Blocked => {
                self.consecutive_idle = 0;
                Response::CloseRequestSession
            }
            Activity::Streaming => {
                self.consecutive_idle = 0;
                Response::EndStream
            }
            Activity::Batch => {
                self.consecutive_idle = 0;
                Response::StopIssuing
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_activity_in_section_22_3_has_its_own_meaning() {
        // The point of the table: "cancel" is not one behaviour. A client that treats them
        // alike tells the user a write did not happen when it may have.
        let cases = [
            (Activity::Editing, Response::ClearInput),
            (Activity::Unsent, Response::CancelUnsent),
            (Activity::AwaitingReply, Response::StopWaiting),
            (Activity::Blocked, Response::CloseRequestSession),
            (Activity::Streaming, Response::EndStream),
            (Activity::Batch, Response::StopIssuing),
        ];
        for (activity, want) in cases {
            let mut i = Interrupts::new();
            assert_eq!(i.on(Signal::Interrupt, activity), want, "{activity:?}");
        }
        // And they really are distinct, not six names for the same thing.
        let mut seen: Vec<Response> = cases.iter().map(|(_, r)| *r).collect();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len());
    }

    #[test]
    fn stopping_a_wait_leaves_the_effects_unknown_but_cancelling_an_unsent_one_does_not() {
        // This is the distinction the exit code turns on (§22.3): `130` means cancelled *and*
        // nothing uncertain.
        assert!(!Response::CancelUnsent.leaves_effects_unknown());
        assert!(!Response::ClearInput.leaves_effects_unknown());
        assert!(!Response::EndStream.leaves_effects_unknown());
        assert!(Response::StopWaiting.leaves_effects_unknown());
        assert!(Response::CloseRequestSession.leaves_effects_unknown());
        assert!(Response::StopIssuing.leaves_effects_unknown());
    }

    #[test]
    fn two_interrupts_at_an_empty_prompt_leave() {
        let mut i = Interrupts::new();
        assert_eq!(
            i.on(Signal::Interrupt, Activity::Idle),
            Response::ClearInput
        );
        assert_eq!(i.consecutive(), 1);
        assert_eq!(i.on(Signal::Interrupt, Activity::Idle), Response::Exit);
    }

    #[test]
    fn anything_the_user_does_in_between_resets_the_count() {
        // "Twice" means twice in a row. A Ctrl+C now and another after ten minutes of work is
        // not a request to quit.
        let mut i = Interrupts::new();
        assert_eq!(
            i.on(Signal::Interrupt, Activity::Idle),
            Response::ClearInput
        );
        i.activity();
        assert_eq!(i.consecutive(), 0);
        assert_eq!(
            i.on(Signal::Interrupt, Activity::Idle),
            Response::ClearInput,
            "the count restarted"
        );
    }

    #[test]
    fn an_interrupt_during_work_resets_the_count_too() {
        // One at the prompt, then one that cancels a real request: the next one at the prompt
        // is a first, not a second.
        let mut i = Interrupts::new();
        i.on(Signal::Interrupt, Activity::Idle);
        assert_eq!(
            i.on(Signal::Interrupt, Activity::AwaitingReply),
            Response::StopWaiting
        );
        assert_eq!(i.consecutive(), 0);
        assert_eq!(
            i.on(Signal::Interrupt, Activity::Idle),
            Response::ClearInput
        );
    }

    #[test]
    fn ctrl_break_at_an_empty_prompt_leaves_at_once() {
        // §35.1: equivalent to two Ctrl+C — so on Windows, one Break does what two Ctrl+C do.
        let mut i = Interrupts::new();
        assert_eq!(i.on(Signal::Break, Activity::Idle), Response::Exit);
    }

    #[test]
    fn ctrl_break_during_work_cancels_the_work_and_then_leaves() {
        // The first of the two does the §22.3 thing for the current activity; the second lands
        // at an idle prompt, which is where the second Ctrl+C would have landed.
        let mut i = Interrupts::new();
        assert_eq!(
            i.on(Signal::Break, Activity::AwaitingReply),
            Response::ClearInput,
            "the wait was stopped, and the second interrupt is the first idle one"
        );
        assert_eq!(i.consecutive(), 1);
        assert_eq!(i.on(Signal::Interrupt, Activity::Idle), Response::Exit);
    }

    #[test]
    fn ctrl_break_is_exactly_two_ctrl_c_and_not_a_special_case() {
        // Running the rule twice rather than short-circuiting means the two spellings cannot
        // drift apart as §22.3 grows.
        for activity in [
            Activity::Idle,
            Activity::Editing,
            Activity::Unsent,
            Activity::AwaitingReply,
            Activity::Blocked,
            Activity::Streaming,
            Activity::Batch,
        ] {
            let mut by_break = Interrupts::new();
            let brk = by_break.on(Signal::Break, activity);

            let mut by_pair = Interrupts::new();
            let first = by_pair.on(Signal::Interrupt, activity);
            let second = if first == Response::Exit {
                first
            } else {
                by_pair.on(Signal::Interrupt, Activity::Idle)
            };

            assert_eq!(brk, second, "{activity:?}");
            assert_eq!(
                by_break.consecutive(),
                by_pair.consecutive(),
                "{activity:?}"
            );
        }
    }

    #[test]
    fn an_interrupt_while_editing_never_exits_however_many_times() {
        // §22.3: clearing the line does not leave the REPL. Someone clearing a long command
        // three times must not be thrown out.
        let mut i = Interrupts::new();
        for _ in 0..5 {
            assert_eq!(
                i.on(Signal::Interrupt, Activity::Editing),
                Response::ClearInput
            );
        }
        assert_eq!(i.consecutive(), 0);
    }
}
