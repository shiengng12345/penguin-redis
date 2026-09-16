//! The boundary of a policy approval (v2.1 §29.4, ADR-024, R12, R46).
//!
//! §29.4 states it exactly, and the precision is the whole design:
//!
//! > 批准由可信的人类入口或已批准策略完成；**agent 不能自行创建自己的 approval**。
//! > 「已批准策略」的边界：策略型批准只能覆盖 `effects ∈ {reads_data}` 且 key 匹配 profile 中
//! > 显式列出的模式、单次结果 ≤ 预算、每小时 ≤ 次数上限的请求；策略本身由人在 profile 中编写
//! > 并带版本。任何 `writes_data`、`consumes`、`admin`、`unknown` 效果的请求，以及
//! > `redis.execute_approved_plan` 的**每一次**调用，都必须存在**由人签发**的 `ApprovalToken`
//! > （§23.2）且 `plan_hash` 匹配；策略不能签发这类令牌。
//!
//! Two things in there are easy to lose and are therefore each given their own named test:
//!
//! 1. **Every** call to `execute_approved_plan` needs a human token — including one whose plan
//!    contains nothing but reads. The plan is the unit being approved, not its steps.
//! 2. `unknown` is refused, not treated as "probably fine". An unclassified command is one the
//!    local catalog could not vouch for, and the one place an agent would reach for to get a
//!    write past a read-only policy.

use pr_core::{CommandRequest, Effects};

/// A policy rule, written by a person in a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRule {
    /// Identifier, for audit.
    pub id: String,
    /// The rule's version. §29.4 requires policies to be versioned: an approval traceable to
    /// "the policy" is not traceable to anything once the policy is edited.
    pub version: u32,
    /// Key patterns a person listed explicitly. An empty list authorises nothing — not
    /// everything, which is the direction an empty list fails in most configuration systems.
    pub key_patterns: Vec<String>,
    /// Largest single result, in bytes.
    pub max_result_bytes: usize,
    /// Calls per hour.
    pub max_calls_per_hour: u32,
}

/// Why a policy declined to authorise something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyRefusal {
    /// The command does more than read.
    NotReadOnly {
        /// Which effects took it outside the boundary.
        offending: &'static str,
    },
    /// The local catalog does not classify the command.
    Unclassified,
    /// The rule lists no key pattern, so it authorises nothing.
    NoKeyPatterns,
    /// A key is not covered by any listed pattern.
    KeyNotListed(String),
    /// The request touches no key, so no pattern can vouch for it.
    NoKeys,
    /// The estimated result is over the rule's budget.
    ResultTooLarge {
        /// Estimated bytes.
        estimated: usize,
        /// The rule's limit.
        limit: usize,
    },
    /// The hourly call budget is spent.
    RateLimited {
        /// Calls already made in the window.
        used: u32,
        /// The rule's limit.
        limit: u32,
    },
    /// `execute_approved_plan` was called. A policy may never authorise one.
    PlanExecutionIsAlwaysHuman,
}

impl PolicyRefusal {
    /// A message an agent host can show, which says what a human would have to do.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotReadOnly { offending } => format!(
                "a policy approval covers read-only requests; this one is {offending}. A person \
                 must approve it (§29.4)"
            ),
            Self::Unclassified => "the local catalog does not classify this command, so its \
                 effects are unknown; a policy cannot vouch for it (ADR-030)"
                .to_owned(),
            Self::NoKeyPatterns => "this policy rule lists no key pattern, so it authorises \
                 nothing"
                .to_owned(),
            Self::KeyNotListed(k) => {
                format!("{k} is not covered by a key pattern this policy lists")
            }
            Self::NoKeys => "this request names no key, so no key pattern can cover it".to_owned(),
            Self::ResultTooLarge { estimated, limit } => format!(
                "the estimated result is {estimated} bytes and this policy's limit is {limit}"
            ),
            Self::RateLimited { used, limit } => {
                format!("this policy allows {limit} calls per hour and {used} have been made")
            }
            Self::PlanExecutionIsAlwaysHuman => {
                "every execute_approved_plan call needs a human-signed approval token whose \
                 plan_hash matches, even when the plan only reads (§29.4)"
                    .to_owned()
            }
        }
    }
}

/// Which effect, if any, puts a request outside a policy's reach.
///
/// Returns the *first* one in a fixed order so the message is deterministic; a request that is
/// both a write and admin is reported as a write, which is the part an operator acts on.
#[must_use]
fn beyond_read_only(e: Effects) -> Option<&'static str> {
    if e.unknown {
        return Some("unclassified");
    }
    if e.writes_data {
        return Some("a write");
    }
    if e.destructive {
        return Some("destructive");
    }
    if e.consumes {
        return Some("a consuming read");
    }
    if e.admin {
        return Some("administrative");
    }
    if e.blocks {
        // Not in §29.4's list, and refused anyway: a blocking command held open by an agent
        // occupies a connection for as long as it likes, which is a resource decision a person
        // should make. Refusing is the conservative direction and costs an approval, not data.
        return Some("potentially blocking");
    }
    None
}

impl PolicyRule {
    /// Can this rule authorise `req`?
    ///
    /// `keys` are the key arguments the catalog extracted, `estimated_bytes` is the result
    /// budget check, and `calls_in_last_hour` is the rate window.
    ///
    /// # Errors
    /// The specific [`PolicyRefusal`], so the agent host can show a person what to approve.
    pub fn may_authorise(
        &self,
        req: &CommandRequest,
        keys: &[Vec<u8>],
        estimated_bytes: usize,
        calls_in_last_hour: u32,
    ) -> Result<(), PolicyRefusal> {
        if req.effects.unknown {
            return Err(PolicyRefusal::Unclassified);
        }
        if let Some(offending) = beyond_read_only(req.effects) {
            return Err(PolicyRefusal::NotReadOnly { offending });
        }
        if !req.effects.reads_data {
            // §29.4 says `effects ∈ {reads_data}` — a command that reads nothing is not in
            // that set either, even though it is harmless-looking. `PING` does not need a
            // policy approval; it needs no approval.
            return Err(PolicyRefusal::NotReadOnly {
                offending: "not a data read",
            });
        }
        if self.key_patterns.is_empty() {
            return Err(PolicyRefusal::NoKeyPatterns);
        }
        if keys.is_empty() {
            return Err(PolicyRefusal::NoKeys);
        }
        for k in keys {
            let key = String::from_utf8_lossy(k).into_owned();
            if !self.key_patterns.iter().any(|p| glob_match(p, &key)) {
                return Err(PolicyRefusal::KeyNotListed(key));
            }
        }
        if estimated_bytes > self.max_result_bytes {
            return Err(PolicyRefusal::ResultTooLarge {
                estimated: estimated_bytes,
                limit: self.max_result_bytes,
            });
        }
        if calls_in_last_hour >= self.max_calls_per_hour {
            return Err(PolicyRefusal::RateLimited {
                used: calls_in_last_hour,
                limit: self.max_calls_per_hour,
            });
        }
        Ok(())
    }

    /// A policy may never authorise `redis.execute_approved_plan`.
    ///
    /// Its own function rather than a branch inside [`Self::may_authorise`], because the rule
    /// is unconditional: it does not depend on what the plan contains, and expressing it as a
    /// condition invites someone to add "unless the plan is read-only".
    ///
    /// # Errors
    /// Always [`PolicyRefusal::PlanExecutionIsAlwaysHuman`].
    pub fn may_authorise_plan_execution(&self) -> Result<std::convert::Infallible, PolicyRefusal> {
        Err(PolicyRefusal::PlanExecutionIsAlwaysHuman)
    }
}

/// Redis glob matching, as `KEYS`/`SCAN MATCH` define it.
///
/// Implemented here rather than borrowed from a generic glob crate because the semantics have
/// to be Redis's: a pattern in a profile means what the same pattern would mean to the server,
/// or an operator writing `user:*` has authorised a different set than they think.
#[must_use]
pub fn glob_match(pattern: &str, key: &str) -> bool {
    glob_bytes(pattern.as_bytes(), key.as_bytes())
}

fn glob_bytes(p: &[u8], k: &[u8]) -> bool {
    let (mut pi, mut ki) = (0usize, 0usize);
    let (mut star, mut star_k) = (usize::MAX, 0usize);
    while ki < k.len() {
        match p.get(pi) {
            Some(b'*') => {
                star = pi;
                star_k = ki;
                pi += 1;
            }
            Some(b'?') => {
                pi += 1;
                ki += 1;
            }
            Some(b'[') => match class(p, pi, k[ki]) {
                Some((next, true)) => {
                    pi = next;
                    ki += 1;
                }
                Some((_, false)) | None => {
                    if star == usize::MAX {
                        return false;
                    }
                    star_k += 1;
                    ki = star_k;
                    pi = star + 1;
                }
            },
            Some(b'\\') if pi + 1 < p.len() && p[pi + 1] == k[ki] => {
                pi += 2;
                ki += 1;
            }
            Some(&c) if c == k[ki] => {
                pi += 1;
                ki += 1;
            }
            _ => {
                if star == usize::MAX {
                    return false;
                }
                star_k += 1;
                ki = star_k;
                pi = star + 1;
            }
        }
    }
    while p.get(pi) == Some(&b'*') {
        pi += 1;
    }
    pi == p.len()
}

/// Match one `[...]` class, returning the index after it and whether `c` matched.
fn class(p: &[u8], at: usize, c: u8) -> Option<(usize, bool)> {
    let mut i = at + 1;
    let negate = p.get(i) == Some(&b'^');
    if negate {
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    while i < p.len() && (p[i] != b']' || first) {
        first = false;
        if p[i] == b'\\' && i + 1 < p.len() {
            i += 1;
            if p[i] == c {
                hit = true;
            }
        } else if i + 2 < p.len() && p[i + 1] == b'-' && p[i + 2] != b']' {
            let (lo, hi) = (p[i], p[i + 2]);
            if (lo..=hi).contains(&c) {
                hit = true;
            }
            i += 2;
        } else if p[i] == c {
            hit = true;
        }
        i += 1;
    }
    if i >= p.len() {
        // Unterminated class: `[` is a literal, as Redis treats it.
        return None;
    }
    Some((i + 1, hit != negate))
}
