//! The server matrix, actually testing the servers (V-A05, V-I01, v2.1 §20.4, §34.2).
//!
//! `ci.yml`'s `server-matrix` job starts each digest-pinned server and runs
//! `cargo nextest run -E 'test(/live_/)'`. Until this file existed, **no test was named
//! `live_*`**, the filter matched nothing, `--no-tests=pass` made that a pass, and seven green
//! badges — redis 7.2, 7.4 and 8.0, valkey 8.0 and 8.1, across three topologies — meant that
//! seven containers had started and been thrown away.
//!
//! That is why every `status` in `compatibility/manifest.toml` says `unknown`: whoever wrote it
//! could not honestly say otherwise, and ADR-011 forbids claiming `verified` for a version that
//! was not actually run.
//!
//! These tests make the matrix mean something. They assert the things that **differ between the
//! targets the manifest pins**, because a test that passes identically on every version proves
//! only that the connection worked:
//!
//! | claim in the manifest | what checks it here |
//! |---|---|
//! | the line is what it says | `live_the_server_is_the_version_the_matrix_says` |
//! | `tested = ["RESP2", "RESP3"]` | `live_resp3_is_available` |
//! | redis 7.2 is "pre-Hash-field-TTL; no HSCAN NOVALUES" | `live_hash_field_ttl_matches_the_line` |
//! | 7.4 "introduces HSCAN NOVALUES and Hash field TTL" | the same test, from the other side |
//! | the family is redis or valkey | `live_the_family_is_what_the_matrix_says` |
//!
//! ## Skipping
//!
//! They need a server on `127.0.0.1:6379`, which a developer will not usually have. They skip
//! when `PENGUIN_TEST_FAMILY` is unset — **and the CI job sets it**, so in the one place where
//! skipping would hide something, skipping is impossible. That is deliberately the opposite of
//! `#[ignore]`: an ignored test is invisible in the run, and this one says what it is doing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use bytes::Bytes;
use pr_protocol::Value;
use pr_transport::Oneshot;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

/// The target the matrix says it started, or `None` when there is no server to talk to.
struct Target {
    family: String,
    line: String,
}

fn target() -> Option<Target> {
    let family = std::env::var("PENGUIN_TEST_FAMILY").ok()?;
    let line = std::env::var("PENGUIN_TEST_LINE").unwrap_or_default();
    if family.is_empty() {
        return None;
    }
    Some(Target { family, line })
}

/// Announce a skip rather than passing quietly.
fn skipped(test: &str) {
    eprintln!(
        "{test}: skipped — PENGUIN_TEST_FAMILY is unset, so there is no matrix target to talk \
         to. The server-matrix CI job sets it."
    );
}

/// The port the target is on. The CI job publishes 6379; running two lines side by side
/// locally to check that a version-dependent assertion really discriminates needs another.
fn port() -> u16 {
    std::env::var("PENGUIN_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(6379)
}

fn connect() -> Oneshot {
    Oneshot::connect("127.0.0.1", port(), TIMEOUT).expect(
        "the matrix job started a server on 127.0.0.1:6379; if this fails the job is broken, \
         not the code",
    )
}

fn argv(parts: &[&str]) -> Vec<Bytes> {
    parts
        .iter()
        .map(|p| Bytes::from((*p).to_owned().into_bytes()))
        .collect()
}

fn text(v: &Value) -> String {
    match v {
        Value::Simple(b) | Value::Bulk(b) | Value::Error(b) | Value::BulkError(b) => {
            String::from_utf8_lossy(b).into_owned()
        }
        Value::Integer(i) => i.to_string(),
        other => format!("{other:?}"),
    }
}

/// `INFO server`, as a map of its `key:value` lines.
fn info_server(c: &mut Oneshot) -> std::collections::BTreeMap<String, String> {
    let v = c.call(&argv(&["INFO", "server"])).expect("INFO server");
    let body = text(&v);
    body.lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

// =============================================================================================

#[test]
fn live_the_server_is_the_version_the_matrix_says() {
    let Some(t) = target() else {
        return skipped("live_the_server_is_the_version_the_matrix_says");
    };
    let mut c = connect();
    let info = info_server(&mut c);
    // valkey reports both `valkey_version` and, for compatibility, `redis_version`.
    let version = info
        .get("valkey_version")
        .or_else(|| info.get("redis_version"))
        .unwrap_or_else(|| panic!("INFO server had no version: {info:?}"));
    assert!(
        version.starts_with(&t.line),
        "the matrix says line {} and the server says {version}; the digest and the manifest \
         disagree, which is the whole thing this job exists to notice",
        t.line
    );
}

#[test]
fn live_the_family_is_what_the_matrix_says() {
    let Some(t) = target() else {
        return skipped("live_the_family_is_what_the_matrix_says");
    };
    let mut c = connect();
    let info = info_server(&mut c);
    match t.family.as_str() {
        "valkey" => assert!(
            info.contains_key("valkey_version"),
            "the matrix says valkey but INFO server has no valkey_version: {info:?}"
        ),
        "redis" => assert!(
            !info.contains_key("valkey_version"),
            "the matrix says redis but the server reports a valkey_version: {info:?}"
        ),
        other => panic!("unknown family {other}"),
    }
}

#[test]
fn live_resp3_is_available() {
    let Some(_) = target() else {
        return skipped("live_resp3_is_available");
    };
    // `tested = ["RESP2", "RESP3"]` in the manifest is a claim about the server, and this is
    // the claim. A map reply is the observable difference: RESP2 has no `%`.
    let mut c = connect();
    let v = c.call(&argv(&["HELLO", "3"])).expect("HELLO 3");
    assert!(
        matches!(v, Value::Map(_)),
        "HELLO 3 did not answer with a RESP3 map: {v:?}"
    );
    // And the connection is usable afterwards, which is the part that matters: a handshake
    // that leaves the socket in a state nothing can read from is not support.
    let pong = c.call(&argv(&["PING"])).expect("PING after HELLO 3");
    assert_eq!(text(&pong), "PONG");
}

#[test]
fn live_hash_field_ttl_matches_the_line() {
    let Some(t) = target() else {
        return skipped("live_hash_field_ttl_matches_the_line");
    };
    // The manifest says 7.2 is "pre-Hash-field-TTL" and 7.4 "introduces … Hash field TTL".
    // Those are the sentences under test.
    let expected = !t.line.starts_with("7.2");
    let mut c = connect();
    let key = format!("live:hfttl:{}", std::process::id());
    c.call(&argv(&["DEL", &key])).expect("DEL");
    c.call(&argv(&["HSET", &key, "f", "v"])).expect("HSET");
    let v = c
        .call(&argv(&["HEXPIRE", &key, "100", "FIELDS", "1", "f"]))
        .expect("HEXPIRE");
    let supported =
        !matches!(&v, Value::Error(e) if String::from_utf8_lossy(e).contains("unknown command"));
    c.call(&argv(&["DEL", &key])).expect("DEL");
    assert_eq!(
        supported,
        expected,
        "line {} {} hash-field TTL, but the server said {v:?}",
        t.line,
        if expected {
            "should have"
        } else {
            "should not have"
        }
    );
}

#[test]
fn live_hscan_novalues_matches_the_line() {
    let Some(t) = target() else {
        return skipped("live_hscan_novalues_matches_the_line");
    };
    // ADR-019 turns this into a behaviour difference the product has to handle: without
    // NOVALUES, field discovery either drags every value across the network or stops. Which
    // side of the line a target is on is therefore a compatibility claim, not trivia.
    let expected = !t.line.starts_with("7.2");
    let mut c = connect();
    let key = format!("live:novalues:{}", std::process::id());
    c.call(&argv(&["DEL", &key])).expect("DEL");
    c.call(&argv(&["HSET", &key, "f", "v"])).expect("HSET");
    let v = c
        .call(&argv(&["HSCAN", &key, "0", "NOVALUES"]))
        .expect("HSCAN");
    let supported = !matches!(&v, Value::Error(_));
    c.call(&argv(&["DEL", &key])).expect("DEL");
    assert_eq!(
        supported,
        expected,
        "line {} {} HSCAN NOVALUES, but the server said {v:?}",
        t.line,
        if expected {
            "should have"
        } else {
            "should not have"
        }
    );
}

#[test]
fn live_a_binary_value_survives_the_round_trip() {
    let Some(_) = target() else {
        return skipped("live_a_binary_value_survives_the_round_trip");
    };
    // ADR-005 on a real server rather than a fixture: every byte, including NUL and things
    // that are not UTF-8, comes back exactly as it went out.
    let mut c = connect();
    let key = format!("live:binary:{}", std::process::id());
    let payload: Vec<u8> = (0u8..=255).collect();
    let mut set = argv(&["SET", &key]);
    set.push(Bytes::from(payload.clone()));
    c.call(&set).expect("SET");
    let got = c.call(&argv(&["GET", &key])).expect("GET");
    c.call(&argv(&["DEL", &key])).expect("DEL");
    match got {
        Value::Bulk(b) => assert_eq!(b.as_ref(), payload.as_slice(), "the bytes changed"),
        other => panic!("GET did not return a bulk string: {other:?}"),
    }
}
