//! Penguin Redis executable `prc` (v2.1 §31.3). Phase 0: argument contract only.

mod args;

use pr_core::ExitCode;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match args::parse(&argv) {
        Ok(_inv) => ExitCode::Success,
        Err(e) => {
            eprintln!("prc: {e}");
            ExitCode::Usage
        }
    };
    std::process::exit(code as i32);
}
