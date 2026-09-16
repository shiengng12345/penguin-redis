//! Live-topology tests (V-G01 / V-G02).
//!
//! These run only when `PENGUIN_TEST_TOPOLOGY` is set, which the CI topology jobs do after
//! `ci/topology/*-up.sh` has brought the fixture online. Without it they skip, so a normal
//! `cargo nextest run` on a laptop is unaffected.
//!
//! Their job is to check our *computed* view against what a real server reports — a slot
//! table we derive but never cross-check is exactly the kind of claim v2.1 §38.2 forbids.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_routing::{parse_redirect, slot_for};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn topology() -> Option<String> {
    std::env::var("PENGUIN_TEST_TOPOLOGY").ok()
}

/// Minimal RESP2 client: enough to issue a command and read one reply as text.
/// Deliberately independent of `pr-protocol` so a decoder bug cannot mask a routing bug.
struct Raw {
    r: BufReader<TcpStream>,
}

impl Raw {
    fn connect(addr: &str) -> std::io::Result<Self> {
        let s = TcpStream::connect(addr)?;
        s.set_read_timeout(Some(Duration::from_secs(10)))?;
        s.set_write_timeout(Some(Duration::from_secs(10)))?;
        Ok(Self {
            r: BufReader::new(s),
        })
    }

    fn cmd(&mut self, args: &[&str]) -> std::io::Result<String> {
        use std::fmt::Write as _;
        let mut out = format!("*{}\r\n", args.len());
        for a in args {
            let _ = write!(out, "${}\r\n{a}\r\n", a.len());
        }
        self.r.get_mut().write_all(out.as_bytes())?;
        self.r.get_mut().flush()?;
        self.read_reply()
    }

    fn read_reply(&mut self) -> std::io::Result<String> {
        let mut line = String::new();
        self.r.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']).to_owned();
        let (tag, rest) = line.split_at(1);
        match tag {
            "$" => {
                let n: i64 = rest.parse().unwrap_or(-1);
                if n < 0 {
                    return Ok("(nil)".into());
                }
                let mut buf = vec![0u8; usize::try_from(n).unwrap_or(0) + 2];
                self.r.read_exact(&mut buf)?;
                buf.truncate(buf.len() - 2);
                Ok(String::from_utf8_lossy(&buf).into_owned())
            }
            "*" => {
                let n: i64 = rest.parse().unwrap_or(0);
                let mut parts = Vec::new();
                for _ in 0..n.max(0) {
                    parts.push(self.read_reply()?);
                }
                Ok(parts.join(" "))
            }
            // Simple string, error and integer are all already complete lines.
            _ => Ok(line),
        }
    }
}

// ---------------------------------------------------------------- cluster

#[test]
fn topology_cluster_is_healthy() {
    let Some(t) = topology() else { return };
    if t != "cluster" {
        return;
    }
    let mut c = Raw::connect("127.0.0.1:7000").expect("connect to seed node");
    let info = c.cmd(&["CLUSTER", "INFO"]).expect("CLUSTER INFO");
    assert!(info.contains("cluster_state:ok"), "cluster not ok:\n{info}");
    assert!(
        info.contains("cluster_known_nodes:6"),
        "expected 6 nodes:\n{info}"
    );
    assert!(
        info.contains("cluster_size:3"),
        "expected 3 shards:\n{info}"
    );
}

#[test]
fn topology_cluster_slot_computation_matches_the_server() {
    // The real check: our CRC16 + hash-tag implementation against the server's own answer.
    let Some(t) = topology() else { return };
    if t != "cluster" {
        return;
    }
    let mut c = Raw::connect("127.0.0.1:7000").expect("connect");
    for key in [
        "foo",
        "bar",
        "hello",
        "player:10001",
        "{user1000}.following",
        "{user1000}.followers",
        "user1000",
        "{}foo",
        "{foo",
        "a{b}c",
        "中文",
    ] {
        let reply = c
            .cmd(&["CLUSTER", "KEYSLOT", key])
            .expect("CLUSTER KEYSLOT");
        let server: u16 = reply.trim_start_matches(':').parse().expect("integer slot");
        assert_eq!(
            slot_for(key.as_bytes()),
            server,
            "slot mismatch for {key:?}"
        );
    }
}

#[test]
fn topology_cluster_tagged_keys_share_a_slot_on_the_server() {
    let Some(t) = topology() else { return };
    if t != "cluster" {
        return;
    }
    let mut c = Raw::connect("127.0.0.1:7000").expect("connect");
    let a = c.cmd(&["CLUSTER", "KEYSLOT", "{u1}:a"]).expect("keyslot");
    let b = c.cmd(&["CLUSTER", "KEYSLOT", "{u1}:b"]).expect("keyslot");
    assert_eq!(a, b, "server disagrees that tagged keys share a slot");
}

#[test]
fn topology_cluster_crossslot_is_an_error_not_a_redirect() {
    // NET-04: the original command must not be split, and CROSSSLOT must never be mistaken
    // for a redirect.
    let Some(t) = topology() else { return };
    if t != "cluster" {
        return;
    }
    let mut c = Raw::connect("127.0.0.1:7000").expect("connect");
    let reply = c.cmd(&["MGET", "user1:a", "user2:b"]).expect("MGET");
    // Either CROSSSLOT, or MOVED because the seed does not own the first key.
    if reply.starts_with("-CROSSSLOT") {
        assert!(
            parse_redirect(reply.trim_start_matches('-')).is_none(),
            "CROSSSLOT parsed as a redirect"
        );
    } else {
        assert!(reply.starts_with("-MOVED"), "unexpected reply: {reply}");
    }
}

#[test]
fn topology_cluster_moved_points_at_a_real_node() {
    let Some(t) = topology() else { return };
    if t != "cluster" {
        return;
    }
    let mut c = Raw::connect("127.0.0.1:7000").expect("connect");
    // Find a key the seed does not own so the server issues MOVED.
    let mut saw_moved = false;
    for i in 0..200 {
        let key = format!("probe:{i}");
        let reply = c.cmd(&["GET", &key]).expect("GET");
        if let Some(r) = reply.strip_prefix('-').and_then(parse_redirect) {
            assert!(!r.ask, "GET should give MOVED, not ASK");
            assert_eq!(
                r.slot,
                slot_for(key.as_bytes()),
                "MOVED slot disagrees with ours"
            );
            assert!(
                (7000..=7005).contains(&r.port),
                "MOVED to an unexpected port: {r:?}"
            );
            saw_moved = true;
            break;
        }
    }
    assert!(
        saw_moved,
        "no MOVED seen in 200 probes; the seed cannot own every slot"
    );
}

// ---------------------------------------------------------------- sentinel

#[test]
fn topology_sentinel_reports_the_monitored_master() {
    let Some(t) = topology() else { return };
    if t != "sentinel" {
        return;
    }
    let mut s = Raw::connect("127.0.0.1:26379").expect("connect to sentinel");
    let reply = s
        .cmd(&["SENTINEL", "get-master-addr-by-name", "mymaster"])
        .expect("get-master-addr-by-name");
    assert!(
        !reply.contains("(nil)"),
        "sentinel does not know mymaster: {reply}"
    );
    assert!(reply.contains("6379"), "unexpected master addr: {reply}");
}

#[test]
fn topology_sentinel_quorum_sees_all_three_sentinels() {
    let Some(t) = topology() else { return };
    if t != "sentinel" {
        return;
    }
    let mut s = Raw::connect("127.0.0.1:26379").expect("connect");
    let reply = s
        .cmd(&["SENTINEL", "ckquorum", "mymaster"])
        .expect("ckquorum");
    assert!(reply.starts_with('+'), "quorum not satisfied: {reply}");
}

#[test]
fn topology_sentinel_master_has_a_replica() {
    let Some(t) = topology() else { return };
    if t != "sentinel" {
        return;
    }
    let mut s = Raw::connect("127.0.0.1:26379").expect("connect");
    let reply = s
        .cmd(&["SENTINEL", "replicas", "mymaster"])
        .expect("replicas");
    assert!(reply.contains("6379"), "no replica registered: {reply}");
}
