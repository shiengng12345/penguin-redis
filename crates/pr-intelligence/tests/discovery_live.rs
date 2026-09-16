//! V-F07 — explicit discovery against 10^6 real keys (v2.1 §12.4, §12.5, R08, R09, R32).
//!
//! The pass criterion is 「稀疏前缀有结果并报告完成度」, and the fallback clause is the
//! interesting part:
//!
//! > 若默认预算在 10⁶ key 上仍空 → **调高默认值（不是删功能）** 并记 ADR
//!
//! So this test is a measurement, not a assertion that a number is already right. It runs the
//! §12.5 default budget against a million keys with ten `player:*` needles in them, and
//! reports what that budget actually finds. Whatever it finds is the input to ADR-033.
//!
//! `#[ignore]` because it needs Docker and a minute; run by the discovery CI job with
//! `--ignored`. The pure half in `discovery.rs` runs unconditionally.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use bytes::Bytes;
use pr_intelligence::discovery::{Budget, Completeness, Discovery, MatchMode, Step};
use pr_protocol::value::Value;
use pr_transport::Oneshot;
use std::time::{Duration, Instant};

const IMAGE: &str = "redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7";
const NAME: &str = "prvf07-redis";
const TOTAL_KEYS: u64 = 1_000_000;
const NEEDLES: u64 = 10;
const TIMEOUT: Duration = Duration::from_secs(30);

fn docker(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("docker")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

struct Server {
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        docker(&["rm", "-f", NAME]);
    }
}

/// A million keys, with `NEEDLES` sparse `player:*` keys among them.
fn million_key_server() -> Server {
    docker(&["rm", "-f", NAME]);
    // `--enable-debug-command yes` is what makes DEBUG POPULATE available; it is confined to
    // this throwaway container, which is what §32.2 means by an isolated test environment.
    let id = docker(&[
        "run",
        "-d",
        "--name",
        NAME,
        "-P",
        IMAGE,
        "redis-server",
        "--enable-debug-command",
        "yes",
        "--save",
        "",
    ])
    .expect("start redis");
    assert!(!id.trim().is_empty());
    let guard = Server { port: 0 };
    std::mem::forget(guard); // replaced below once the port is known

    let mut port = None;
    for _ in 0..200 {
        if docker(&["exec", NAME, "redis-cli", "PING"]).is_some_and(|o| o.trim() == "PONG") {
            port = docker(&["port", NAME, "6379/tcp"]).and_then(|o| {
                o.lines()
                    .next()?
                    .rsplit(':')
                    .next()?
                    .trim()
                    .parse::<u16>()
                    .ok()
            });
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let port = port.expect("redis never became ready");

    // DEBUG POPULATE is instantaneous and deterministic: `key:0` … `key:999999`.
    let out = docker(&[
        "exec",
        NAME,
        "redis-cli",
        "DEBUG",
        "POPULATE",
        &TOTAL_KEYS.to_string(),
    ])
    .expect("DEBUG POPULATE");
    assert!(out.contains("OK"), "DEBUG POPULATE said {out:?}");

    // The needles, spread across the keyspace by name rather than clustered, because a
    // clustered set would be found or missed as a block and tell us less.
    for i in 0..NEEDLES {
        let k = format!("player:{}", i * 7919);
        docker(&["exec", NAME, "redis-cli", "SET", &k, "x"]).expect("SET");
    }

    let n = docker(&["exec", NAME, "redis-cli", "DBSIZE"]).expect("DBSIZE");
    let n: u64 = n.trim().parse().expect("DBSIZE is a number");
    assert_eq!(
        n,
        TOTAL_KEYS + NEEDLES,
        "the fixture is not the size this test is about"
    );

    Server { port }
}

/// Drive a `Discovery` to completion against a real server.
fn run(port: u16, d: &mut Discovery) -> Duration {
    let mut conn = Oneshot::connect("127.0.0.1", port, TIMEOUT).expect("connect");
    let started = Instant::now();
    loop {
        match d.next_step(Instant::now()) {
            Step::Done(_) => break,
            Step::Wait(w) => std::thread::sleep(w),
            Step::Scan {
                cursor,
                pattern,
                count,
            } => {
                let argv = vec![
                    Bytes::from_static(b"SCAN"),
                    Bytes::from(cursor.to_string()),
                    Bytes::from_static(b"MATCH"),
                    Bytes::from(pattern),
                    Bytes::from_static(b"COUNT"),
                    Bytes::from(count.to_string()),
                ];
                let reply = conn.call(&argv).expect("SCAN");
                let (next, keys, bytes) = parse_scan(&reply);
                d.feed(next, &keys, bytes);
            }
        }
    }
    started.elapsed()
}

fn parse_scan(v: &Value) -> (u64, Vec<Vec<u8>>, usize) {
    let Value::Array(items) = v else {
        panic!("SCAN did not return an array: {v:?}");
    };
    let [cursor, keys] = items.as_slice() else {
        panic!("SCAN returned {} elements", items.len());
    };
    let Value::Bulk(c) = cursor else {
        panic!("the cursor is not a bulk string");
    };
    let next: u64 = std::str::from_utf8(c).unwrap().parse().unwrap();
    let Value::Array(ks) = keys else {
        panic!("the key list is not an array");
    };
    let mut out = Vec::with_capacity(ks.len());
    let mut bytes = c.len() + 16;
    for k in ks {
        let Value::Bulk(b) = k else {
            panic!("a key is not a bulk string");
        };
        bytes += b.len() + 8;
        out.push(b.to_vec());
    }
    (next, out, bytes)
}

#[test]
#[ignore = "needs Docker and a million keys; run by the discovery CI job with --ignored"]
fn the_default_explicit_budget_measured_against_a_million_keys() {
    let srv = million_key_server();

    // The measurement: §12.5's shipped default, unchanged.
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget::explicit_default(),
    );
    let elapsed = run(srv.port, &mut d);
    let o = d.outcome();

    eprintln!(
        "V-F07 default budget on {} keys ({} needles): matched {}, scans {}, keys seen {}, \
         bytes {}, elapsed {:?}, completeness {:?}",
        TOTAL_KEYS,
        NEEDLES,
        o.progress.matched,
        o.progress.scans_used,
        o.progress.keys_seen,
        o.progress.bytes_seen,
        elapsed,
        o.completeness
    );
    eprintln!("V-F07 summary line: {}", o.summary());

    // What is asserted is the *contract*, not the count -- the count is the thing being
    // measured, and asserting a number here would make the measurement circular.
    assert!(
        !o.completeness.is_conclusive() || o.matches.len() as u64 == NEEDLES,
        "a conclusive run must have found every needle: {o:?}"
    );
    if o.matches.is_empty() {
        let m = o.summary();
        assert!(
            m.contains("discovery incomplete"),
            "an empty result on an incomplete walk must say so: {m}"
        );
        assert!(
            !m.to_lowercase().contains("does not exist"),
            "an empty result claimed absence: {m}"
        );
    }
    // Every match really is a needle: the pattern did what it said.
    for k in &o.matches {
        assert!(
            k.starts_with(b"player:"),
            "{:?} does not match the prefix",
            String::from_utf8_lossy(k)
        );
    }
    assert!(
        elapsed <= Duration::from_secs(12),
        "the time budget did not bound the run: {elapsed:?}"
    );
}

#[test]
#[ignore = "needs Docker and a million keys; run by the discovery CI job with --ignored"]
fn the_scan_count_and_the_time_budget_now_bind_together() {
    // ADR-033's evidence. The measurement above found the old default stopping at 50 SCANs
    // after 2.58 s of a 10 s budget: the scan count was pre-empting the time budget, so the
    // limit that bit was not the one the user experiences.
    //
    // 50 SCANs at 20 requests/second is 2.5 seconds. Ten seconds at that rate is 200. Setting
    // `max_scans` to 200 makes the two budgets describe the same stopping point instead of one
    // silently winning at a quarter of the other.
    let srv = million_key_server();
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget::explicit_default(),
    );
    let elapsed = run(srv.port, &mut d);
    let o = d.outcome();

    eprintln!(
        "V-F07 adjusted default: matched {}, scans {}, elapsed {:?}, completeness {:?}",
        o.progress.matched, o.progress.scans_used, elapsed, o.completeness
    );

    // The property ADR-033 is about: whichever budget stops the run, it is not the scan count
    // arriving long before the clock. Either the clock ran out, or the walk finished.
    let scans_used = o.progress.scans_used;
    let coherent = match &o.completeness {
        Completeness::Exhausted | Completeness::BudgetReached { limit: "time" } => true,
        // Stopping at the SCAN count is acceptable only if the clock was nearly spent too --
        // that is the mismatch ADR-033 removed.
        Completeness::BudgetReached {
            limit: "SCAN count",
        } => elapsed >= Duration::from_secs(8),
        other => panic!("unexpected completeness {other:?}"),
    };
    assert!(
        coherent,
        "the run stopped at {scans_used} SCANs after {elapsed:?}, so the budgets still \
         disagree about when to stop"
    );
    assert!(
        elapsed <= Duration::from_secs(12),
        "the time budget did not bound the run: {elapsed:?}"
    );
    // What improved is *coverage*, and coverage is what gets asserted. The needle count is a
    // random variable: at roughly 19% of the table walked, the expected number of the ten
    // needles seen is about two, and a run that sees one is not a regression. Asserting on it
    // would be asserting on a coin toss, and §32.5 is explicit that a flaky test must be
    // located rather than papered over.
    //
    // The deterministic improvement: the old default stopped after 50 SCANs and 2.58 s; this
    // one uses its whole budget.
    assert!(
        scans_used > 50,
        "the run still stopped at the old 50-SCAN ceiling: {scans_used} SCANs"
    );
    assert!(
        elapsed >= Duration::from_secs(8) || o.completeness.is_conclusive(),
        "the run used only {elapsed:?} of a 10 s budget, so something else stopped it early"
    );
}

#[test]
#[ignore = "needs Docker and a million keys; run by the discovery CI job with --ignored"]
fn a_raised_budget_finds_every_needle_in_the_same_keyspace() {
    // The fallback clause's other half: 「调高默认值（不是删功能）」. Raising the budget has to
    // actually work, or raising it is not a remedy. This walks the keyspace to the end and
    // requires all ten needles, which also proves the pattern is right -- a wrong pattern
    // would find none however long it ran.
    let srv = million_key_server();
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget {
            max_scans: 100_000,
            count_hint: 1000,
            max_duration: Duration::from_mins(2),
            max_requests_per_sec: 100_000,
            max_response_bytes: 64 * 1024 * 1024,
        },
    );
    let elapsed = run(srv.port, &mut d);
    let o = d.outcome();

    eprintln!(
        "V-F07 exhaustive: matched {}, scans {}, keys seen {}, elapsed {:?}, completeness {:?}",
        o.progress.matched, o.progress.scans_used, o.progress.keys_seen, elapsed, o.completeness
    );

    assert_eq!(o.completeness, Completeness::Exhausted);
    assert_eq!(
        o.matches.len() as u64,
        NEEDLES,
        "an exhaustive walk missed a needle"
    );
    assert!(o.completeness.is_conclusive());
    assert!(o.summary().contains("walked to the end"), "{}", o.summary());
}

#[test]
#[ignore = "needs Docker and a million keys; run by the discovery CI job with --ignored"]
fn the_two_match_modes_send_different_bytes_to_a_real_server() {
    // §12.4's `player:[` case, against a server rather than against my reading of the docs.
    // A literal prefix must find the key named `player:[1]`; a raw glob of the same input is
    // a character class, and the server decides what that means.
    let srv = million_key_server();
    docker(&["exec", NAME, "redis-cli", "SET", "player:[1]", "x"]).expect("SET");

    // Literal prefix: `player:[` escapes to `player:\[*`, which matches `player:[1]`.
    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:[",
        Budget {
            max_scans: 100_000,
            max_requests_per_sec: 100_000,
            max_duration: Duration::from_mins(2),
            ..Budget::explicit_default()
        },
    );
    assert_eq!(d.pattern(), b"player:\\[*");
    run(srv.port, &mut d);
    let o = d.outcome();
    assert_eq!(o.completeness, Completeness::Exhausted);
    assert_eq!(
        o.matches,
        vec![b"player:[1]".to_vec()],
        "the literal prefix did not find the key the user typed a prefix of"
    );

    // Raw glob: the same input is sent untouched. `player:[` is an unterminated class, and
    // whatever Redis makes of it is Redis's business -- what matters is that we did not
    // quietly turn it into something else.
    let mut g = Discovery::new(
        MatchMode::RawGlob,
        b"player:[",
        Budget {
            max_scans: 100_000,
            max_requests_per_sec: 100_000,
            max_duration: Duration::from_mins(2),
            ..Budget::explicit_default()
        },
    );
    assert_eq!(g.pattern(), b"player:[");
    run(srv.port, &mut g);
    let go = g.outcome();
    assert_eq!(go.completeness, Completeness::Exhausted);
    assert!(
        go.matches != o.matches,
        "the two modes produced the same result, so one of them is not doing what it says"
    );
    eprintln!(
        "V-F07 modes: prefix matched {:?}, glob matched {:?}",
        o.matches.len(),
        go.matches.len()
    );
}

#[test]
#[ignore = "needs Docker; run by the discovery CI job with --ignored"]
fn a_server_that_refuses_scan_produces_a_cooldown_and_no_claim_about_the_keyspace() {
    // ASSIST-084 against a real ACL rather than a simulated error, because the thing being
    // tested is that we recognise a refusal when the server actually sends one.
    docker(&["rm", "-f", NAME]);
    let id = docker(&["run", "-d", "--name", NAME, "-P", IMAGE]).expect("start redis");
    assert!(!id.trim().is_empty());
    let mut port = None;
    for _ in 0..200 {
        if docker(&["exec", NAME, "redis-cli", "PING"]).is_some_and(|o| o.trim() == "PONG") {
            port = docker(&["port", NAME, "6379/tcp"]).and_then(|o| {
                o.lines()
                    .next()?
                    .rsplit(':')
                    .next()?
                    .trim()
                    .parse::<u16>()
                    .ok()
            });
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let srv = Server {
        port: port.expect("ready"),
    };

    // A user that may do everything except SCAN.
    docker(&[
        "exec",
        NAME,
        "redis-cli",
        "ACL",
        "SETUSER",
        "noscan",
        "on",
        ">pw",
        "~*",
        "+@all",
        "-scan",
    ])
    .expect("ACL SETUSER");

    let mut conn = Oneshot::connect("127.0.0.1", srv.port, TIMEOUT).unwrap();
    let auth = conn
        .call(&[
            Bytes::from_static(b"AUTH"),
            Bytes::from_static(b"noscan"),
            Bytes::from_static(b"pw"),
        ])
        .unwrap();
    assert!(matches!(auth, Value::Simple(_)), "AUTH failed: {auth:?}");

    let mut d = Discovery::new(
        MatchMode::LiteralPrefix,
        b"player:",
        Budget::explicit_default(),
    );
    let Step::Scan {
        cursor,
        pattern,
        count,
    } = d.next_step(Instant::now())
    else {
        panic!("expected a scan");
    };
    let reply = conn
        .call(&[
            Bytes::from_static(b"SCAN"),
            Bytes::from(cursor.to_string()),
            Bytes::from_static(b"MATCH"),
            Bytes::from(pattern),
            Bytes::from_static(b"COUNT"),
            Bytes::from(count.to_string()),
        ])
        .unwrap();
    let Value::Error(e) = &reply else {
        panic!("expected the server to refuse, got {reply:?}");
    };
    let text = String::from_utf8_lossy(e).into_owned();
    assert!(text.starts_with("NOPERM"), "expected NOPERM, got {text}");

    d.deny(text.clone());
    let o = d.outcome();
    assert!(!o.completeness.is_conclusive());
    let m = o.summary();
    assert!(m.contains("refused"), "{m}");
    assert!(
        m.contains("nothing can be concluded"),
        "a refusal must not read as an absence: {m}"
    );
    eprintln!("V-F07 refusal: {m}");
}
