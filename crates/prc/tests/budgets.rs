//! V-H01 — the startup and idle baselines (v2.1 §24.6, §36, V-A06).
//!
//! §24.6 gives three numbers. The question V-H01 asks is not whether our code fits inside
//! them — there is barely any feature code yet — but whether **the dependency chain has
//! already eaten them**. `prc` therefore links the whole graph: tokio, rusqlite, keyring,
//! crossterm, ratatui, blake3, `serde_json` and the embedded catalog.
//!
//! | budget | §24.6 | measured here |
//! |---|---|---|
//! | `prc --help` | ≤ 100 ms p95 | a real process, 30 runs |
//! | idle REPL RSS | ≤ 30 MiB | a real process holding the REPL's state |
//! | idle TUI RSS | ≤ 60 MiB | a real process holding a drawn Ratatui frame |
//!
//! These run against the **test-profile** binary, which is slower and larger than a release
//! build. Passing here is therefore the conservative result: a release build has strictly
//! more headroom, and a debug build that already fits proves the dependencies have not eaten
//! the budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::process::Command;
use std::time::{Duration, Instant};

const PRC: &str = env!("CARGO_BIN_EXE_prc");

/// §24.6 budgets, in the units the spec states them.
const HELP_P95: Duration = Duration::from_millis(100);
const IDLE_REPL_RSS: u64 = 30 * 1024 * 1024;
const IDLE_TUI_RSS: u64 = 60 * 1024 * 1024;

fn run(args: &[&str], env: &[(&str, &str)]) -> (String, String, i32) {
    let mut c = Command::new(PRC);
    c.args(args);
    // A probe must not inherit a stray probe setting from the test runner's environment.
    c.env_remove("PR_PHASE0_PROBE");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().expect("prc runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn probe_rss(name: &str) -> u64 {
    let (stdout, stderr, code) = run(&[], &[("PR_PHASE0_PROBE", name)]);
    assert_eq!(code, 0, "{name} probe failed: {stderr}");
    let field = stdout
        .split("rss_bytes=")
        .nth(1)
        .unwrap_or_else(|| panic!("no rss in {stdout:?}"))
        .split_whitespace()
        .next()
        .unwrap();
    assert_ne!(
        field, "unavailable",
        "this platform cannot measure RSS, so the budget cannot be checked here"
    );
    field.parse().expect("rss is a number")
}

/// Bytes as mebibytes, for the message a human reads.
///
/// An RSS in bytes is far below the range where `f64` loses anything, and the value is only
/// ever printed — the comparisons are done on the integers.
#[allow(clippy::cast_precision_loss)]
fn mib(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

// ================================================ startup
#[test]
fn help_answers_within_the_startup_budget() {
    // Warm the page cache first; the budget is about what the process does, not about the
    // first read of the binary off disk.
    for _ in 0..3 {
        let _ = run(&["--help"], &[]);
    }

    let mut samples: Vec<Duration> = Vec::with_capacity(30);
    for _ in 0..30 {
        let t = Instant::now();
        let (stdout, _, code) = run(&["--help"], &[]);
        samples.push(t.elapsed());
        assert_eq!(code, 0);
        assert!(stdout.starts_with("prc —"));
    }
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95) / 100];
    let max = samples[samples.len() - 1];

    println!("--help p50={p50:?} p95={p95:?} max={max:?} (budget p95 {HELP_P95:?})");
    assert!(
        p95 <= HELP_P95,
        "startup p95 {p95:?} exceeds the §24.6 budget of {HELP_P95:?}; \
         p50 was {p50:?} and max {max:?}"
    );
}

#[test]
fn help_touches_none_of_the_expensive_subsystems() {
    // This is the structural half of the startup budget, and it is the half that survives a
    // fast machine: a change that makes the catalog eager would still pass a timing check on
    // a quick laptop and then blow the budget on a loaded CI runner. `--help` printing the
    // catalog's provenance would be the giveaway, so the two are kept apart.
    let (help_out, _, _) = run(&["--help"], &[]);
    assert!(!help_out.contains("@sha256:"), "help loaded the catalog");
    assert!(!help_out.contains("catalog integrity"));

    // `--version` is the path that *does* load it, which is what makes the distinction real
    // rather than an accident of the help text's wording.
    let (version_out, _, code) = run(&["--version"], &[]);
    assert_eq!(code, 0);
    assert!(version_out.contains("@sha256:"), "{version_out}");
    assert!(version_out.contains("catalog integrity: verified"));
}

#[test]
fn help_is_answered_even_when_the_rest_of_the_command_line_is_wrong() {
    let (stdout, _, code) = run(&["@nosuch", "--not-a-flag", "--help"], &[]);
    assert_eq!(code, 0, "asking for help must not be a usage error");
    assert!(stdout.contains("USAGE:"));
}

// ================================================ idle memory
#[test]
fn an_idle_repl_fits_the_rss_budget() {
    let rss = probe_rss("repl");
    println!(
        "idle REPL RSS = {:.1} MiB (budget {:.0} MiB)",
        mib(rss),
        mib(IDLE_REPL_RSS)
    );
    assert!(
        rss <= IDLE_REPL_RSS,
        "idle REPL RSS {:.1} MiB exceeds the §24.6 budget of {:.0} MiB — \
         the dependency chain has eaten it",
        mib(rss),
        mib(IDLE_REPL_RSS)
    );
    // And it is not implausibly small, which would mean the probe measured nothing.
    assert!(rss > 1024 * 1024, "the probe measured nothing: {rss}");
}

#[test]
fn an_idle_tui_fits_the_rss_budget() {
    let rss = probe_rss("tui");
    println!(
        "idle TUI RSS = {:.1} MiB (budget {:.0} MiB)",
        mib(rss),
        mib(IDLE_TUI_RSS)
    );
    assert!(
        rss <= IDLE_TUI_RSS,
        "idle TUI RSS {:.1} MiB exceeds the §24.6 budget of {:.0} MiB",
        mib(rss),
        mib(IDLE_TUI_RSS)
    );
}

#[test]
fn the_catalogs_share_of_the_budget_is_attributable() {
    // Knowing the total is not enough: if the budget ever comes under pressure, the decision
    // is about which dependency to change, and that needs the parts.
    let catalog = probe_rss("catalog");
    let repl = probe_rss("repl");
    let tui = probe_rss("tui");
    println!(
        "attribution: catalog {:.1} MiB, repl {:.1} MiB, tui {:.1} MiB",
        mib(catalog),
        mib(repl),
        mib(tui)
    );
    assert!(
        catalog <= IDLE_REPL_RSS,
        "the catalog alone already exceeds the idle REPL budget: {:.1} MiB",
        mib(catalog)
    );
    // The TUI is the REPL plus its frame buffers, so it cannot be the cheaper one.
    assert!(
        tui >= catalog,
        "TUI {:.1} MiB below catalog-only {:.1} MiB — the probe is not measuring what it says",
        mib(tui),
        mib(catalog)
    );
}

#[test]
fn an_unknown_probe_is_refused_rather_than_silently_ignored() {
    // A typo in a measurement run that produces a number anyway is how a budget gets
    // "verified" against nothing.
    let (_, stderr, code) = run(&[], &[("PR_PHASE0_PROBE", "nosuch")]);
    assert_eq!(code, 2, "usage error");
    assert!(stderr.contains("nosuch"), "{stderr}");
}

// ================================================ WIN-02
#[test]
fn win_02_binary_output_reaches_a_pipe_byte_for_byte() {
    // §35.1: no CRLF translation, on any platform. The canary carries the bytes a text-mode
    // pipeline mangles — a CRLF pair, bare `\r` and `\n`, Ctrl+Z, NUL and invalid UTF-8 —
    // because a test with a friendly string would pass on a platform that corrupts data.
    let mut c = Command::new(PRC);
    c.env("PR_PHASE0_PROBE", "binary-out");
    let out = c.output().expect("prc runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let expected: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(b"start\r\n\r\n\x1a\0");
        v.extend_from_slice(&[0xff, 0xfe]);
        v.extend_from_slice("中文".as_bytes());
        v.extend_from_slice(b"end");
        v
    };
    assert_eq!(
        out.stdout, expected,
        "bytes were altered on the way out:\n  got {:?}\n  want {:?}",
        out.stdout, expected
    );
    assert_eq!(
        out.stdout.windows(3).filter(|w| *w == b"\r\r\n").count(),
        0,
        "a CRLF pair was translated into CR CR LF"
    );
    assert!(
        !out.stdout.ends_with(b"\n") || out.stdout.ends_with(b"end"),
        "no delimiter may be appended: --bytes is the blob and nothing else"
    );
}

#[test]
fn every_probe_reports_the_whole_catalog() {
    // If a probe quietly measured an empty catalog, its RSS would pass any budget.
    for name in ["catalog", "repl", "tui"] {
        let (stdout, _, _) = run(&[], &[("PR_PHASE0_PROBE", name)]);
        let n: usize = stdout
            .split("commands=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(n > 500, "{name} probe saw only {n} commands");
    }
}
