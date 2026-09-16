//! V-A02 runner: start two servers, run five cases through both clients, write the report.
//!
//! Usage: `cargo run -p differential -- <out-dir>` (default `tests/differential/report`).
//!
//! Exit 0 only if every case passed every layer.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use bytes::Bytes;
use differential::cases::Case;
use differential::compare::{Side, compare, escape};
use differential::docker::{DockerError, Server};
use differential::proxy::RecordingProxy;
use differential::report::{CaseReport, Observed, Report};
use pr_transport::Oneshot;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The server both sides run, pinned by digest (`compatibility/manifest.toml`, `baseline_cli`).
///
/// The same image supplies the baseline `redis-cli`, so the comparison is against the version
/// the manifest actually pins rather than whatever is on the developer's PATH.
const IMAGE: &str = "redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7";
const BASELINE_CLI: &str = "redis-cli 7.4.11";
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> std::process::ExitCode {
    let out_dir = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("tests/differential/report"), PathBuf::from);
    match run(&out_dir) {
        Ok(report) => {
            let ok = report.ok();
            println!(
                "{} case(s), {}",
                report.cases.len(),
                if ok { "all layers agree" } else { "FAILURES" }
            );
            if ok {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("differential: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(out_dir: &Path) -> Result<Report, String> {
    // Two servers. Not one used twice -- see the module note in lib.rs.
    let baseline_server =
        Server::start("prdiff-baseline", IMAGE).map_err(|e: DockerError| e.to_string())?;
    let penguin_server =
        Server::start("prdiff-penguin", IMAGE).map_err(|e: DockerError| e.to_string())?;

    let prc = prc_binary()?;
    let mut cases_out = Vec::new();

    for case in differential::cases() {
        // Identical initial state, applied directly to each server so the seed traffic does
        // not land in the recording.
        for server in [&baseline_server, &penguin_server] {
            seed(server, &case)?;
        }

        let baseline = run_baseline(&baseline_server, &case)?;
        let penguin = run_penguin(&penguin_server, &prc, &case)?;

        let layers = compare(&baseline, &penguin, case.output);
        cases_out.push(CaseReport {
            case_id: case.id.to_owned(),
            baseline_cli: BASELINE_CLI.to_owned(),
            server_image_digest: IMAGE.to_owned(),
            protocol: "RESP2".to_owned(),
            initial_state_fixture: case.initial_state_fixture.to_owned(),
            request: case.request.iter().map(|s| (*s).to_owned()).collect(),
            expected_reply: case.expected_reply.to_owned(),
            criterion: case.criterion.to_owned(),
            execution_status: if layers.ok() { "PASS" } else { "FAIL" }.to_owned(),
            layers,
            baseline_observed: observed(&baseline),
            penguin_observed: observed(&penguin),
        });
    }

    let report = Report {
        schema: "penguin.differential.v1".to_owned(),
        generated: "2026-09-16".to_owned(),
        servers: vec![
            baseline_server.container().to_owned(),
            penguin_server.container().to_owned(),
        ],
        cases: cases_out,
    };

    write_report(out_dir, &report)?;
    Ok(report)
}

/// Apply the case's fixture to one server, directly.
fn seed(server: &Server, case: &Case) -> Result<(), String> {
    let mut conn =
        Oneshot::connect("127.0.0.1", server.port(), TIMEOUT).map_err(|e| e.to_string())?;
    for argv in case.seed {
        let cmd: Vec<Bytes> = argv
            .iter()
            .map(|a| Bytes::from(a.as_bytes().to_vec()))
            .collect();
        conn.call(&cmd).map_err(|e| format!("seed {argv:?}: {e}"))?;
    }
    Ok(())
}

/// Read the case's state probes back from one server, with the harness's own client.
fn read_state(server: &Server, case: &Case) -> Result<Vec<Vec<u8>>, String> {
    let mut conn =
        Oneshot::connect("127.0.0.1", server.port(), TIMEOUT).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for probe in case.state_probes {
        let cmd: Vec<Bytes> = probe
            .argv
            .iter()
            .map(|a| Bytes::from(a.as_bytes().to_vec()))
            .collect();
        let reply = conn
            .call(&cmd)
            .map_err(|e| format!("probe {:?}: {e}", probe.argv))?;
        out.push(format!("{reply:?}").into_bytes());
    }
    Ok(out)
}

fn run_baseline(server: &Server, case: &Case) -> Result<Side, String> {
    let proxy = RecordingProxy::start(server.address()).map_err(|e| e.to_string())?;
    // redis-cli runs inside a container of the pinned image, and reaches the host-side
    // recording proxy through the gateway alias. Running the binary from the image rather
    // than from PATH is what makes "pinned baseline" true rather than aspirational.
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--add-host=host.docker.internal:host-gateway".into(),
        IMAGE.into(),
        "redis-cli".into(),
        "-h".into(),
        "host.docker.internal".into(),
        "-p".into(),
        proxy.port().to_string(),
    ];
    args.extend(case.request.iter().map(|s| (*s).to_owned()));

    let out = Command::new("docker")
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;

    let recordings = proxy.finish();
    Ok(Side {
        sent: recordings
            .iter()
            .flat_map(|r| r.to_server.clone())
            .collect(),
        received: recordings
            .iter()
            .flat_map(|r| r.to_client.clone())
            .collect(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit: out.status.code().unwrap_or(-1),
        final_state: read_state(server, case)?,
    })
}

fn run_penguin(server: &Server, prc: &Path, case: &Case) -> Result<Side, String> {
    let proxy = RecordingProxy::start(server.address()).map_err(|e| e.to_string())?;
    let mut args: Vec<String> = vec![
        "-h".into(),
        "127.0.0.1".into(),
        "-p".into(),
        proxy.port().to_string(),
    ];
    args.extend(case.request.iter().map(|s| (*s).to_owned()));

    let out = Command::new(prc)
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;

    let recordings = proxy.finish();
    Ok(Side {
        sent: recordings
            .iter()
            .flat_map(|r| r.to_server.clone())
            .collect(),
        received: recordings
            .iter()
            .flat_map(|r| r.to_client.clone())
            .collect(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit: out.status.code().unwrap_or(-1),
        final_state: read_state(server, case)?,
    })
}

fn observed(s: &Side) -> Observed {
    Observed {
        sent: differential::compare::render_commands(&s.sent),
        received: escape(&s.received),
        stdout: escape(&s.stdout),
        stderr: escape(&s.stderr),
        exit: s.exit,
        final_state: s.final_state.iter().map(|r| escape(r)).collect(),
    }
}

/// Where `prc` is. Built by the caller; looked up rather than rebuilt so the harness measures
/// the binary CI produced.
fn prc_binary() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("PRC_BINARY") {
        return Ok(PathBuf::from(p));
    }
    let root = repo_root();
    for profile in ["debug", "release"] {
        let p = root.join("target").join(profile).join("prc");
        if p.exists() {
            return Ok(p);
        }
    }
    Err("prc is not built; run `cargo build -p prc` first or set PRC_BINARY".to_owned())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn write_report(dir: &Path, report: &Report) -> Result<(), String> {
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        repo_root().join(dir)
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    for case in &report.cases {
        let path = dir.join(format!("{}.json", case.case_id));
        let json = serde_json::to_string_pretty(case).map_err(|e| e.to_string())?;
        std::fs::write(&path, json + "\n").map_err(|e| e.to_string())?;
    }
    let summary = summarise(report);
    std::fs::write(dir.join("README.md"), summary).map_err(|e| e.to_string())?;
    Ok(())
}

fn summarise(report: &Report) -> String {
    use std::fmt::Write as _;
    let mut s = String::from(
        "# Differential 比对报告（V-A02，v2.1 §32.2、附录 B.2）\n\n\
         由 `cargo run -p differential` 生成，逐 case 一个 JSON。**不要手改**——\
         `ci/check-differential.sh` 会重跑并逐字节比对。\n\n\
         ## 为什么是两台服务器\n\n\
         §32.2 写得很直白：同一个写命令，官方与 Penguin 必须用两个独立但相同初态的实例。\
         在同一实例上先后跑两次 `INCR` 再比结果，看起来像差异测试，其实测的是执行顺序。\n\n\
         ## 四层\n\n\
         | 层 | 比什么 | 怎么取到的 |\n\
         |---|---|---|\n\
         | 1 | argv 字节 | 录制代理记下客户端→服务器的原始字节，再解成命令序列 |\n\
         | 2 | 响应字节 | 同一个代理记下服务器→客户端的原始字节，逐字节比 |\n\
         | 3 | 最终状态 | 事后用**同一个** reader 从两台服务器读回来——这一层测的是服务器，不是客户端 |\n\
         | 4 | 输出与 exit code | 两个进程的 stdout / stderr / 退出码 |\n\n\
         1～3 层必须完全一致。第 4 层允许不同，但**差异必须在 case 里事先写明**，\
         否则明天冒出来的新差异只会被耸肩放过。\n\n\
         ## 结果\n\n",
    );
    s.push_str(
        "| case | 场景 | 通过标准 | 层1 | 层2 | 层3 | 层4 |\n|---|---|---|---|---|---|---|\n",
    );
    for c in &report.cases {
        let _ = writeln!(
            s,
            "| {} | `{}` | {} | {} | {} | {} | {} |",
            c.case_id,
            c.request.join(" "),
            c.criterion,
            mark(&c.layers.argv_bytes),
            mark(&c.layers.reply_bytes),
            mark(&c.layers.final_state),
            mark(&c.layers.output_and_exit),
        );
    }
    let _ = write!(
        s,
        "\n服务器镜像：`{IMAGE}`；baseline：`{BASELINE_CLI}`（同一镜像内的 `redis-cli`，\
         不是开发机 PATH 上的那个）。\n"
    );
    s
}

fn mark(v: &differential::compare::LayerVerdict) -> &'static str {
    use differential::compare::LayerVerdict as L;
    match v {
        L::Same => "一致",
        L::ExpectedDifference { .. } => "已登记差异",
        L::UnexpectedDifference { .. } => "**未登记差异**",
    }
}
