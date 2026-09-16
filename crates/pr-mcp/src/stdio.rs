//! The `--mcp-stdio` bootstrap (v2.1 §29.4, R29, V-D08).
//!
//! > 由明确调用的 MCP host 启动子进程，通过 stdin/stdout 通讯；**stdout 仅用于协议，日志走
//! > stderr**。`--mcp-stdio` 是独立的 bootstrap 路径：不初始化 REPL/TUI/终端协调器、不输出
//! > banner 或进度、不进入连接选择器、不触发任何交互式凭证提示（缺凭证即以 JSON-RPC 错误
//! > 返回）；**stdout 上除 JSON-RPC 帧外没有任何字节**，这一点由 PTY-less 集成测试断言。
//!
//! The last clause is why this module owns its own writer rather than using `println!`. One
//! stray line anywhere in the crate graph — a progress message, a warning, a `dbg!` left
//! behind — reaches the host as a parse error that points nowhere near its cause, and the
//! symptom is "the MCP server does not work", which is the hardest kind of bug to find.
//!
//! Missing credentials return a JSON-RPC error rather than prompting. A prompt on a pipe is
//! not a prompt; it is a process that never returns, and the host has no way to see why.

use crate::policy::PolicyRule;
use crate::tools::{self, Tool};
use std::io::{BufRead, Write};

/// JSON-RPC error codes. The reserved ones are from the specification; the rest are ours and
/// are documented here because an agent host will branch on them.
pub mod code {
    /// Malformed JSON.
    pub const PARSE_ERROR: i64 = -32700;
    /// Not a valid request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No such method or tool.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Bad parameters.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Refused by policy or for want of an approval a person must give.
    ///
    /// Distinct from `INVALID_PARAMS` on purpose: the request was well formed and understood,
    /// and what is missing is a human decision. An agent that retries with different
    /// parameters is doing the wrong thing; one that asks its user is doing the right thing.
    pub const APPROVAL_REQUIRED: i64 = -32001;
    /// A credential is not available, and this path never prompts.
    pub const CREDENTIAL_UNAVAILABLE: i64 = -32002;
    /// Understood, allowed, and not built yet. Phase 0 ships the boundary, not the execution.
    pub const NOT_IMPLEMENTED: i64 = -32003;
}

/// What the server needs in order to answer.
pub trait Backend {
    /// Profiles the agent may see. Endpoints and secrets are withheld by construction: this
    /// returns names and nothing else.
    fn safe_profile_names(&self) -> Vec<String>;
    /// The policy rule in force for a profile, if a person wrote one.
    fn policy_rule(&self, profile: &str) -> Option<PolicyRule>;
    /// Whether a credential is available **without prompting**.
    fn credential_available(&self, profile: &str) -> bool;
}

/// A backend with no profiles, for the bootstrap test.
#[derive(Debug, Default)]
pub struct EmptyBackend;

impl Backend for EmptyBackend {
    fn safe_profile_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn policy_rule(&self, _profile: &str) -> Option<PolicyRule> {
        None
    }
    fn credential_available(&self, _profile: &str) -> bool {
        false
    }
}

/// Run the JSON-RPC loop until stdin closes.
///
/// `out` receives protocol frames and **nothing else**; `log` receives everything a human
/// might want. They are separate parameters so a test can hold them apart, which is the only
/// way to assert the separation rather than hope for it.
///
/// # Errors
/// Only I/O failures on `out`. A malformed request is answered, not returned.
pub fn serve<B: Backend>(
    input: impl BufRead,
    mut out: impl Write,
    mut log: impl Write,
    backend: &B,
) -> std::io::Result<()> {
    let _ = writeln!(log, "penguin-redis mcp-stdio: ready");
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle(&line, backend);
        if let Some(r) = response {
            // One frame, one line, one flush. Line-delimited JSON is what the stdio transport
            // uses, and flushing per frame is what stops a host waiting on a buffer.
            out.write_all(r.as_bytes())?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
    let _ = writeln!(log, "penguin-redis mcp-stdio: stdin closed");
    Ok(())
}

/// Handle one line. Returns `None` for a notification, which takes no response.
fn handle<B: Backend>(line: &str, backend: &B) -> Option<String> {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error(
                &serde_json::Value::Null,
                code::PARSE_ERROR,
                &e.to_string(),
            ));
        }
    };
    let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let Some(method) = v.get("method").and_then(|m| m.as_str()) else {
        return Some(error(&id, code::INVALID_REQUEST, "no method"));
    };
    // A notification has no id and takes no response, per JSON-RPC. Answering one would put a
    // frame on stdout that the host is not expecting.
    let is_notification = v.get("id").is_none();

    let result = match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "penguin-redis", "version": env!("CARGO_PKG_VERSION") },
        })),
        "tools/list" => Ok(serde_json::json!({
            "tools": tools::ALL.iter().map(|t| serde_json::json!({
                "name": t.name(),
                "description": t.description(),
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => return call(id, &v, backend, is_notification),
        "ping" => Ok(serde_json::json!({})),
        other => Err((code::METHOD_NOT_FOUND, other.to_owned())),
    };

    if is_notification {
        return None;
    }
    Some(match result {
        Ok(r) => ok(&id, &r),
        Err((c, m)) => error(&id, c, &m),
    })
}

fn call<B: Backend>(
    id: serde_json::Value,
    v: &serde_json::Value,
    backend: &B,
    is_notification: bool,
) -> Option<String> {
    let params = v.get("params");
    let name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
    let Some(name) = name else {
        return (!is_notification).then(|| error(&id, code::INVALID_PARAMS, "no tool name"));
    };
    let Some(tool) = Tool::parse(name) else {
        // The closed list, enforced. There is no arbitrary-command tool to fall through to.
        return (!is_notification).then(|| {
            error(
                &id,
                code::METHOD_NOT_FOUND,
                &format!(
                    "{name} is not a tool; Penguin exposes a closed list and no \
                     arbitrary-command tool (§29.4)"
                ),
            )
        });
    };

    let profile = params
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.get("profile"))
        .and_then(|p| p.as_str())
        .unwrap_or("");

    let result = match tool {
        Tool::ConnectionsListSafe => Ok(serde_json::json!({
            "profiles": backend.safe_profile_names(),
            "note": "endpoints and secrets are not exposed over MCP (§29.4)",
        })),
        Tool::RedisExecuteApprovedPlan => Err((
            code::APPROVAL_REQUIRED,
            crate::tools::ToolRefusal::PlanNeedsHumanToken.message(),
        )),
        _ => {
            if backend.credential_available(profile) {
                Err((
                    code::NOT_IMPLEMENTED,
                    format!("{name} is not implemented in Phase 0"),
                ))
            } else {
                // Never a prompt. A prompt on a pipe is a process that never returns.
                Err((
                    code::CREDENTIAL_UNAVAILABLE,
                    format!(
                        "no credential is available for {profile:?} and this path never prompts; \
                         run `prc @{profile}` once in a terminal to store one"
                    ),
                ))
            }
        }
    };

    if is_notification {
        return None;
    }
    Some(match result {
        Ok(r) => ok(&id, &r),
        Err((c, m)) => error(&id, c, &m),
    })
}

fn ok(id: &serde_json::Value, result: &serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error(id: &serde_json::Value, code: i64, message: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A backend that *does* have profiles, so the "names only" claim is tested against
    /// something rather than against an empty list.
    struct Populated;

    impl Backend for Populated {
        fn safe_profile_names(&self) -> Vec<String> {
            vec!["dev".into(), "prod".into()]
        }
        fn policy_rule(&self, _p: &str) -> Option<PolicyRule> {
            None
        }
        fn credential_available(&self, p: &str) -> bool {
            p == "dev"
        }
    }

    fn one(line: &str, backend: &impl Backend) -> serde_json::Value {
        let mut out = Vec::new();
        let mut log = Vec::new();
        serve(
            std::io::BufReader::new(format!("{line}\n").as_bytes()),
            &mut out,
            &mut log,
            backend,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        serde_json::from_str(text.trim()).unwrap()
    }

    #[test]
    fn list_safe_returns_names_without_anything_attached_to_them() {
        let v = one(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"connections.list_safe"}}"#,
            &Populated,
        );
        let profiles = v["result"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);
        for p in profiles {
            assert!(p.is_string(), "{p} is not a bare name");
        }
        assert_eq!(profiles[0], "dev");
    }

    #[test]
    fn a_profile_with_a_credential_gets_past_the_credential_check() {
        // Without this, `a_missing_credential_is_an_error_frame` would also pass for a server
        // that refuses everything, which is not the behaviour being claimed.
        let v = one(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"redis.read","arguments":{"profile":"dev"}}}"#,
            &Populated,
        );
        assert_eq!(
            v["error"]["code"],
            code::NOT_IMPLEMENTED,
            "expected to get past the credential check, got {v}"
        );

        let v = one(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"redis.read","arguments":{"profile":"prod"}}}"#,
            &Populated,
        );
        assert_eq!(v["error"]["code"], code::CREDENTIAL_UNAVAILABLE);
    }

    #[test]
    fn stdout_and_the_log_are_different_streams() {
        // The separation §29.4 requires, asserted where it is cheapest to assert: the writer
        // that receives frames receives only frames.
        let mut out = Vec::new();
        let mut log = Vec::new();
        serve(
            std::io::BufReader::new(&b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n"[..]),
            &mut out,
            &mut log,
            &EmptyBackend,
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        for line in out.lines() {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("{line:?} is not a frame: {e}"));
        }
        assert!(!log.is_empty(), "nothing was logged");
        assert!(!out.contains("mcp-stdio"), "a log line reached stdout");
    }
}
