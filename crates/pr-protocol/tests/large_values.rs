//! V-H02 / PERF-01 — a 1 GiB synthetic blob and a real 512 MB Redis string (v2.1 §24.7).
//!
//! §24.7 explains why this needs two sources and insists they be reported apart:
//!
//! > 官方字符串文档列出默认最大字符串大小为 512 MB。[R52] 因此 1 GiB 单 blob 的测试默认使用
//! > synthetic RESP fixture server，验证解码/输出的增量行为；真 Redis 测试按固定版本和实际
//! > 配置的数据上限生成，不把调大服务器限制或 mock 结果冒充默认真实部署。
//!
//! So: the 1 GiB case cannot come from a stock Redis, and raising `proto-max-bulk-len` to
//! manufacture one would be measuring a deployment nobody runs. The synthetic server answers
//! "does the decoder stay incremental past a gigabyte"; the real server answers "does it do
//! that against an actual Redis at its documented limit". Neither substitutes for the other,
//! and merging the two numbers would hide which question was asked.
//!
//! The measured quantity is **peak bytes held**, not wall-clock. §24.3's budget is about
//! memory, and a decoder that is fast because it buffered the whole gigabyte is the failure
//! this test exists to catch.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    // The report prints MiB for a human to read; a f64 rounding a byte count is the point.
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::items_after_statements
)]

use pr_protocol::stream::{Event, ScalarKind, Step, StreamBudget, Streamer};
use resp_server::{Proto, Script, Step as ServerStep, SyntheticServer};
use std::io::Read;
use std::net::TcpStream;

/// One source's result, so the two can be printed side by side without being averaged.
#[derive(Debug)]
struct Measurement {
    source: &'static str,
    declared: u64,
    delivered: u64,
    read_size: usize,
    peak_held: usize,
    peak_rss_delta: Option<i64>,
}

impl Measurement {
    fn report(&self) {
        println!(
            "V-H02 {source}: declared {declared} B, delivered {delivered} B, \
             read size {read} B, peak held {held} B ({held_mib:.2} MiB), RSS delta {rss}",
            source = self.source,
            declared = self.declared,
            delivered = self.delivered,
            read = self.read_size,
            held = self.peak_held,
            held_mib = self.peak_held as f64 / (1024.0 * 1024.0),
            rss = self.peak_rss_delta.map_or_else(
                || "unavailable".to_owned(),
                |d| format!("{:+.2} MiB", d as f64 / (1024.0 * 1024.0))
            ),
        );
    }
}

/// Drain one streamed scalar from `sock`, holding nothing.
fn drain_streamed(mut sock: TcpStream, read_size: usize, source: &'static str) -> Measurement {
    let baseline_rss = pr_core::mem::rss_bytes();
    let mut s = Streamer::new(
        pr_protocol::Budget::default(),
        StreamBudget {
            // Force the streaming path so the measurement is of streaming, not of a
            // threshold that happened to be crossed.
            stream_above: 1,
            ..StreamBudget::default()
        },
    );

    let mut buf = vec![0u8; read_size];
    let mut delivered = 0u64;
    let mut declared = 0u64;
    let mut peak_held = 0usize;
    let mut peak_rss = baseline_rss;
    let mut done = false;

    while !done {
        loop {
            match s.step().unwrap() {
                Step::Incomplete => break,
                Step::Event(Event::ScalarBegin { kind, len }) => {
                    assert_eq!(kind, ScalarKind::Bulk);
                    declared = len.unwrap_or(0) as u64;
                }
                Step::Event(Event::ScalarChunk(c)) => {
                    delivered += c.len() as u64;
                    // Dropped immediately. A consumer would write it to a socket, a file or a
                    // hash here; what it must not do is accumulate, and neither does this.
                    drop(c);
                }
                Step::Event(Event::ScalarEnd) => {}
                Step::Event(Event::FrameEnd) => {
                    done = true;
                    break;
                }
                Step::Event(other) => panic!("unexpected {other:?}"),
            }
            peak_held = peak_held.max(s.buffered());
        }
        if done {
            break;
        }
        let n = sock.read(&mut buf).unwrap();
        assert_ne!(n, 0, "the server closed before the blob finished");
        s.feed(&buf[..n]);
        peak_held = peak_held.max(s.buffered());
        if let (Some(now), Some(prev)) = (pr_core::mem::rss_bytes(), peak_rss) {
            peak_rss = Some(now.max(prev));
        }
    }

    Measurement {
        source,
        declared,
        delivered,
        read_size,
        peak_held,
        peak_rss_delta: match (peak_rss, baseline_rss) {
            (Some(p), Some(b)) => Some(p as i64 - b as i64),
            _ => None,
        },
    }
}

/// The synthetic source: one gigabyte, which no stock Redis will ever send.
#[test]
fn a_one_gibibyte_synthetic_blob_streams_without_being_held() {
    const LEN: u64 = 1024 * 1024 * 1024;
    const SERVER_CHUNK: usize = 256 * 1024;
    const READ: usize = 64 * 1024;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let server = rt.spawn(async move {
        let srv = SyntheticServer::bind(Proto::Resp3).await.unwrap();
        addr_tx.send(srv.addr().unwrap()).unwrap();
        let script = Script::new()
            .then(ServerStep::ReadCommand)
            .then(ServerStep::BlobStream {
                len: LEN,
                chunk: SERVER_CHUNK,
                fill: b'x',
            });
        srv.run_once(&script).await.unwrap();
    });

    let addr = addr_rx.recv().unwrap();
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_nodelay(true).unwrap();
    std::io::Write::write_all(&mut sock, b"*2\r\n$3\r\nGET\r\n$4\r\nhuge\r\n").unwrap();

    let m = drain_streamed(sock, READ, "synthetic RESP server (1 GiB)");
    rt.block_on(server).unwrap();
    m.report();

    assert_eq!(m.declared, LEN, "the server declared a gibibyte");
    assert_eq!(m.delivered, LEN, "every byte was delivered to the consumer");
    // The budget this is measured against: what is held must track the *read* size, not the
    // value size. Two reads of headroom, because a chunk can arrive while one is queued.
    assert!(
        m.peak_held <= 2 * READ,
        "held {} bytes for a 1 GiB value; the read size is {READ}",
        m.peak_held
    );
    // 1 GiB decoded inside a process whose RSS moved by a few MiB at most. The bound is
    // generous on purpose: an allocator is entitled to keep pages, and the claim being made
    // is "it is not proportional to the value", not a precise figure.
    if let Some(d) = m.peak_rss_delta {
        assert!(
            d < 64 * 1024 * 1024,
            "RSS grew by {d} bytes while streaming 1 GiB"
        );
    }
}

/// The real source: a 512 MB string, Redis's documented maximum [R52], on a pinned server.
///
/// `#[ignore]` because it needs Docker and half a gigabyte; run by the `differential` CI job's
/// sibling step. The synthetic test above runs unconditionally, so the incremental property is
/// never untested — this one adds "and against a real server at its real limit".
#[test]
#[ignore = "needs Docker and 512 MB; run by the large-values CI step with --ignored"]
fn a_real_redis_512_mb_string_streams_too() {
    const IMAGE: &str =
        "redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7";
    const LEN: u64 = 512 * 1024 * 1024;
    const READ: usize = 64 * 1024;
    const NAME: &str = "prvh02-redis";

    docker(&["rm", "-f", NAME]);
    let id = docker(&["run", "-d", "--name", NAME, "-P", IMAGE]).expect("start redis");
    assert!(!id.trim().is_empty());
    let _guard = Guard;

    // Wait for it.
    let mut port = None;
    for _ in 0..100 {
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
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let port = port.expect("redis never became ready");

    // SETRANGE at the last byte creates a string of exactly the documented maximum without
    // sending 512 MB over the wire to build it. The server is at its stock configuration:
    // §24.7 forbids raising the limit to manufacture a bigger number.
    let last = LEN - 1;
    let out = docker(&[
        "exec",
        NAME,
        "redis-cli",
        "SETRANGE",
        "vh02:big",
        &last.to_string(),
        "x",
    ])
    .expect("SETRANGE");
    assert_eq!(
        out.trim(),
        LEN.to_string(),
        "the string is not 512 MB: {out}"
    );

    let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.set_nodelay(true).unwrap();
    std::io::Write::write_all(&mut sock, b"*2\r\n$3\r\nGET\r\n$8\r\nvh02:big\r\n").unwrap();

    let m = drain_streamed(sock, READ, "real Redis 7.4.11 (512 MB, stock limit)");
    m.report();

    assert_eq!(m.declared, LEN);
    assert_eq!(m.delivered, LEN);
    assert!(
        m.peak_held <= 2 * READ,
        "held {} bytes for a 512 MB value; the read size is {READ}",
        m.peak_held
    );

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            docker(&["rm", "-f", NAME]);
        }
    }
}

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
