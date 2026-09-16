//! V-H05 — the soak runner and, more importantly, the analysis that decides what it found
//! (v2.1 §32.1 Soak, PERF-04).
//!
//! > | Soak | 高频命令、连接切换、消息流、反复开关 TUI | 8h 基线、24h 发布周期场景 |
//! > | PERF-04 | 8h/24h 会话 | 稳态资源，**无线性泄漏** |
//!
//! Running for eight hours is the easy half. The half that decides whether the eight hours
//! were worth anything is [`analyse`]: a leak of a few kilobytes per minute is invisible in a
//! graph and obvious in a slope, and "the number at the end was fine" misses a process that
//! grew for six hours and was collected once.
//!
//! So the analysis is a least-squares fit over the whole series, reported as **bytes per
//! hour** with a residual spread, and the verdict distinguishes three things a single
//! threshold cannot:
//!
//! - **growth** — the slope is positive and large relative to the noise
//! - **steady** — the slope is within the noise, which is what a healthy process looks like
//! - **too short** — not enough samples to say either, which must not read as a pass

// A least-squares fit over byte counts and sample indices. `f64` cannot lose anything
// meaningful at these magnitudes, and integer arithmetic on a regression would make the
// formula unreadable for no gain.
#![allow(clippy::cast_precision_loss)]

use serde::{Deserialize, Serialize};

/// One sample, written as a line of JSON so a run can be analysed while it is still going.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Sample {
    /// Seconds since the run started.
    pub at_s: f64,
    /// Resident set size, bytes.
    pub rss_bytes: u64,
    /// Live heap bytes from the counting allocator.
    pub heap_bytes: u64,
    /// Tasks alive in the supervision scope. Should return to its baseline every cycle.
    pub live_tasks: usize,
    /// Commands issued so far, so a stall is visible as a flat line rather than as silence.
    pub commands: u64,
    /// Push messages received so far.
    pub pushes: u64,
    /// TUI open/close cycles so far.
    pub tui_cycles: u64,
}

/// What the slope says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The series is flat within its own noise.
    Steady,
    /// The series rises faster than its noise explains.
    Growing,
    /// Too few samples, or too short a window, to decide. Never a pass.
    TooShort,
}

/// The result of fitting one metric.
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    /// Least-squares slope, in units per hour.
    pub per_hour: f64,
    /// Value the fit starts from.
    pub intercept: f64,
    /// Standard deviation of the residuals, in the metric's units.
    pub residual_sd: f64,
    /// How much the whole run rose or fell, end minus start of the fitted line.
    pub total_change: f64,
    /// Samples used.
    pub samples: usize,
    /// Span covered, in hours.
    pub hours: f64,
    /// The verdict.
    pub verdict: Verdict,
}

/// Fit a metric against time and decide whether it grows.
///
/// `noise_multiple` is how many residual standard deviations of total rise counts as growth.
/// Three is the usual "clearly not noise" bar and is what this uses by default; it is a
/// parameter so a test can prove the detector reacts to both a flat and a rising series
/// rather than being tuned until the real run passes.
#[must_use]
pub fn analyse(points: &[(f64, f64)], noise_multiple: f64, min_hours: f64) -> Fit {
    let n = points.len();
    if n < 30 {
        return Fit {
            per_hour: 0.0,
            intercept: 0.0,
            residual_sd: 0.0,
            total_change: 0.0,
            samples: n,
            hours: 0.0,
            verdict: Verdict::TooShort,
        };
    }
    let hours = (points[n - 1].0 - points[0].0) / 3600.0;

    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n as f64;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    let intercept = mean_y - slope * mean_x;

    let residual_sd = {
        let ss: f64 = points
            .iter()
            .map(|p| (p.1 - (intercept + slope * p.0)).powi(2))
            .sum();
        (ss / (n as f64 - 2.0)).sqrt()
    };

    let total_change = slope * (points[n - 1].0 - points[0].0);
    let verdict = if hours < min_hours {
        Verdict::TooShort
    } else if total_change > noise_multiple * residual_sd && total_change > 0.0 {
        Verdict::Growing
    } else {
        Verdict::Steady
    };

    Fit {
        per_hour: slope * 3600.0,
        intercept,
        residual_sd,
        total_change,
        samples: n,
        hours,
        verdict,
    }
}

/// Read a metrics file written by the runner.
///
/// # Errors
/// The IO or parse failure, with the line number, because a half-written last line is the
/// normal state of a file from a run that is still going.
pub fn read_samples(path: &std::path::Path) -> Result<Vec<Sample>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Sample>(line) {
            Ok(s) => out.push(s),
            // The last line of a live run is often incomplete. Everything before it is data.
            Err(_) if i + 1 == text.lines().count() => break,
            Err(e) => return Err(format!("{}:{}: {e}", path.display(), i + 1)),
        }
    }
    Ok(out)
}

/// The full report for a run.
#[derive(Clone, Copy, Debug)]
pub struct Report {
    /// RSS over time.
    pub rss: Fit,
    /// Heap over time.
    pub heap: Fit,
    /// Live task count over time. A supervision leak shows here before it shows in memory.
    pub tasks: Fit,
    /// Commands issued, which must *rise* -- a flat line means the run stalled.
    pub commands: Fit,
}

impl Report {
    /// Whether the run passes PERF-04.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.rss.verdict == Verdict::Steady
            && self.heap.verdict == Verdict::Steady
            && self.tasks.verdict == Verdict::Steady
            // The one metric that must grow. Without this, a runner that crashed after two
            // minutes would report perfectly steady memory.
            && self.commands.verdict == Verdict::Growing
    }
}

/// Analyse a whole run.
#[must_use]
pub fn report(samples: &[Sample], min_hours: f64) -> Report {
    let series = |f: fn(&Sample) -> f64| -> Vec<(f64, f64)> {
        samples.iter().map(|s| (s.at_s, f(s))).collect()
    };
    Report {
        rss: analyse(&series(|s| s.rss_bytes as f64), 3.0, min_hours),
        heap: analyse(&series(|s| s.heap_bytes as f64), 3.0, min_hours),
        tasks: analyse(&series(|s| s.live_tasks as f64), 3.0, min_hours),
        commands: analyse(&series(|s| s.commands as f64), 3.0, min_hours),
    }
}
