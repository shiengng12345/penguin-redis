//! The restricted tool surface (v2.1 §29.4).
//!
//! > 工具面设计为 `connections.list_safe`、`redis.read`、`redis.inspect`、
//! > `redis.scan_limited`、`redis.diagnose`、`redis.plan_write`、
//! > `redis.execute_approved_plan` 等受限能力。**不能默认暴露「任意命令 + 任意凭证 +
//! > 无限制扫描」。**
//!
//! The list is closed. An `execute` tool taking arbitrary argv is not missing from it — it is
//! the thing it exists instead of.

use crate::policy::{PolicyRefusal, PolicyRule};
use pr_core::CommandRequest;
use pr_security::approval::{ApprovalRefusal, ApprovalToken, ExecutionContext, Issuer};

/// The tools an MCP host may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Profiles the agent may see, with secrets and endpoints withheld.
    ConnectionsListSafe,
    /// A read of named keys.
    RedisRead,
    /// Type, length and TTL metadata for named keys.
    RedisInspect,
    /// A bounded `SCAN`, never an unbounded sweep.
    RedisScanLimited,
    /// Read-only diagnostics.
    RedisDiagnose,
    /// Produce a write plan for a person to look at. Produces, never runs.
    RedisPlanWrite,
    /// Execute a plan a person already approved.
    RedisExecuteApprovedPlan,
}

impl Tool {
    /// The wire name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ConnectionsListSafe => "connections.list_safe",
            Self::RedisRead => "redis.read",
            Self::RedisInspect => "redis.inspect",
            Self::RedisScanLimited => "redis.scan_limited",
            Self::RedisDiagnose => "redis.diagnose",
            Self::RedisPlanWrite => "redis.plan_write",
            Self::RedisExecuteApprovedPlan => "redis.execute_approved_plan",
        }
    }

    /// Parse a wire name. Unknown names are not tools, which is the point of a closed list.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        ALL.iter().copied().find(|t| t.name() == name)
    }

    /// One line for `tools/list`.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::ConnectionsListSafe => {
                "List profiles the agent may use. Endpoints and secrets are withheld."
            }
            Self::RedisRead => {
                "Read named keys. Covered by a policy approval only when every \
                 key matches a pattern the profile lists."
            }
            Self::RedisInspect => "Type, length and TTL for named keys. Does not read values.",
            Self::RedisScanLimited => "Scan with an explicit bound. There is no unbounded sweep.",
            Self::RedisDiagnose => "Read-only diagnostics about the connection and server.",
            Self::RedisPlanWrite => {
                "Produce a write plan and its plan_hash for a person to review. Runs nothing."
            }
            Self::RedisExecuteApprovedPlan => {
                "Execute a plan a person approved. Every call requires a human-signed approval \
                 token whose plan_hash matches."
            }
        }
    }

    /// Whether this tool may be reached by a policy approval at all.
    ///
    /// `plan_write` produces text; `execute_approved_plan` needs a person every time (§29.4).
    #[must_use]
    pub fn policy_may_reach(self) -> bool {
        matches!(
            self,
            Self::ConnectionsListSafe
                | Self::RedisRead
                | Self::RedisInspect
                | Self::RedisScanLimited
                | Self::RedisDiagnose
        )
    }
}

/// Every tool, in the order `tools/list` reports them.
pub const ALL: &[Tool] = &[
    Tool::ConnectionsListSafe,
    Tool::RedisRead,
    Tool::RedisInspect,
    Tool::RedisScanLimited,
    Tool::RedisDiagnose,
    Tool::RedisPlanWrite,
    Tool::RedisExecuteApprovedPlan,
];

/// How a call was authorised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Authorisation {
    /// Needs nothing: the tool reads no data from the server.
    NotRequired,
    /// A policy rule covered it.
    ByPolicy {
        /// Rule id.
        rule: String,
        /// Rule version, so an audit line survives the rule being edited.
        version: u32,
    },
    /// A person signed a token for it.
    ByHuman,
}

/// Why a tool call was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolRefusal {
    /// No such tool. The list is closed on purpose.
    UnknownTool(String),
    /// A policy declined, and no human token was offered.
    NeedsApproval {
        /// Why the policy could not cover it.
        policy: PolicyRefusal,
    },
    /// A token was offered but did not authorise this request.
    TokenRefused(ApprovalRefusal),
    /// A policy-issued token was offered where only a person may authorise.
    TokenIsNotHuman,
    /// `execute_approved_plan` was called with a token whose plan hash is for another plan.
    PlanHashMismatch {
        /// What the token authorises.
        expected: String,
        /// What was asked for.
        got: String,
    },
    /// `execute_approved_plan` was called with no token at all.
    PlanNeedsHumanToken,
}

impl ToolRefusal {
    /// A message for the agent host, phrased as what a person would have to do.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::UnknownTool(n) => format!(
                "{n} is not a tool. Penguin exposes a closed list; there is no arbitrary-command \
                 tool (§29.4)"
            ),
            Self::NeedsApproval { policy } => {
                format!("a person must approve this: {}", policy.message())
            }
            Self::TokenRefused(r) => format!("the approval token does not authorise this: {r:?}"),
            Self::TokenIsNotHuman => "this request needs an approval a person signed; a policy \
                 cannot issue one (§29.4)"
                .to_owned(),
            Self::PlanHashMismatch { expected, got } => format!(
                "the approval is for plan {expected} and this call is for {got}; approving a \
                 plan does not approve a different one"
            ),
            Self::PlanNeedsHumanToken => {
                "every execute_approved_plan call needs a human-signed token whose plan_hash \
                 matches, even when the plan only reads (§29.4)"
                    .to_owned()
            }
        }
    }
}

/// Decide whether one tool call may proceed.
///
/// This is the whole of V-D08's "写类请求无人签令牌被拒" criterion, in one place so there is
/// one place to read and one place to test.
///
/// # Errors
/// The specific [`ToolRefusal`].
pub fn authorise(
    tool: Tool,
    req: Option<&CommandRequest>,
    keys: &[Vec<u8>],
    estimated_bytes: usize,
    calls_in_last_hour: u32,
    rule: Option<&PolicyRule>,
    token: Option<&ApprovalToken>,
    ctx: &ExecutionContext,
) -> Result<Authorisation, ToolRefusal> {
    // `execute_approved_plan` first, and unconditionally. Putting it anywhere else would make
    // it look like a case among cases, and someone would eventually add "unless the plan is
    // read-only" — which §29.4 rules out in the same sentence that creates the tool.
    if tool == Tool::RedisExecuteApprovedPlan {
        let Some(t) = token else {
            return Err(ToolRefusal::PlanNeedsHumanToken);
        };
        if !matches!(t.issued_by, Issuer::Human) {
            return Err(ToolRefusal::TokenIsNotHuman);
        }
        let Some(req) = req else {
            return Err(ToolRefusal::PlanNeedsHumanToken);
        };
        t.authorise(req, ctx).map_err(ToolRefusal::TokenRefused)?;
        return Ok(Authorisation::ByHuman);
    }

    // A tool that reads nothing from the server needs no approval of any kind.
    let Some(req) = req else {
        return Ok(Authorisation::NotRequired);
    };

    // A human token, when present, decides. It is the strongest thing on offer.
    if let Some(t) = token {
        if matches!(t.issued_by, Issuer::Human) {
            t.authorise(req, ctx).map_err(ToolRefusal::TokenRefused)?;
            return Ok(Authorisation::ByHuman);
        }
        // A policy-issued token is not a shortcut past the policy check: it has to pass the
        // same boundary the policy would have applied, below.
    }

    if !tool.policy_may_reach() {
        return Err(ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::NotReadOnly {
                offending: "not a tool a policy can reach",
            },
        });
    }

    let Some(rule) = rule else {
        return Err(ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::NoKeyPatterns,
        });
    };
    rule.may_authorise(req, keys, estimated_bytes, calls_in_last_hour)
        .map_err(|policy| ToolRefusal::NeedsApproval { policy })?;
    Ok(Authorisation::ByPolicy {
        rule: rule.id.clone(),
        version: rule.version,
    })
}
