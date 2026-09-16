//! Explicit key discovery: two match modes, a budget, and an honest completeness report
//! (v2.1 §12.4, §12.5, R08, R09, R32, V-F07).
//!
//! Three rules shape this module, and each of them exists because the obvious implementation
//! gets it wrong:
//!
//! 1. **Two match modes that never mix.** §12.4 gives them a table. A literal prefix escapes
//!    the user's bytes and appends `*`; a raw glob sends the bytes untouched. `player:[` means
//!    two different things in the two modes, and 「不存在「无声变成另一个匹配条件」的路径」.
//! 2. **`SCAN` guarantees almost nothing.** `COUNT` is a hint, a page may be empty while the
//!    cursor is non-zero, the same element may come back twice, and there is no snapshot
//!    [R08]. A discovery that stops on an empty page is wrong; one that reports a percentage
//!    from the cursor value is inventing it.
//! 3. **Running out of budget is not an answer.** §12.4 is explicit: show
//!    `No matches in the observed portion; discovery incomplete`, **not**
//!    `key does not exist`. The client's budget says nothing about what is on the server.

use std::time::{Duration, Instant};

/// How the user's input becomes a `MATCH` pattern (§12.4's table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode {
    /// Automatic candidates, F6, `:complete keys --prefix <bytes>`.
    ///
    /// The user typed a prefix, not a pattern. Their `[` is a `[`.
    LiteralPrefix,
    /// `:complete keys --match <pattern>`, `:bulk … --match`, raw `SCAN … MATCH`.
    ///
    /// The user typed a pattern. Their `[` is a character class, and whether it is well formed
    /// is the server's judgement, not ours.
    RawGlob,
}

impl MatchMode {
    /// The label the UI shows. §12.4 requires the two to be distinguishable on screen.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LiteralPrefix => "prefix",
            Self::RawGlob => "glob",
        }
    }
}

/// Turn the user's bytes into the `MATCH` argument.
///
/// Byte-wise, not char-wise: a key is bytes, and a multi-byte sequence whose continuation
/// bytes happen to equal `0x5b` does not exist in UTF-8 — but a key need not be UTF-8 at all,
/// and escaping the raw bytes is correct either way.
#[must_use]
pub fn scan_pattern(mode: MatchMode, input: &[u8]) -> Vec<u8> {
    match mode {
        MatchMode::RawGlob => input.to_vec(),
        MatchMode::LiteralPrefix => {
            let mut out = Vec::with_capacity(input.len() + 8);
            for &b in input {
                if matches!(b, b'*' | b'?' | b'[' | b']' | b'\\') {
                    out.push(b'\\');
                }
                out.push(b);
            }
            out.push(b'*');
            out
        }
    }
}

/// What a plan preview shows for a literal prefix (§12.4: 「同时显示原字节与转义结果」).
///
/// Both, always. Showing only the escaped form hides what the user typed; showing only what
/// they typed hides what will be sent.
#[must_use]
pub fn preview(mode: MatchMode, input: &[u8]) -> String {
    let sent = scan_pattern(mode, input);
    format!(
        "{}: typed {} → MATCH {}",
        mode.label(),
        escape(input),
        escape(&sent)
    )
}

fn escape(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() + 2);
    for &c in b {
        if let 0x20..=0x7e = c {
            s.push(c as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(s, "\\x{c:02x}");
        }
    }
    s
}

/// One discovery run's limits (§12.5).
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Maximum `SCAN`-class requests.
    pub max_scans: u32,
    /// `COUNT` hint. A hint, not a page size [R08].
    pub count_hint: u64,
    /// Wall-clock ceiling.
    pub max_duration: Duration,
    /// Requests per second.
    pub max_requests_per_sec: u32,
    /// Bytes of reply this run will accept before stopping.
    pub max_response_bytes: usize,
}

impl Budget {
    /// §12.5's explicit default: F6, `:complete`, palette Discover.
    ///
    /// 「每项可在 profile 中调高」 — these are defaults, not limits of the design, and V-F07
    /// measured them against 10^6 keys rather than assuming them.
    ///
    /// **`max_scans` is 200, not §12.5's initial 50 (ADR-033).** Measured: with 50, a sparse
    /// prefix over a million keys stopped after 2.58 s of a 10 s budget — the scan count was
    /// pre-empting the time budget, so the limit that actually bit was not the one the user
    /// experiences. 20 requests/second for 10 seconds is 200 requests; setting the scan count
    /// to match makes the two budgets describe the same stopping point. The rate and the
    /// duration are unchanged, so the load this puts on a server is unchanged too.
    #[must_use]
    pub fn explicit_default() -> Self {
        Self {
            max_scans: 200,
            count_hint: 1000,
            max_duration: Duration::from_secs(10),
            max_requests_per_sec: 20,
            max_response_bytes: 1024 * 1024,
        }
    }

    /// §12.5's automatic default, for a dev session that authorised it. Deliberately tiny.
    #[must_use]
    pub fn automatic_default() -> Self {
        Self {
            max_scans: 2,
            count_hint: 100,
            max_duration: Duration::from_secs(1),
            max_requests_per_sec: 2,
            max_response_bytes: 256 * 1024,
        }
    }
}

/// Why a run stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Completeness {
    /// The cursor came back to 0: the whole keyspace was walked.
    ///
    /// Still not a snapshot — elements added or removed during the walk may have been missed
    /// or seen twice [R08] — but it is the only case where "no matches" means something.
    Exhausted,
    /// A budget was reached. Says which, because "try again" and "raise the limit" are
    /// different advice.
    BudgetReached {
        /// Which limit stopped it.
        limit: &'static str,
    },
    /// The user cancelled.
    Cancelled,
    /// The server refused. Discovery backs off rather than retrying into an ACL wall.
    Denied {
        /// The server's message.
        reason: String,
    },
}

impl Completeness {
    /// Whether "no matches" from this run means the keys are not there.
    ///
    /// Only [`Self::Exhausted`]. Everything else means we stopped looking.
    #[must_use]
    pub fn is_conclusive(&self) -> bool {
        matches!(self, Self::Exhausted)
    }

    /// The line shown with an empty result.
    ///
    /// §12.4: 「不能显示 `key does not exist`」. The client's budget is not evidence about the
    /// server's contents, and a user told "does not exist" will stop looking.
    #[must_use]
    pub fn empty_message(&self) -> String {
        match self {
            Self::Exhausted => {
                "No matches. The keyspace was walked to the end, though SCAN offers no \
                 snapshot: a key added or removed during the walk may not be reflected"
                    .to_owned()
            }
            Self::BudgetReached { limit } => format!(
                "No matches in the observed portion; discovery incomplete (stopped at the \
                 {limit} budget)"
            ),
            Self::Cancelled => {
                "No matches in the observed portion; discovery incomplete (cancelled)".to_owned()
            }
            Self::Denied { reason } => format!(
                "Discovery was refused by the server ({reason}); nothing can be concluded \
                 about which keys exist"
            ),
        }
    }
}

/// What to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Issue this `SCAN`.
    Scan {
        /// Cursor to send.
        cursor: u64,
        /// `MATCH` argument, already in the right mode.
        pattern: Vec<u8>,
        /// `COUNT` hint.
        count: u64,
    },
    /// Wait before the next request, to stay inside the rate budget.
    Wait(Duration),
    /// Stop.
    Done(Completeness),
}

/// Progress, for the UI §12.5 requires ("已扫 cursor 次数、已匹配数、剩余预算").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// `SCAN` calls issued.
    pub scans_used: u32,
    /// `SCAN` calls left in the budget.
    pub scans_left: u32,
    /// Distinct keys matched so far.
    pub matched: usize,
    /// Keys the server returned, including duplicates.
    pub keys_seen: u64,
    /// Reply bytes accounted for.
    pub bytes_seen: usize,
}

/// A single explicit discovery run.
///
/// A state machine rather than a loop, so the caller owns the I/O, the cancellation and the
/// clock — and so the whole of the budget logic can be tested without a server.
#[derive(Debug)]
pub struct Discovery {
    pattern: Vec<u8>,
    mode: MatchMode,
    budget: Budget,
    cursor: u64,
    scans_used: u32,
    keys_seen: u64,
    bytes_seen: usize,
    matches: Vec<Vec<u8>>,
    seen: std::collections::HashSet<Vec<u8>>,
    started: Option<Instant>,
    last_request: Option<Instant>,
    finished: Option<Completeness>,
}

impl Discovery {
    /// Start a run for `input` in `mode`.
    #[must_use]
    pub fn new(mode: MatchMode, input: &[u8], budget: Budget) -> Self {
        Self {
            pattern: scan_pattern(mode, input),
            mode,
            budget,
            cursor: 0,
            scans_used: 0,
            keys_seen: 0,
            bytes_seen: 0,
            matches: Vec::new(),
            seen: std::collections::HashSet::new(),
            started: None,
            last_request: None,
            finished: None,
        }
    }

    /// The `MATCH` bytes this run will send.
    #[must_use]
    pub fn pattern(&self) -> &[u8] {
        &self.pattern
    }

    /// The mode, for the UI label.
    #[must_use]
    pub fn mode(&self) -> MatchMode {
        self.mode
    }

    /// Current progress.
    #[must_use]
    pub fn progress(&self) -> Progress {
        Progress {
            scans_used: self.scans_used,
            scans_left: self.budget.max_scans.saturating_sub(self.scans_used),
            matched: self.matches.len(),
            keys_seen: self.keys_seen,
            bytes_seen: self.bytes_seen,
        }
    }

    /// What to do at `now`.
    pub fn next_step(&mut self, now: Instant) -> Step {
        if let Some(f) = &self.finished {
            return Step::Done(f.clone());
        }
        let started = *self.started.get_or_insert(now);

        if now.duration_since(started) >= self.budget.max_duration {
            return self.finish(Completeness::BudgetReached { limit: "time" });
        }
        if self.scans_used >= self.budget.max_scans {
            return self.finish(Completeness::BudgetReached {
                limit: "SCAN count",
            });
        }
        if self.bytes_seen >= self.budget.max_response_bytes {
            return self.finish(Completeness::BudgetReached {
                limit: "response size",
            });
        }

        // Rate limiting is a wait, not a failure: the run is still within its other budgets.
        if let Some(last) = self.last_request {
            let min_gap = Duration::from_secs(1) / self.budget.max_requests_per_sec;
            let since = now.duration_since(last);
            // `checked_sub` rather than `-`: `since < min_gap` already rules out an
            // underflow, but a clock that is not monotonic on some platform would make that
            // reasoning wrong, and a panic in a rate limiter is a poor trade for a saved
            // branch.
            if let Some(wait) = min_gap.checked_sub(since)
                && !wait.is_zero()
            {
                return Step::Wait(wait);
            }
        }

        self.last_request = Some(now);
        self.scans_used += 1;
        Step::Scan {
            cursor: self.cursor,
            pattern: self.pattern.clone(),
            count: self.budget.count_hint,
        }
    }

    /// Record one `SCAN` reply.
    ///
    /// A cursor of 0 ends the run as [`Completeness::Exhausted`]; anything else continues,
    /// **including an empty page**. §12.4 and [R08]: 「空页非零 cursor」 is normal, and a client
    /// that stops there reports "not found" for a key that is there.
    pub fn feed(&mut self, cursor: u64, keys: &[Vec<u8>], reply_bytes: usize) {
        self.keys_seen += keys.len() as u64;
        self.bytes_seen += reply_bytes;
        for k in keys {
            // SCAN may return the same element more than once [R08]; the user should not see
            // it twice, and the matched count should not double.
            if self.seen.insert(k.clone()) {
                self.matches.push(k.clone());
            }
        }
        self.cursor = cursor;
        if cursor == 0 {
            self.finished = Some(Completeness::Exhausted);
        }
    }

    /// The user cancelled.
    pub fn cancel(&mut self) {
        if self.finished.is_none() {
            self.finished = Some(Completeness::Cancelled);
        }
    }

    /// The server refused (`NOPERM`, or any other error).
    pub fn deny(&mut self, reason: impl Into<String>) {
        self.finished = Some(Completeness::Denied {
            reason: reason.into(),
        });
    }

    /// The result so far, with how much of the keyspace it speaks for.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        Outcome {
            matches: self.matches.clone(),
            completeness: self
                .finished
                .clone()
                .unwrap_or(Completeness::BudgetReached {
                    limit: "unfinished",
                }),
            progress: self.progress(),
        }
    }

    fn finish(&mut self, c: Completeness) -> Step {
        self.finished = Some(c.clone());
        Step::Done(c)
    }
}

/// What a run produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Distinct matches, in the order the server first returned them.
    pub matches: Vec<Vec<u8>>,
    /// How much of the keyspace this speaks for.
    pub completeness: Completeness,
    /// Final counters.
    pub progress: Progress,
}

impl Outcome {
    /// The line to show a user.
    ///
    /// Never claims a key does not exist unless the keyspace was actually walked.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.matches.is_empty() {
            return self.completeness.empty_message();
        }
        let n = self.matches.len();
        match &self.completeness {
            Completeness::Exhausted => format!("{n} match(es); the keyspace was walked to the end"),
            Completeness::BudgetReached { limit } => format!(
                "{n} match(es) in the observed portion; discovery incomplete (stopped at the \
                 {limit} budget after {} SCANs)",
                self.progress.scans_used
            ),
            Completeness::Cancelled => {
                format!("{n} match(es) in the observed portion; discovery incomplete (cancelled)")
            }
            Completeness::Denied { reason } => {
                format!("{n} match(es) before the server refused ({reason})")
            }
        }
    }
}

/// Back-off after a refusal (§12.4, ASSIST-084: NOPERM 冷却).
///
/// A discovery that retries into an ACL wall turns one refusal into a stream of them, which is
/// noise in the server's log and, on some deployments, a lockout. The cooldown is per
/// (profile, database) because that is the scope an ACL rule has.
#[derive(Debug, Default)]
pub struct Cooldown {
    until: std::collections::HashMap<(String, u32), Instant>,
}

/// How long discovery stays quiet after a refusal.
pub const COOLDOWN: Duration = Duration::from_mins(1);

impl Cooldown {
    /// A cooldown table with nothing in it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a refusal.
    pub fn refused(&mut self, profile: &str, db: u32, now: Instant) {
        self.until.insert((profile.to_owned(), db), now + COOLDOWN);
    }

    /// May discovery run now?
    ///
    /// Returns the remaining wait when it may not, so the UI can say how long rather than
    /// only that.
    ///
    /// # Errors
    /// The remaining cooldown.
    pub fn check(&self, profile: &str, db: u32, now: Instant) -> Result<(), Duration> {
        match self.until.get(&(profile.to_owned(), db)) {
            Some(t) if *t > now => Err(*t - now),
            _ => Ok(()),
        }
    }

    /// A manual retry clears it: the user may know the ACL changed.
    ///
    /// The cooldown exists to stop *automatic* retries, not to argue with a person.
    pub fn cleared_by_user(&mut self, profile: &str, db: u32) {
        self.until.remove(&(profile.to_owned(), db));
    }
}
