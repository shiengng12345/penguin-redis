//! V-D08 — the `--mcp-stdio` bootstrap, from outside the process (v2.1 §29.4, R29).
//!
//! PTY-less on purpose, and that *is* the test: an MCP host starts `prc` as a subprocess with
//! pipes on both ends. Anything that only works on a terminal — a banner, a progress line, a
//! credential prompt, a raw-mode claim — either produces bytes the host cannot parse or blocks
//! forever. Both failures look like "the MCP server does not work" and neither points at the
//! cause, so they are checked here rather than discovered by a user.
//!
//! The pass criterion is two things: **stdout 无杂字节** and **写类请求无人签令牌被拒**. This
//! file owns the first; `approval_boundary.rs` owns the second.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn prc() -> PathBuf {
    if let Ok(p) = std::env::var("PRC_BINARY") {
        return PathBuf::from(p);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    for profile in ["debug", "release"] {
        let p = root.join("target").join(profile).join("prc");
        if p.exists() {
            return p;
        }
    }
    panic!("prc is not built; run `cargo build -p prc`");
}

struct Run {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    elapsed: Duration,
    status: std::process::ExitStatus,
}

/// Feed `input` to `prc --mcp-stdio` over a pipe and collect both streams.
fn run(input: &str) -> Run {
    let started = Instant::now();
    let mut child = Command::new(prc())
        .arg("--mcp-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn prc --mcp-stdio");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    // Closing stdin is how an MCP host says goodbye; a server that does not notice hangs.
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    Run {
        stdout: out.stdout,
        stderr: out.stderr,
        elapsed: started.elapsed(),
        status: out.status,
    }
}

/// Every stdout line, parsed. A line that is not a JSON-RPC frame fails here.
fn frames(r: &Run) -> Vec<serde_json::Value> {
    let text = std::str::from_utf8(&r.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not UTF-8, so it is not JSON-RPC: {e}; bytes: {:?}",
            r.stdout
        )
    });
    text.lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .unwrap_or_else(|e| panic!("a line on stdout is not a JSON-RPC frame: {l:?} ({e})"))
        })
        .collect()
}

#[test]
fn an_empty_session_writes_nothing_at_all_to_stdout() {
    // The banner test. Not "the banner is short" — zero bytes. A host reads stdout expecting
    // frames, so anything friendly written before the first one is a parse error.
    let r = run("");
    assert!(
        r.stdout.is_empty(),
        "stdout is not empty on an empty session: {:?}",
        String::from_utf8_lossy(&r.stdout)
    );
    assert!(r.status.success());
    // And the log did happen, on the other stream. Without this the test would also pass for
    // a binary that logs nowhere, which is not what §29.4 asks for.
    assert!(
        String::from_utf8_lossy(&r.stderr).contains("mcp-stdio"),
        "nothing was logged to stderr: {:?}",
        String::from_utf8_lossy(&r.stderr)
    );
}

#[test]
fn every_byte_on_stdout_belongs_to_a_json_rpc_frame() {
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
         {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n\
         {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\n");
    let f = frames(&r);
    assert_eq!(f.len(), 3, "one frame per request");
    for frame in &f {
        assert_eq!(frame["jsonrpc"], "2.0");
        assert!(frame.get("result").is_some() || frame.get("error").is_some());
    }

    // No control bytes beyond the newlines that separate frames. A terminal escape on stdout
    // would be both a parse error for the host and, per §23.6, a thing we never emit blind.
    for (i, b) in r.stdout.iter().enumerate() {
        assert!(
            *b == b'\n' || *b >= 0x20,
            "control byte {b:#04x} at offset {i} on stdout"
        );
    }
}

#[test]
fn a_malformed_line_is_answered_rather_than_crashing_or_going_quiet() {
    // An agent will send garbage eventually. The two bad outcomes are a panic — whose message
    // would go to stderr and leave the host waiting — and silence, which is the same thing
    // without the evidence.
    let r = run("not json at all\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
    let f = frames(&r);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0]["error"]["code"], -32700, "parse error expected");
    assert_eq!(f[1]["id"], 7, "the session continued after the bad line");
    assert!(r.status.success());
}

#[test]
fn a_notification_gets_no_frame_back() {
    // JSON-RPC: no id, no response. Answering one puts a frame on stdout the host is not
    // expecting, and an unexpected frame is as bad as an unparseable one.
    let r = run("{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n");
    assert!(
        r.stdout.is_empty(),
        "a notification was answered: {:?}",
        String::from_utf8_lossy(&r.stdout)
    );
}

#[test]
fn a_missing_credential_is_an_error_frame_and_never_a_prompt() {
    // §29.4: 「不触发任何交互式凭证提示（缺凭证即以 JSON-RPC 错误返回）」.
    //
    // A prompt on a pipe is not a prompt. It is a process that never returns, and the host has
    // no way to see why — which is why the elapsed time is asserted as well as the frame.
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
         \"params\":{\"name\":\"redis.read\",\"arguments\":{\"profile\":\"nosuch\"}}}\n");
    let f = frames(&r);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0]["error"]["code"], -32002, "credential-unavailable code");
    let msg = f[0]["error"]["message"].as_str().unwrap();
    assert!(msg.contains("never prompts"), "{msg}");
    assert!(
        msg.contains("prc @nosuch"),
        "the error does not say how to fix it: {msg}"
    );
    assert!(
        r.elapsed < Duration::from_secs(5),
        "took {:?}; a prompt would have blocked",
        r.elapsed
    );
}

#[test]
fn the_tool_list_is_closed_and_has_no_arbitrary_command_tool() {
    // §29.4: 「不能默认暴露『任意命令 + 任意凭证 + 无限制扫描』」. The absence is the design, so
    // the test asserts the exact list rather than a lower bound on it.
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n");
    let f = frames(&r);
    let names: Vec<&str> = f[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "connections.list_safe",
            "redis.read",
            "redis.inspect",
            "redis.scan_limited",
            "redis.diagnose",
            "redis.plan_write",
            "redis.execute_approved_plan",
        ]
    );

    // And a name that is not on it is refused rather than falling through to something.
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
         \"params\":{\"name\":\"redis.execute\",\"arguments\":{\"argv\":[\"FLUSHALL\"]}}}\n");
    let f = frames(&r);
    assert_eq!(f[0]["error"]["code"], -32601);
    assert!(
        f[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("closed list")
    );
}

#[test]
fn connections_list_safe_returns_names_and_nothing_else() {
    // The tool exists so an agent can name a profile without learning where it points. A
    // reply carrying the endpoint would hand an agent exactly the thing §21.3 spends its
    // length keeping separate from the credential.
    //
    // Checked by shape rather than by scanning for words: a substring scan would trip over
    // the explanatory note (which *says* "endpoints") and, worse, would pass for a leak
    // spelled differently. The shape is that `profiles` is an array of plain strings.
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
         \"params\":{\"name\":\"connections.list_safe\"}}\n");
    let f = frames(&r);
    let result = f[0]["result"].as_object().expect("an object");
    let mut keys: Vec<&str> = result.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["note", "profiles"],
        "unexpected field in a list_safe reply"
    );

    for p in result["profiles"].as_array().expect("an array") {
        assert!(
            p.is_string(),
            "a profile is not a bare name, so it carries something: {p}"
        );
    }
    assert!(
        result["note"]
            .as_str()
            .unwrap()
            .contains("not exposed over MCP")
    );
}

#[test]
fn execute_approved_plan_is_refused_before_anything_else_happens() {
    // The headline of the approval boundary, checked here too because this is the surface an
    // agent actually reaches: no token, no execution, and an error code that tells the agent
    // to ask a person rather than to retry with different parameters.
    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
         \"params\":{\"name\":\"redis.execute_approved_plan\",\
         \"arguments\":{\"plan_hash\":\"deadbeef\"}}}\n");
    let f = frames(&r);
    assert_eq!(f[0]["error"]["code"], -32001, "approval-required code");
    let msg = f[0]["error"]["message"].as_str().unwrap();
    assert!(msg.contains("human-signed"), "{msg}");
    assert!(
        msg.contains("even when the plan only reads"),
        "the unconditional part must be stated: {msg}"
    );
}

#[test]
fn the_bootstrap_does_not_print_what_the_ordinary_start_up_prints() {
    // `--version` loads the catalog and prints its provenance. If `--mcp-stdio` shared the
    // ordinary start-up path, some of that would reach stdout. Comparing against the real
    // output of the other path is a stronger check than a list of strings I thought of.
    let version = Command::new(prc()).arg("--version").output().unwrap();
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    assert!(version.contains("catalog"), "sanity: {version}");

    let r = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n");
    let stdout = String::from_utf8_lossy(&r.stdout).into_owned();
    for line in version.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            !stdout.contains(line),
            "the mcp-stdio path printed a start-up line: {line:?}"
        );
    }
}
