//! Phase 0 build/measurement tasks (v2.1 §31.3 xtask).
//!
//! ```text
//! xtask measure-baseline   # V-A06/V-H01: startup, idle heap, RSS baseline
//! xtask budgets            # print the budget table from v2.1
//! xtask catalog-stamp <f>  # V-F01: fingerprint a pinned catalog snapshot
//! xtask catalog-verify <f> # V-F01: check a snapshot against its fingerprint
//! ```

pub mod catalog;
pub mod fault;
pub mod measure;

use measure::{Budget, CountingAllocator, Sample, budgets, rss_bytes};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

fn print_budgets() {
    let all: [Budget; 5] = [
        budgets::ASSISTANCE_HEAP,
        budgets::RESULT_STORE,
        budgets::SINGLE_RESULT,
        budgets::IDLE_REPL_RSS,
        budgets::IDLE_TUI_RSS,
    ];
    println!("{:<28} {:>12}", "budget", "bytes");
    for b in all {
        println!("{:<28} {:>12}", b.name, b.max_heap_bytes);
    }
}

fn measure_baseline() {
    let start = std::time::Instant::now();
    let s = Sample::take(&ALLOC);
    let elapsed = start.elapsed();
    println!("# V-A06 baseline");
    println!("heap_live_bytes  {}", s.heap_live);
    println!("heap_peak_bytes  {}", s.heap_peak);
    println!("alloc_calls      {}", s.allocs);
    match s.rss {
        Some(r) => println!("rss_bytes        {r}"),
        None => println!("rss_bytes        unavailable-on-this-platform"),
    }
    println!("sample_ns        {}", elapsed.as_nanos());
    println!("host_os          {}", std::env::consts::OS);
    println!("host_arch        {}", std::env::consts::ARCH);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("measure-baseline") => {
            measure_baseline();
            0
        }
        Some("budgets") => {
            print_budgets();
            0
        }
        Some(verb @ ("catalog-stamp" | "catalog-verify")) => {
            let files = &args[2..];
            if files.is_empty() {
                eprintln!("{verb}: expected at least one snapshot path");
                std::process::exit(2);
            }
            let mut bad = 0;
            for f in files {
                let path = std::path::Path::new(f);
                let r = if verb == "catalog-stamp" {
                    catalog::stamp(path)
                } else {
                    catalog::verify(path)
                };
                match r {
                    Ok(msg) => println!("{f}: {msg}"),
                    Err(e) => {
                        eprintln!("{e}");
                        bad += 1;
                    }
                }
            }
            i32::from(bad > 0)
        }
        Some("rss") => {
            match rss_bytes() {
                Some(v) => println!("{v}"),
                None => println!("unavailable"),
            }
            0
        }
        _ => {
            eprintln!("usage: xtask <measure-baseline | budgets | rss>");
            2
        }
    };
    std::process::exit(code);
}
