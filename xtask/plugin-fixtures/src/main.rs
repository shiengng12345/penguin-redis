//! V-D09 fixtures: an L2 renderer plugin that misbehaves on purpose (v2.1 §30.4, R45).
//!
//! §30.4 promises that a plugin's crash, OOM or infinite loop only affects the plugin. A
//! promise like that cannot be tested with a plugin that behaves, so this one is built to
//! fail in each of the ways a real one eventually will — and in the ways a hostile one would
//! choose.
//!
//! The mode comes from argv so the host spawns the same executable each time and the test
//! reads as a table.

// This crate exists to misbehave. `panic` is the behaviour under test, not a lapse.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::exit,
    clippy::panic,
    missing_docs
)]

use std::io::{Read, Write};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "ok".to_owned());

    // Every mode reads its input first, because a plugin that never reads stdin makes the
    // host's write block once the pipe buffer fills — which is its own failure mode and would
    // otherwise mask the one under test.
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);

    match mode.as_str() {
        // The control. Without it, a host that failed everything would pass every other case.
        "ok" => {
            println!(
                "{{\"lines\":[\"rendered by the plugin\",\"input was {} bytes\"]}}",
                input.len()
            );
        }
        // A panic: the ordinary way real code fails.
        "panic" => panic!("this plugin panics on purpose (V-D09)"),
        // `abort` skips unwinding entirely — SIGABRT on Unix, a fast-fail on Windows. A host
        // that only handles panics would miss this.
        "abort" => std::process::abort(),
        // An infinite loop that produces nothing: the host must kill it rather than wait.
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_hours(1));
        },
        // An infinite loop that produces *a lot*: the host must truncate and kill without
        // growing to match. A host that reads to end of stream dies here, not the plugin.
        "flood" => {
            let mut out = std::io::stdout().lock();
            let chunk = vec![b'x'; 64 * 1024];
            loop {
                if out.write_all(&chunk).is_err() {
                    // The host closed the pipe, which is the host doing its job.
                    return;
                }
            }
        }
        // Allocates until it cannot. The OS kills this process; §30.4 says the host lives.
        "oom" => {
            let mut held: Vec<Vec<u8>> = Vec::new();
            loop {
                held.push(vec![0u8; 64 * 1024 * 1024]);
                // Touch the pages so they are real rather than promised.
                if let Some(last) = held.last_mut() {
                    for p in last.iter_mut().step_by(4096) {
                        *p = 1;
                    }
                }
            }
        }
        // Valid JSON, wrong shape.
        "wrong-shape" => println!("{{\"unexpected\":true}}"),
        // Not JSON at all.
        "garbage" => println!("this is not json"),
        // Produces output, then exits non-zero. The output must not be used: a plugin that
        // says it failed has said so.
        "output-then-fail" => {
            println!("{{\"lines\":[\"looks fine\"]}}");
            std::process::exit(3);
        }
        // Emits **raw** control bytes inside a JSON string. That is not valid JSON, and the
        // host must say so rather than repair it: a plugin that can smuggle a raw ESC past
        // the parser has a channel to the terminal that nothing inspected.
        "escapes-raw" => {
            let esc = "\u{1b}";
            let bel = "\u{7}";
            println!("{{\"lines\":[\"{esc}]0;pwned{bel}before{esc}[2Jafter\"]}}");
        }
        // Emits the same escapes *correctly encoded*, so the JSON is valid and the ESC bytes
        // are real once decoded. This is the case §23.6 is actually about: well-formed output
        // whose content is hostile.
        "escapes-json" => {
            println!("{{\"lines\":[\"\\u001b]0;pwned\\u0007before\\u001b[2Jafter\"]}}");
        }
        // Reports the environment it was given, so the host's scrubbing can be checked from
        // outside rather than by reading the host's own code.
        "echo-env" => {
            let names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
            let joined = names.join(" ");
            println!("{{\"lines\":[\"{joined}\"]}}");
        }
        // Slow but finite: finishes well inside a generous timeout, so the timeout test is
        // measuring a timeout rather than a slow machine.
        "slow" => {
            std::thread::sleep(std::time::Duration::from_millis(300));
            println!("{{\"lines\":[\"eventually\"]}}");
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    }
}
