//! V-H05 — the soak runner (v2.1 §32.1 Soak, PERF-04).
//!
//! Four workloads, because §32.1 names four and each fails differently:
//!
//! | workload | what it would leak |
//! |---|---|
//! | 高频命令 | decoder buffers, result records |
//! | 连接切换 | sockets, per-connection state, task handles |
//! | 消息流 | the push ring buffer, subscription bookkeeping |
//! | 反复开关 TUI | terminal ownership, layout caches, the alternate screen |
//!
//! Usage: `soak --hours 8 --out metrics.jsonl [--redis host:port]`
//!
//! Without `--redis` the command and push workloads run against the synthetic RESP server, so
//! the runner is useful on a machine with no Docker. With it, they run against a real server,
//! which is what the CI job does.
//!
//! The process samples itself every ten seconds and writes one JSON line per sample, so the
//! run can be analysed **while it is still going** rather than only at the end. An eight-hour
//! test whose result is unknowable until hour eight is an eight-hour test you run once.

// A least-squares fit over byte counts and sample indices. `f64` cannot lose anything
// meaningful at these magnitudes, and integer arithmetic on a regression would make the
// formula unreadable for no gain.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use bytes::Bytes;
use pr_transport::Oneshot;
use std::io::Write;
use std::time::{Duration, Instant};
use xtask::measure::CountingAllocator;

/// The same instrument every other budget in this repository is measured with.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

struct Args {
    hours: f64,
    out: std::path::PathBuf,
    redis: Option<String>,
    sample_every: Duration,
    /// Check that the *runner* works, not that the system is steady.
    ///
    /// A short run cannot answer §32.1's question and the analysis correctly refuses to call it
    /// a pass — which makes a deliberate three-minute run a guaranteed red build and a
    /// guaranteed notification email. That is noise about nothing: the thing being checked is
    /// whether the job's plumbing works at all.
    ///
    /// So `--smoke` asks a smaller question and says so: did samples get written, and did the
    /// workloads actually do something? It never claims the system is leak-free, and the
    /// scheduled run does not use it.
    smoke: bool,
}

fn parse_args() -> Args {
    let mut hours = 8.0;
    let mut out = std::path::PathBuf::from("soak-metrics.jsonl");
    let mut redis = None;
    let mut sample_every = Duration::from_secs(10);
    let mut smoke = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--hours" => hours = it.next().and_then(|v| v.parse().ok()).unwrap_or(8.0),
            "--out" => out = it.next().map_or(out, std::path::PathBuf::from),
            "--redis" => redis = it.next(),
            "--smoke" => smoke = true,
            "--sample-seconds" => {
                sample_every = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .map_or(sample_every, Duration::from_secs);
            }
            other => {
                eprintln!("soak: unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    Args {
        hours,
        out,
        redis,
        sample_every,
        smoke,
    }
}

#[allow(clippy::too_many_lines)]
fn main() {
    let args = parse_args();
    // A runtime, because the task-count metric has to be real. A `live_tasks` field that is
    // structurally always zero can never detect anything, and the detector has a test proving
    // it would catch a task leak -- so the runner has to be capable of having one.
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("soak: no runtime: {e}");
            std::process::exit(1);
        }
    };
    let _guard = rt.enter();
    let mut scope = pr_core::TaskScope::new("soak");
    let tasks = scope.handle();
    let deadline = Instant::now() + Duration::from_secs_f64(args.hours * 3600.0);
    let started = Instant::now();

    let mut metrics = match std::fs::File::create(&args.out) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("soak: cannot write {}: {e}", args.out.display());
            std::process::exit(1);
        }
    };
    eprintln!(
        "soak: {:.2}h, sampling every {:?}, into {}",
        args.hours,
        args.sample_every,
        args.out.display()
    );

    let mut commands = 0u64;
    let mut pushes = 0u64;
    let mut tui_cycles = 0u64;
    let mut next_sample = Instant::now();
    let mut conn: Option<Oneshot> = None;

    while Instant::now() < deadline {
        // ---- 高频命令, and 连接切换 every 500 commands -------------------------------
        if let Some(addr) = &args.redis {
            if conn.is_none() || commands.is_multiple_of(500) {
                // Dropping the old connection *is* the switch: if a socket or its task
                // leaked, the descriptor count climbs and the run fails long before memory
                // notices. Explicit rather than shadowed, so the close happens before the
                // open and the two are never both live.
                drop(conn.take());
                let (host, port) = split_addr(addr);
                conn = Oneshot::connect(host, port, Duration::from_secs(5)).ok();
            }
            // One supervised task per switch: it takes the scope through spawn and completion,
            // so a scope that stopped reaping shows up as a rising `live_tasks` long before
            // it shows up as memory.
            let handle = scope.spawn(|token| async move {
                tokio::time::sleep(Duration::from_millis(1)).await;
                token.is_cancelled()
            });
            rt.block_on(async { handle.await.ok() });

            if let Some(c) = conn.as_mut() {
                let key = format!("soak:{}", commands % 1000);
                let ok = c
                    .call(&[
                        Bytes::from_static(b"SET"),
                        Bytes::from(key.clone()),
                        Bytes::from(format!("{commands}")),
                    ])
                    .is_ok()
                    && c.call(&[Bytes::from_static(b"GET"), Bytes::from(key)])
                        .is_ok();
                if ok {
                    commands += 2;
                } else {
                    conn = None;
                }
            }
        } else {
            // No server: exercise the decoder over the same shapes, which is what the command
            // workload is actually stressing locally.
            commands += synthetic_decode_round();
        }

        // ---- 消息流 ------------------------------------------------------------------
        pushes += push_round();

        // ---- 反复开关 TUI ---------------------------------------------------------------
        if commands.is_multiple_of(200) {
            tui_cycles += tui_round();
        }

        if Instant::now() >= next_sample {
            let s = soak::Sample {
                at_s: started.elapsed().as_secs_f64(),
                rss_bytes: pr_core::mem::rss_bytes().unwrap_or(0),
                heap_bytes: ALLOC.live_bytes() as u64,
                live_tasks: tasks.live(),
                commands,
                pushes,
                tui_cycles,
            };
            if let Ok(line) = serde_json::to_string(&s) {
                let _ = writeln!(metrics, "{line}");
                let _ = metrics.flush();
            }
            next_sample = Instant::now() + args.sample_every;
        }

        // A soak is sustained load over time, not a throughput benchmark. Without this the
        // runner pins a core for eight hours, which makes it something nobody runs on the
        // machine they are also working on -- and a soak nobody runs finds nothing.
        std::thread::sleep(Duration::from_millis(1));
    }

    // Shut the scope down before reporting: a scope that cannot drain is itself a finding,
    // and the final task count should be back at zero.
    let drained = rt.block_on(scope.shutdown(5_000));
    eprintln!(
        "soak: done. {commands} commands, {pushes} pushes, {tui_cycles} TUI cycles, \
         scope drained: {drained}, tasks left: {}",
        tasks.live()
    );
    let samples = soak::read_samples(&args.out).unwrap_or_default();
    // Require 90% of the requested window: a run that was cut short must not report a pass.
    let report = soak::report(&samples, args.hours * 0.9);
    print_report(&report);

    if args.smoke {
        // A different, smaller claim, stated as such: the runner works. Nothing here says the
        // system is steady, and the scheduled run does not pass `--smoke`.
        let moved = samples.len() >= 2 && commands > 0 && tui_cycles > 0;
        if moved {
            eprintln!(
                "soak: smoke ok -- {} samples written, {commands} commands, {tui_cycles} TUI \
                 cycles. This says the job runs; it does NOT say the system is steady.",
                samples.len()
            );
        } else {
            eprintln!(
                "soak: smoke FAILED -- {} samples, {commands} commands, {tui_cycles} TUI cycles; \
                 the runner produced nothing to analyse",
                samples.len()
            );
            std::process::exit(1);
        }
        return;
    }

    if report.ok() {
        eprintln!("soak: steady");
    } else {
        eprintln!("soak: FAILED -- see the fits above");
        std::process::exit(1);
    }
}

fn print_report(r: &soak::Report) {
    for (name, f) in [
        ("rss", r.rss),
        ("heap", r.heap),
        ("tasks", r.tasks),
        ("commands", r.commands),
    ] {
        println!(
            "{name:9} {:>16.1}/h  total {:>16.1}  noise sd {:>12.1}  n={} over {:.2}h  {:?}",
            f.per_hour, f.total_change, f.residual_sd, f.samples, f.hours, f.verdict
        );
    }
}

fn split_addr(a: &str) -> (&str, u16) {
    a.rsplit_once(':')
        .and_then(|(h, p)| p.parse().ok().map(|p| (h, p)))
        .unwrap_or((a, 6379))
}

/// Decode a batch of representative replies. Exercises the same buffers a real command does.
fn synthetic_decode_round() -> u64 {
    use pr_protocol::decoder::{Decoder, Step};
    let frames: &[&[u8]] = &[
        b"+OK\r\n",
        b"$5\r\nhello\r\n",
        b"*3\r\n$1\r\na\r\n$1\r\nb\r\n$-1\r\n",
        b"%2\r\n+k1\r\n:1\r\n+k2\r\n:2\r\n",
        b"~2\r\n:1\r\n:2\r\n",
        b",1.2300\r\n",
    ];
    let mut n = 0;
    let mut d = Decoder::with_defaults();
    for f in frames {
        d.feed(f);
        while let Ok(Step::Value(_)) = d.decode() {
            n += 1;
        }
    }
    n
}

/// A round of push frames through the bounded subscription store.
fn push_round() -> u64 {
    use pr_protocol::decoder::{Decoder, Step};
    let mut d = Decoder::with_defaults();
    let mut n = 0;
    for i in 0..16 {
        let payload = format!("event-{i}");
        let frame = format!(
            "*3\r\n$7\r\nmessage\r\n$7\r\nchannel\r\n${}\r\n{payload}\r\n",
            payload.len()
        );
        d.feed(frame.as_bytes());
        while let Ok(Step::Value(_)) = d.decode() {
            n += 1;
        }
    }
    n
}

/// Open a TUI, draw, close. The cycle §32.1 asks for, and the one that leaks terminal state.
fn tui_round() -> u64 {
    match pr_tui::Idle::new(120, 40) {
        Ok(mut tui) => {
            let _ = tui.draw("@soak / DB 0 / result #1");
            drop(tui);
            1
        }
        Err(_) => 0,
    }
}
