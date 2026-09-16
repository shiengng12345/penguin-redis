//! Penguin Redis executable `prc` (v2.1 §31.3, §24.6).
//!
//! Phase 0 scope: the argument contract, the fast startup paths, and the idle probes V-H01
//! measures. The whole crate graph is linked on purpose — the budget question is whether the
//! dependencies have eaten it before any feature code exists, and a binary that links half of
//! them cannot answer that.

mod args;
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
        Fast::None => {}
    }

    let code = match args::parse(&argv) {
        Ok(_inv) => ExitCode::Success,
        Err(e) => {
            eprintln!("prc: {e}");
            ExitCode::Usage
        }
    };
    std::process::exit(code as i32);
}
