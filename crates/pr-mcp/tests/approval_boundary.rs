//! V-D08 — the policy approval boundary (v2.1 §29.4, §23.2, ADR-024, R12, R46).
//!
//! > 批准由可信的人类入口或已批准策略完成；**agent 不能自行创建自己的 approval**。
//! > 「已批准策略」的边界：策略型批准只能覆盖 `effects ∈ {reads_data}` 且 key 匹配 profile 中
//! > 显式列出的模式、单次结果 ≤ 预算、每小时 ≤ 次数上限的请求……任何 `writes_data`、
//! > `consumes`、`admin`、`unknown` 效果的请求，以及 `redis.execute_approved_plan` 的**每一次**
//! > 调用，都必须存在**由人签发**的 `ApprovalToken` 且 `plan_hash` 匹配；策略不能签发这类令牌。
//!
//! The pass criterion V-D08 states — 写类请求无人签令牌被拒 — is the headline, and the ways
//! around it are what the rest of this file is about. An agent that wants a write executed
//! does not send `SET`; it sends something the classifier is unsure about, or a plan whose
//! steps look harmless, or the same approved read forty thousand times.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use bytes::Bytes;
use pr_core::{CommandRequest, Effects, RequestOrigin};
use pr_mcp::policy::{PolicyRefusal, PolicyRule, glob_match};
use pr_mcp::tools::{Authorisation, Tool, ToolRefusal, authorise};
use pr_security::approval::{ApprovalToken, Epochs, ExecutionContext, Issuer};
use pr_security::trust::{AuthIdentity, Endpoint, ServerIdentity, TlsIdentity, TrustIdentity};

fn identity() -> TrustIdentity {
    TrustIdentity {
        endpoint: Endpoint::Tcp {
            host: "redis.internal".into(),
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

fn req(args: &[&str], effects: Effects) -> CommandRequest {
    CommandRequest::new(
        args.iter()
            .map(|a| Bytes::from(a.as_bytes().to_vec()))
            .collect(),
        // Everything here comes over MCP, so the origin is `Agent` — the most restricted.
        RequestOrigin::Agent,
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

fn rule() -> PolicyRule {
    PolicyRule {
        id: "read-players".into(),
        version: 3,
        key_patterns: vec!["player:*".into(), "config:{shard}:*".into()],
        max_result_bytes: 64 * 1024,
        max_calls_per_hour: 100,
    }
}

fn human_token(id: &TrustIdentity, r: &CommandRequest) -> ApprovalToken {
    ApprovalToken::for_request("p1", id, 0, Epochs::default(), r, 10_000, Issuer::Human)
}

fn policy_token(id: &TrustIdentity, r: &CommandRequest) -> ApprovalToken {
    ApprovalToken::for_request(
        "p1",
        id,
        0,
        Epochs::default(),
        r,
        10_000,
        Issuer::Policy {
            id: "read-players".into(),
        },
    )
}

// ---------------------------------------------------------------------------------------------
// What a policy may cover
// ---------------------------------------------------------------------------------------------

#[test]
fn a_read_of_a_listed_key_within_budget_is_covered_by_the_policy() {
    // The positive case, so the refusals below mean something.
    let id = identity();
    let r = req(&["GET", "player:10001"], Effects::read());
    let got = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"player:10001".to_vec()],
        1024,
        3,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .expect("a listed key within budget");
    assert_eq!(
        got,
        Authorisation::ByPolicy {
            rule: "read-players".into(),
            version: 3,
        },
        "the rule's version must reach the audit line; an approval traceable to \"the policy\" \
         stops being traceable the moment the policy is edited"
    );
}

#[test]
fn a_write_is_refused_and_the_message_says_a_person_must_approve_it() {
    // V-D08's stated criterion, at its most direct.
    let id = identity();
    let r = req(&["SET", "player:10001", "x"], Effects::write());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"player:10001".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .expect_err("a write must not be covered by a policy");
    let ToolRefusal::NeedsApproval { policy } = &err else {
        panic!("expected a policy refusal, got {err:?}");
    };
    assert_eq!(
        *policy,
        PolicyRefusal::NotReadOnly {
            offending: "a write"
        }
    );
    assert!(
        err.message().contains("A person must approve"),
        "{}",
        err.message()
    );
}

#[test]
fn every_effect_outside_reads_data_is_refused_by_name() {
    // §29.4 lists four. `destructive` and `blocks` are refused too: the first because a
    // `FLUSHDB` that is somehow not flagged `writes_data` would otherwise slip through, and
    // the second because a blocking command held open by an agent occupies a connection for
    // as long as it likes — a resource decision a person should make.
    let id = identity();
    let cases: &[(Effects, &str)] = &[
        (Effects::write(), "a write"),
        (
            Effects {
                reads_data: true,
                consumes: true,
                ..Effects::default()
            },
            "a consuming read",
        ),
        (
            Effects {
                admin: true,
                ..Effects::default()
            },
            "administrative",
        ),
        (
            Effects {
                writes_data: true,
                destructive: true,
                ..Effects::default()
            },
            "a write",
        ),
        (
            Effects {
                destructive: true,
                ..Effects::default()
            },
            "destructive",
        ),
        (
            Effects {
                reads_data: true,
                blocks: true,
                ..Effects::default()
            },
            "potentially blocking",
        ),
        (Effects::unknown(), "unclassified"),
    ];
    for (effects, _expected) in cases {
        let r = req(&["CMD", "player:1"], *effects);
        let err = authorise(
            Tool::RedisRead,
            Some(&r),
            &[b"player:1".to_vec()],
            16,
            0,
            Some(&rule()),
            None,
            &ctx(&id),
        )
        .expect_err("outside reads_data must be refused");
        assert!(
            matches!(
                err,
                ToolRefusal::NeedsApproval {
                    policy: PolicyRefusal::NotReadOnly { .. } | PolicyRefusal::Unclassified
                }
            ),
            "{effects:?} produced {err:?}"
        );
    }
}

#[test]
fn an_unclassified_command_is_refused_rather_than_assumed_harmless() {
    // The route an agent would actually take. A command the local catalog cannot vouch for is
    // the one place a write could reach a read-only policy, so `unknown` gets its own refusal
    // with its own message rather than being folded into "not read-only".
    let id = identity();
    let r = req(&["SOMEMODULE.DOTHING", "player:1"], Effects::unknown());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"player:1".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::Unclassified
        }
    ));
    assert!(
        err.message().contains("does not classify"),
        "{}",
        err.message()
    );
}

#[test]
fn a_key_the_profile_does_not_list_is_refused_even_for_a_read() {
    let id = identity();
    let r = req(&["GET", "session:abc"], Effects::read());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"session:abc".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(
        matches!(
            &err,
            ToolRefusal::NeedsApproval { policy: PolicyRefusal::KeyNotListed(k) } if k == "session:abc"
        ),
        "{err:?}"
    );
}

#[test]
fn one_unlisted_key_among_listed_ones_refuses_the_whole_request() {
    // An MGET is one request. Covering it partially would mean covering it.
    let id = identity();
    let r = req(&["MGET", "player:1", "session:abc"], Effects::read());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"player:1".to_vec(), b"session:abc".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::KeyNotListed(_)
        }
    ));
}

#[test]
fn a_rule_with_no_key_patterns_authorises_nothing() {
    // The direction an empty list has to fail in. Most configuration systems read an empty
    // list as "no restriction", which here would mean a rule somebody started writing and did
    // not finish silently covers the whole keyspace.
    let id = identity();
    let mut r0 = rule();
    r0.key_patterns.clear();
    let r = req(&["GET", "player:1"], Effects::read());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"player:1".to_vec()],
        16,
        0,
        Some(&r0),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::NoKeyPatterns
        }
    ));
}

#[test]
fn the_result_budget_and_the_hourly_rate_are_both_enforced() {
    let id = identity();
    let r = req(&["GET", "player:1"], Effects::read());
    let keys = [b"player:1".to_vec()];

    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &keys,
        64 * 1024 + 1,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::ResultTooLarge { .. }
        }
    ));

    // And the rate limit, which is what stops one approved read becoming a keyspace dump.
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &keys,
        16,
        100,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ToolRefusal::NeedsApproval {
                policy: PolicyRefusal::RateLimited {
                    used: 100,
                    limit: 100
                }
            }
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// What only a person may do
// ---------------------------------------------------------------------------------------------

#[test]
fn every_execute_approved_plan_call_needs_a_human_token_even_for_a_read_only_plan() {
    // The clause most easily lost. §29.4 says *every* call, and the plan being read-only is
    // not an exception — the plan is the unit being approved, not its steps.
    let id = identity();
    let harmless = req(&["GET", "player:1"], Effects::read());

    let err = authorise(
        Tool::RedisExecuteApprovedPlan,
        Some(&harmless),
        &[b"player:1".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert_eq!(err, ToolRefusal::PlanNeedsHumanToken);
    assert!(err.message().contains("even when the plan only reads"));

    // And a policy-issued token for the same read is still not enough.
    let t = policy_token(&id, &harmless);
    let err = authorise(
        Tool::RedisExecuteApprovedPlan,
        Some(&harmless),
        &[b"player:1".to_vec()],
        16,
        0,
        Some(&rule()),
        Some(&t),
        &ctx(&id),
    )
    .unwrap_err();
    assert_eq!(err, ToolRefusal::TokenIsNotHuman);

    // A human token for the same request does authorise it.
    let t = human_token(&id, &harmless);
    assert_eq!(
        authorise(
            Tool::RedisExecuteApprovedPlan,
            Some(&harmless),
            &[b"player:1".to_vec()],
            16,
            0,
            Some(&rule()),
            Some(&t),
            &ctx(&id),
        )
        .unwrap(),
        Authorisation::ByHuman
    );
}

#[test]
fn a_token_for_one_plan_does_not_authorise_another() {
    // §23.2: the token binds the exact bytes. Approving a plan does not approve a different
    // one, however similar it reads.
    let id = identity();
    let approved = req(&["DEL", "player:1"], Effects::write());
    let substituted = req(&["DEL", "player:2"], Effects::write());
    let t = human_token(&id, &approved);

    let err = authorise(
        Tool::RedisExecuteApprovedPlan,
        Some(&substituted),
        &[b"player:2".to_vec()],
        16,
        0,
        None,
        Some(&t),
        &ctx(&id),
    )
    .unwrap_err();
    assert!(
        matches!(err, ToolRefusal::TokenRefused(_)),
        "a substituted plan was authorised: {err:?}"
    );
}

#[test]
fn a_policy_issued_token_is_not_a_shortcut_past_the_policy_check() {
    // If a policy token short-circuited the boundary, a rule could issue itself a token for a
    // write and the write would go through. It has to pass the same check the policy would.
    let id = identity();
    let write = req(&["SET", "player:1", "x"], Effects::write());
    let t = policy_token(&id, &write);
    let err = authorise(
        Tool::RedisRead,
        Some(&write),
        &[b"player:1".to_vec()],
        16,
        0,
        Some(&rule()),
        Some(&t),
        &ctx(&id),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ToolRefusal::NeedsApproval {
                policy: PolicyRefusal::NotReadOnly { .. }
            }
        ),
        "{err:?}"
    );
}

#[test]
fn plan_write_produces_a_plan_and_is_not_something_a_policy_can_reach() {
    // `redis.plan_write` exists so an agent can *propose*. It is outside a policy's reach, so
    // no rule can quietly turn proposing into doing.
    assert!(!Tool::RedisPlanWrite.policy_may_reach());
    assert!(!Tool::RedisExecuteApprovedPlan.policy_may_reach());
    for t in [
        Tool::ConnectionsListSafe,
        Tool::RedisRead,
        Tool::RedisInspect,
        Tool::RedisScanLimited,
        Tool::RedisDiagnose,
    ] {
        assert!(t.policy_may_reach(), "{} should be reachable", t.name());
    }
}

#[test]
fn a_policy_cannot_authorise_a_plan_execution_by_any_route() {
    // Stated as its own unconditional method rather than a branch, because a branch invites
    // "unless the plan is read-only".
    let err = rule().may_authorise_plan_execution().unwrap_err();
    assert_eq!(err, PolicyRefusal::PlanExecutionIsAlwaysHuman);
}

#[test]
fn there_is_no_tool_that_takes_arbitrary_argv() {
    // §29.4: 「不能默认暴露『任意命令 + 任意凭证 + 无限制扫描』」.
    for name in [
        "redis.execute",
        "redis.command",
        "redis.raw",
        "redis.eval",
        "redis.scan",
        "shell",
    ] {
        assert!(Tool::parse(name).is_none(), "{name} resolved to a tool");
    }
    // The bounded scan is the one that exists, and its name says so.
    assert_eq!(
        Tool::parse("redis.scan_limited"),
        Some(Tool::RedisScanLimited)
    );
}

// ---------------------------------------------------------------------------------------------
// Key patterns mean what the server means by them
// ---------------------------------------------------------------------------------------------

#[test]
fn key_patterns_follow_redis_glob_rules() {
    // Borrowed from a generic glob crate, `user:*` would authorise a different set than the
    // operator who wrote it in a profile believes. These cases are Redis's.
    assert!(glob_match("player:*", "player:10001"));
    assert!(glob_match("player:*", "player:"));
    assert!(!glob_match("player:*", "players:1"));
    assert!(glob_match("h?llo", "hello"));
    assert!(!glob_match("h?llo", "heello"));
    assert!(glob_match("h[ae]llo", "hallo"));
    assert!(!glob_match("h[ae]llo", "hillo"));
    assert!(glob_match("h[^e]llo", "hallo"));
    assert!(!glob_match("h[^e]llo", "hello"));
    assert!(glob_match("h[a-c]llo", "hbllo"));
    assert!(!glob_match("h[a-c]llo", "hdllo"));
    assert!(glob_match("*", "anything"));
    assert!(glob_match("a*b*c", "axxbyyc"));
    assert!(!glob_match("a*b*c", "axxbyy"));
    // An escaped metacharacter is a literal.
    assert!(glob_match(r"config\*", "config*"));
    assert!(!glob_match(r"config\*", "configX"));
}

#[test]
fn a_pattern_cannot_be_widened_by_a_key_that_contains_a_metacharacter() {
    // The key is data, not a pattern. A key literally named `*` must not match a rule that
    // lists only `player:*`, and must not act as a wildcard against anything.
    assert!(!glob_match("player:*", "*"));
    assert!(!glob_match("player:1", "*"));

    let id = identity();
    let r = req(&["GET", "*"], Effects::read());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[b"*".to_vec()],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::KeyNotListed(_)
        }
    ));
}

#[test]
fn a_request_that_names_no_key_is_not_covered_by_a_key_pattern_rule() {
    // `DBSIZE` reads, touches no key, and no pattern can vouch for it. Covering it would mean
    // the rule authorises something its author never wrote down.
    let id = identity();
    let r = req(&["DBSIZE"], Effects::read());
    let err = authorise(
        Tool::RedisRead,
        Some(&r),
        &[],
        16,
        0,
        Some(&rule()),
        None,
        &ctx(&id),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ToolRefusal::NeedsApproval {
            policy: PolicyRefusal::NoKeys
        }
    ));
}
