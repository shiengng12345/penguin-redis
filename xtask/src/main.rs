//! Phase 0 build/measurement tasks (v2.1 §31.3 xtask).
//!
//! ```text
//! xtask measure-baseline   # V-A06/V-H01: startup, idle heap, RSS baseline
//! xtask budgets            # print the budget table from v2.1
//! ```

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
