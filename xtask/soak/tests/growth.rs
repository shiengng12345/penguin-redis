//! V-H05 — the growth detector, tested against series whose answer is known (PERF-04).
//!
//! The eight hours are the cheap part. The part that decides whether they meant anything is
//! this: a detector that cannot tell a leak from noise turns a long test into a long wait.
//!
//! So it is checked against synthetic series first — flat, noisy-flat, slowly rising, rising
//! under noise, sawtooth — because a detector validated only on the real run is a detector
//! tuned until the real run passed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]
#![allow(clippy::cast_precision_loss)]

use soak::{Sample, Verdict, analyse, report};

/// A deterministic pseudo-random sequence, so a failure is reproducible.
fn noise(seed: u64, n: usize) -> Vec<f64> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 2_000) as f64 - 1_000.0
        })
        .collect()
}

fn series(n: usize, every_s: f64, f: impl Fn(usize) -> f64) -> Vec<(f64, f64)> {
    (0..n).map(|i| (i as f64 * every_s, f(i))).collect()
}

#[test]
fn a_flat_series_is_steady() {
    let s = series(2_880, 10.0, |_| 50_000_000.0);
    let fit = analyse(&s, 3.0, 1.0);
    assert_eq!(fit.verdict, Verdict::Steady);
    assert!(fit.per_hour.abs() < 1.0, "{fit:?}");
}

#[test]
fn a_noisy_flat_series_is_still_steady() {
    // Eight hours of a healthy process: RSS wanders by a megabyte and goes nowhere.
    let n = noise(7, 2_880);
    let s = series(2_880, 10.0, |i| 50_000_000.0 + n[i] * 1_000.0);
    let fit = analyse(&s, 3.0, 1.0);
    assert_eq!(fit.verdict, Verdict::Steady, "{fit:?}");
}

#[test]
fn a_slow_leak_is_caught_even_though_it_would_not_show_on_a_graph() {
    // 4 MiB per hour over eight hours. That is 32 MiB total on a 50 MiB process -- visible in
    // hindsight and easy to miss while it happens, which is exactly why the slope is what is
    // measured rather than the endpoint.
    let per_sample = 4.0 * 1024.0 * 1024.0 / 360.0;
    let s = series(2_880, 10.0, |i| 50_000_000.0 + i as f64 * per_sample);
    let fit = analyse(&s, 3.0, 1.0);
    assert_eq!(fit.verdict, Verdict::Growing, "{fit:?}");
    assert!(
        (fit.per_hour - 4.0 * 1024.0 * 1024.0).abs() < 1_000.0,
        "slope is {:.0}/h",
        fit.per_hour
    );
}

#[test]
fn a_leak_hidden_under_noise_is_still_caught() {
    // The case a threshold on the final value misses: the leak is smaller than the noise at
    // any single moment, and larger than it over the run.
    let n = noise(11, 2_880);
    let per_sample = 2.0 * 1024.0 * 1024.0 / 360.0;
    let s = series(2_880, 10.0, |i| {
        50_000_000.0 + i as f64 * per_sample + n[i] * 2_000.0
    });
    let fit = analyse(&s, 3.0, 1.0);
    assert_eq!(fit.verdict, Verdict::Growing, "{fit:?}");
}

#[test]
fn a_sawtooth_that_returns_to_its_baseline_is_steady() {
    // A cache that fills and is evicted. It rises and falls by 20 MiB and leaks nothing, and
    // a detector that flagged this would be turned off within a week.
    let s = series(2_880, 10.0, |i| {
        50_000_000.0 + ((i % 180) as f64 / 180.0) * 20_000_000.0
    });
    let fit = analyse(&s, 3.0, 1.0);
    assert_eq!(fit.verdict, Verdict::Steady, "{fit:?}");
}

#[test]
fn a_short_run_is_never_a_pass() {
    // The failure this prevents: a runner that died after two minutes reporting perfectly
    // steady memory.
    let s = series(2_880, 10.0, |_| 50_000_000.0);
    let fit = analyse(&s, 3.0, 24.0);
    assert_eq!(fit.verdict, Verdict::TooShort);

    let few = series(10, 10.0, |_| 50_000_000.0);
    let short = analyse(&few, 3.0, 0.0);
    assert_eq!(short.verdict, Verdict::TooShort);

    // And it has to say *how* short. A three-minute smoke run and a run that died at hour
    // seven both used to print `n=18 over 0.00h`, and the second is the one somebody has to be
    // able to tell apart from a success.
    assert_eq!(short.samples, 10);
    assert!(
        (short.hours - 90.0 / 3600.0).abs() < 1e-9,
        "a too-short fit must still report its span, got {}h",
        short.hours
    );
    let one = analyse(&series(1, 10.0, |_| 1.0), 3.0, 0.0);
    assert!(one.hours.abs() < f64::EPSILON, "one sample spans nothing");
}

#[test]
fn a_run_that_stalled_does_not_pass_however_steady_its_memory() {
    // Commands must *rise*. A process that stopped doing anything has perfectly flat memory,
    // and that is the most misleading pass a soak could report.
    let flat = |i: usize| Sample {
        at_s: i as f64 * 10.0,
        rss_bytes: 50_000_000,
        heap_bytes: 10_000_000,
        live_tasks: 3,
        commands: 1_000,
        pushes: 500,
        tui_cycles: 7,
    };
    let stalled: Vec<Sample> = (0..2_880).map(flat).collect();
    let r = report(&stalled, 1.0);
    assert_eq!(r.rss.verdict, Verdict::Steady);
    assert_eq!(r.commands.verdict, Verdict::Steady, "commands did not rise");
    assert!(!r.ok(), "a stalled run reported a pass");

    // And a healthy run does pass.
    let healthy: Vec<Sample> = (0..2_880)
        .map(|i| Sample {
            commands: 1_000 + i as u64 * 50,
            ..flat(i)
        })
        .collect();
    let r = report(&healthy, 1.0);
    assert!(r.ok(), "a healthy run failed: {r:?}");
}

#[test]
fn a_task_leak_fails_even_when_memory_looks_fine() {
    // A supervision leak shows in the live task count long before it shows in bytes -- which
    // is why `TaskScope` exposes a count at all.
    let leaky: Vec<Sample> = (0..2_880)
        .map(|i| Sample {
            at_s: i as f64 * 10.0,
            rss_bytes: 50_000_000,
            heap_bytes: 10_000_000,
            live_tasks: 3 + i / 50,
            commands: 1_000 + i as u64 * 50,
            pushes: 500,
            tui_cycles: 7,
        })
        .collect();
    let r = report(&leaky, 1.0);
    assert_eq!(r.rss.verdict, Verdict::Steady);
    assert_eq!(r.tasks.verdict, Verdict::Growing);
    assert!(!r.ok());
}
