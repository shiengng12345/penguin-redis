//! Penguin Redis executable `prc` (v2.1 §31.3, §24.6).
//!
//! Phase 0 scope: the argument contract, the fast startup paths, and the idle probes V-H01
//! measures. The whole crate graph is linked on purpose — the budget question is whether the
//! dependencies have eaten it before any feature code exists, and a binary that links half of
//! them cannot answer that.

mod args;
mod binout;
mod passthrough;
mod startup;

use pr_core::ExitCode;
use startup::{Fast, Probe};

/// The environment variable that selects an idle probe.
///
/// An environment variable rather than a flag: it is a measurement hook, not part of the
/// command-line contract, and it must not appear in `--help` as though users should run it.
const PROBE_ENV: &str = "PR_PHASE0_PROBE";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // WIN-02: write the byte canary and nothing else, so a test can compare what came out of
    // the pipe with what went in. Checked before the argument contract for the same reason the
    // other probes are: a measurement must not depend on the thing being measured.
    if std::env::var(PROBE_ENV).as_deref() == Ok("binary-out") {
        match binout::write_binary(&binout::canary()) {
            Ok(()) => std::process::exit(ExitCode::Success as i32),
            Err(e) => {
                eprintln!("prc: {e}");
                std::process::exit(ExitCode::LocalFailure as i32);
            }
        }
    }

    // Measurement first, so a probe run never depends on the argument contract.
    if let Ok(name) = std::env::var(PROBE_ENV) {
        let Some(probe) = Probe::parse(&name) else {
            eprintln!("prc: unknown {PROBE_ENV} value {name:?}");
            std::process::exit(ExitCode::Usage as i32);
        };
        match startup::run_probe(probe) {
            Ok(line) => {
                print!("{line}");
                std::process::exit(ExitCode::Success as i32);
            }
            Err(e) => {
                eprintln!("prc: probe {name} failed: {e}");
                std::process::exit(ExitCode::LocalFailure as i32);
            }
        }
    }

    // §29.4: `--mcp-stdio` is a **separate** bootstrap path, and separate means it branches
    // here — before the argument contract, before the catalog, before anything that might
    // print. A path that shares the ordinary start-up and then suppresses output is one stray
    // `println!` away from breaking every MCP host that talks to it, with a JSON parse error
    // that points nowhere near the cause.
    if argv.iter().any(|a| a == "--mcp-stdio") {
        let stdin = std::io::stdin();
        let code = match pr_mcp::stdio::serve(
            stdin.lock(),
            std::io::stdout().lock(),
            std::io::stderr().lock(),
            &pr_mcp::stdio::EmptyBackend,
        ) {
            Ok(()) => ExitCode::Success,
            Err(e) => {
                eprintln!("prc: mcp-stdio: {e}");
                ExitCode::LocalFailure
            }
        };
        std::process::exit(code as i32);
    }

    // `--help` must reach the terminal without loading the catalog, opening a database,
    // claiming the terminal or starting a runtime. V-H01 asserts exactly that.
    match startup::fast_path(&argv) {
        Fast::Help => {
            print!("{}", startup::help());
            std::process::exit(ExitCode::Success as i32);
        }
        Fast::Version => {
            print!("{}", startup::version());
            std::process::exit(ExitCode::Success as i32);
        }
        Fast::ProbeWidth => {
            let (text, agreed) = startup::probe_width();
            print!("{text}");
            std::process::exit(if agreed {
                ExitCode::Success as i32
            } else {
                // A disagreement is information, not a failure of the command; only an
                // unusable terminal is an error.
                ExitCode::Success as i32
            });
        }
        Fast::None => {}
    }

    let code = match args::parse(&argv) {
        // V-A02 scaffolding: a direct target plus a command is executed as a passthrough, so
        // the differential harness has something to compare against `redis-cli`. Everything
        // else still stops at the argument contract — Phase 0 owns the contract, not the
        // kernel. See `pr_transport::oneshot` for why this must not grow.
        Ok(inv) => match &inv.target {
            args::Target::Direct { host, port, url } if !inv.command.is_empty() => {
                if url.is_some() {
                    eprintln!("prc: -u is not implemented in the Phase 0 passthrough");
                    ExitCode::Usage
                } else {
                    let host = host.clone().unwrap_or_else(|| "127.0.0.1".to_owned());
                    passthrough::run(&inv, &host, port.unwrap_or(6379))
                }
            }
            _ => ExitCode::Success,
        },
        Err(e) => {
            eprintln!("prc: {e}");
            ExitCode::Usage
        }
    };
    std::process::exit(code as i32);
}
