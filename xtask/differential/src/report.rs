//! The report, in appendix B.2's shape (v2.1 附录 B.2).
//!
//! B.2's template ships with `"execution_status": "NOT_RUN_IN_THIS_PLAN"` and says so on
//! purpose:
//!
//! > `NOT_RUN_IN_THIS_PLAN` 是刻意保留的真实性标记：本文件规划测试，不伪造产品实现或通过记录。
//!
//! This module is what replaces that marker with a measurement. Every field B.2 names is
//! present and filled from the run, plus the four layers §32.2 requires, because a template
//! that records a verdict without recording what produced it is the same fiction in a
//! different shape.

use crate::compare::Outcome;

/// One case, in B.2's shape plus the four layers.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CaseReport {
    /// `CMD-01` …
    pub case_id: String,
    /// The pinned `redis-cli` version this was compared against.
    pub baseline_cli: String,
    /// The digest both servers were started from.
    pub server_image_digest: String,
    /// `RESP2` or `RESP3`.
    pub protocol: String,
    /// The named fixture applied to both servers.
    pub initial_state_fixture: String,
    /// The command under test.
    pub request: Vec<String>,
    /// What the reply should be.
    pub expected_reply: String,
    /// §33's pass criterion.
    pub criterion: String,
    /// `PASS` or `FAIL` — B.2's `execution_status`, now carrying a measurement.
    pub execution_status: String,
    /// The four layers.
    pub layers: Outcome,
    /// What `redis-cli` did, as text.
    pub baseline_observed: Observed,
    /// What `prc` did, as text.
    pub penguin_observed: Observed,
}

/// One side's recorded behaviour, escaped so the report is losslessly readable.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Observed {
    /// The commands the client actually sent.
    pub sent: String,
    /// The bytes the server sent back.
    pub received: String,
    /// The client's stdout.
    pub stdout: String,
    /// The client's stderr.
    pub stderr: String,
    /// The client's exit code.
    pub exit: i32,
    /// State read back from that client's server.
    pub final_state: Vec<String>,
}

/// The whole run.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Report {
    /// Schema marker, so a future reader knows what they are holding.
    pub schema: String,
    /// When it ran.
    pub generated: String,
    /// The two server containers, named so it is visible they were two.
    pub servers: Vec<String>,
    /// One entry per case.
    pub cases: Vec<CaseReport>,
}

impl Report {
    /// Whether every case passed every layer.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.cases.iter().all(|c| c.layers.ok())
    }
}
