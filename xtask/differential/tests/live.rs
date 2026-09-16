//! V-A02 — the live half: two real servers, and proof that the harness can fail.
//!
//! Marked `#[ignore]` because it needs Docker, and run explicitly by the `differential` CI job
//! with `--ignored`. That combination is a trap worth naming: V-C07 found that *every* PTY
//! test was `#[cfg_attr(windows, ignore)]`, so the Windows path had never once run while the
//! suite reported green. The guard against a repeat is `the_committed_report_covers_exactly_
//! the_case_list` in `pure.rs`, which runs unconditionally and fails if this job's artifact
//! goes stale, plus `ci/check-differential.sh`, which re-runs and diffs.
//!
//! The negative controls below matter more than the positive one. A differential harness that
//! has only ever printed "same" has not been shown to be able to print anything else.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use bytes::Bytes;
use differential::cases::OutputExpectation;
use differential::compare::{Side, compare};
use differential::docker::{Server, ensure_docker};
use differential::proxy::RecordingProxy;
use pr_transport::Oneshot;
use std::time::Duration;

const IMAGE: &str = "redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7";
const TIMEOUT: Duration = Duration::from_secs(5);

fn argv(parts: &[&str]) -> Vec<Bytes> {
    parts
        .iter()
        .map(|p| Bytes::from(p.as_bytes().to_vec()))
        .collect()
}

/// Send one command through a recording proxy and return what crossed the wire.
fn through_proxy(server: &Server, cmd: &[&str]) -> Side {
    let proxy = RecordingProxy::start(server.address()).unwrap();
    let mut conn = Oneshot::connect("127.0.0.1", proxy.port(), TIMEOUT).unwrap();
    let reply = conn.call(&argv(cmd)).unwrap();
    drop(conn);
    let recordings = proxy.finish();
    Side {
        sent: recordings
            .iter()
            .flat_map(|r| r.to_server.clone())
            .collect(),
        received: recordings
            .iter()
            .flat_map(|r| r.to_client.clone())
            .collect(),
        stdout: format!("{reply:?}").into_bytes(),
        stderr: Vec::new(),
        exit: 0,
        final_state: Vec::new(),
    }
}

fn read_state(server: &Server, cmd: &[&str]) -> Vec<Vec<u8>> {
    let mut conn = Oneshot::connect("127.0.0.1", server.port(), TIMEOUT).unwrap();
    vec![format!("{:?}", conn.call(&argv(cmd)).unwrap()).into_bytes()]
}

#[test]
#[ignore = "needs Docker; run by the differential CI job with --ignored"]
fn the_proxy_records_the_real_wire_in_both_directions() {
    ensure_docker().expect("docker");
    let s = Server::start("prdiff-live-wire", IMAGE).unwrap();
    let side = through_proxy(&s, &["SET", "live:wire", "--raw"]);

    // Layer 1: the argument that looks like a flag went out as a value, byte for byte.
    assert_eq!(
        side.sent,
        b"*3\r\n$3\r\nSET\r\n$9\r\nlive:wire\r\n$5\r\n--raw\r\n".to_vec(),
        "the recorded argv is not what was sent"
    );
    // Layer 2: the reply came back framed as RESP, not as text the client made up.
    assert_eq!(side.received, b"+OK\r\n".to_vec());
}

#[test]
#[ignore = "needs Docker; run by the differential CI job with --ignored"]
fn two_servers_that_really_diverge_are_reported_as_diverged() {
    // The negative control for layer 3, and the one §32.2 exists for. Both sides run the same
    // command and print the same thing; only the state they leave behind differs. A harness
    // that compared output alone would call this a pass.
    ensure_docker().expect("docker");
    let a = Server::start("prdiff-live-a", IMAGE).unwrap();
    let b = Server::start("prdiff-live-b", IMAGE).unwrap();

    let mut ca = Oneshot::connect("127.0.0.1", a.port(), TIMEOUT).unwrap();
    let mut cb = Oneshot::connect("127.0.0.1", b.port(), TIMEOUT).unwrap();
    ca.call(&argv(&["SET", "live:n", "1"])).unwrap();
    cb.call(&argv(&["SET", "live:n", "41"])).unwrap();
    drop((ca, cb));

    let mut sa = through_proxy(&a, &["INCR", "live:n"]);
    let mut sb = through_proxy(&b, &["INCR", "live:n"]);
    sa.final_state = read_state(&a, &["GET", "live:n"]);
    sb.final_state = read_state(&b, &["GET", "live:n"]);
    // Make layers 1 and 4 agree so the failure can only come from 2 and 3.
    sb.stdout.clone_from(&sa.stdout);

    let out = compare(&sa, &sb, OutputExpectation::Identical);
    assert!(out.argv_bytes.ok(), "the same argv went out both times");
    assert!(
        !out.final_state.ok(),
        "layer 3 missed a real state divergence"
    );
    assert!(!out.reply_bytes.ok(), "layer 2 missed :2 versus :42");
}

#[test]
#[ignore = "needs Docker; run by the differential CI job with --ignored"]
fn the_same_server_used_twice_would_have_hidden_it() {
    // Why the harness pays for a second container, demonstrated rather than asserted.
    //
    // One server, `INCR` run twice: the two replies differ for a reason that has nothing to do
    // with either client. This is the mistake §32.2 names, and it is invisible unless you go
    // looking for it -- the run is green, the numbers just mean nothing.
    ensure_docker().expect("docker");
    let one = Server::start("prdiff-live-shared", IMAGE).unwrap();
    let mut c = Oneshot::connect("127.0.0.1", one.port(), TIMEOUT).unwrap();
    c.call(&argv(&["SET", "live:shared", "1"])).unwrap();
    drop(c);

    let first = through_proxy(&one, &["INCR", "live:shared"]);
    let second = through_proxy(&one, &["INCR", "live:shared"]);

    assert_eq!(first.sent, second.sent, "identical commands");
    assert_ne!(
        first.received, second.received,
        "the second run saw the first one's effect -- this is the comparison §32.2 forbids"
    );
    assert_eq!(first.received, b":2\r\n".to_vec());
    assert_eq!(second.received, b":3\r\n".to_vec());
}

#[test]
#[ignore = "needs Docker; run by the differential CI job with --ignored"]
fn the_committed_report_still_reproduces() {
    // The artifact and the code agree today, not on the day it was generated.
    ensure_docker().expect("docker");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repo root");
    let status = std::process::Command::new("bash")
        .arg(root.join("ci/check-differential.sh"))
        .current_dir(root)
        .status()
        .expect("run the checker");
    assert!(
        status.success(),
        "the committed report no longer reproduces"
    );
}
