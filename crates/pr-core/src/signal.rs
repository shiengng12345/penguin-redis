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

/// Getting an interrupt from the operating system into [`Interrupts`] (§35.1, V-C07).
///
/// Everything above this point is platform-free on purpose: Ctrl+C means the same thing
/// whatever delivered it. This is the part that does not.
///
/// On Unix, Ctrl+C arrives as a keystroke in raw mode and the terminal layer already sees it;
/// there is nothing to install. On Windows, `CTRL_C_EVENT` and `CTRL_BREAK_EVENT` are delivered
/// by the console to a registered handler on a thread the OS creates, which is why this needs a
/// counter rather than a callback: a handler must set a flag and return, because whatever it
/// calls runs concurrently with every other thread in the process.
///
/// §35.1 defines Ctrl+Break as two Ctrl+C. That is applied **here**, at delivery, so the
/// meaning layer keeps one rule rather than two: `pending()` returns a count of Ctrl+C
/// equivalents, and a Break contributes two of them.
pub mod delivery {
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Ctrl+C equivalents that have arrived and not yet been taken.
    static PENDING: AtomicU32 = AtomicU32::new(0);

    /// What one console event is worth, in Ctrl+C equivalents.
    ///
    /// A free function so the mapping is testable on every platform, not only where the console
    /// exists. `0` is `CTRL_C_EVENT` and `1` is `CTRL_BREAK_EVENT`; the rest (close, logoff,
    /// shutdown) are not interrupts and are left for the default handler.
    #[must_use]
    pub fn weight_of_event(ctrl_type: u32) -> Option<u32> {
        match ctrl_type {
            0 => Some(1),
            // §35.1: 「Ctrl+Break 等价于两次 Ctrl+C」. Two, at the door, so nothing downstream
            // has to special-case it — the same reasoning as running the rule twice rather
            // than giving Break its own branch in `Interrupts::on`.
            1 => Some(2),
            _ => None,
        }
    }

    /// Record a console event. Returns whether it was one this owns.
    pub fn deliver(ctrl_type: u32) -> bool {
        match weight_of_event(ctrl_type) {
            Some(n) => {
                PENDING.fetch_add(n, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    /// Take everything that has arrived, leaving the counter at zero.
    pub fn take_pending() -> u32 {
        PENDING.swap(0, Ordering::SeqCst)
    }

    /// How many are waiting, without taking them.
    #[must_use]
    pub fn pending() -> u32 {
        PENDING.load(Ordering::SeqCst)
    }

    #[cfg(windows)]
    #[allow(unsafe_code)]
    mod windows_console {
        //! The one `unsafe` in this crate, and the reason it is here rather than behind a
        //! wrapper crate: the whole binding is four lines, and a dependency whose only job is
        //! to call one function is a supply-chain decision made to avoid writing four lines.
        //!
        //! The handler runs on a thread the console creates, concurrently with everything else.
        //! It does exactly one thing — an atomic add — and returns. Anything else (locking,
        //! allocating, printing) can deadlock against a thread the console has just suspended.

        // `BOOL` is `windows_sys::core::BOOL`, not `Win32::Foundation::BOOL` — `TRUE` and
        // `FALSE` live under `Foundation` but the type they are does not, and importing it
        // from the obvious place compiles nowhere but fails only on Windows.
        use windows_sys::Win32::Foundation::{FALSE, TRUE};
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
        use windows_sys::core::BOOL;

        /// SAFETY: called by the console on its own thread. The body touches one `AtomicU32`
        /// and nothing else, so it is safe to run at any point in any other thread's execution.
        unsafe extern "system" fn handler(ctrl_type: u32) -> BOOL {
            if super::deliver(ctrl_type) {
                TRUE
            } else {
                FALSE
            }
        }

        /// Register the handler.
        ///
        /// # Errors
        /// The OS error, if the console refuses.
        pub fn install() -> std::io::Result<()> {
            // SAFETY: `handler` has the signature `PHANDLER_ROUTINE` requires and a `'static`
            // lifetime; `TRUE` adds rather than removes. The only failure mode is the console
            // refusing, which is reported rather than ignored.
            let ok = unsafe { SetConsoleCtrlHandler(Some(handler), TRUE) };
            if ok == FALSE {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }
    }

    /// Install the OS-level handler.
    ///
    /// On Windows this registers a console control handler. Everywhere else there is nothing to
    /// install — Ctrl+C is a keystroke the terminal layer already reads — and this succeeds
    /// without doing anything, so callers have one code path.
    ///
    /// # Errors
    /// On Windows, the OS error if the console refuses the registration.
    pub fn install() -> std::io::Result<()> {
        #[cfg(windows)]
        {
            windows_console::install()
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;

        #[test]
        fn a_break_is_worth_two_interrupts_at_the_door() {
            // §35.1's rule, applied where the event arrives so nothing downstream repeats it.
            assert_eq!(weight_of_event(0), Some(1));
            assert_eq!(weight_of_event(1), Some(2));
        }

        #[test]
        fn close_logoff_and_shutdown_are_not_interrupts() {
            // CTRL_CLOSE_EVENT / CTRL_LOGOFF_EVENT / CTRL_SHUTDOWN_EVENT. Claiming them would
            // mean the process says it handled a shutdown it did not handle.
            for t in [2u32, 5, 6, 99] {
                assert_eq!(weight_of_event(t), None, "{t}");
            }
        }

        #[test]
        fn installing_is_a_no_op_off_windows_and_succeeds_on_it() {
            install().expect("installing a console handler must not fail");
        }

        #[test]
        fn delivery_accumulates_and_take_drains() {
            // Serialised against the other tests by taking first: `PENDING` is process-wide,
            // which is what it has to be, so a test that assumes it starts at zero is a test
            // that fails when the suite is run in parallel.
            let _ = take_pending();
            assert!(deliver(0));
            assert!(deliver(1));
            assert!(!deliver(2));
            assert_eq!(take_pending(), 3);
            assert_eq!(pending(), 0);
        }
    }
}
