//! CLI for the synthetic RESP server.
//!
//! ```text
//! resp-server gen-fixtures <dir>   # write hostile corpus to <dir>/<category>/<name>.resp + manifest.json
//! resp-server stats                # print category counts
//! ```

use resp_server::hostile::{Expectation, corpus};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn expectation_str(e: Expectation) -> &'static str {
    match e {
        Expectation::ProtocolError => "protocol-error",
        Expectation::Valid => "valid",
        Expectation::BudgetExceeded => "budget-exceeded",
        Expectation::Incomplete => "incomplete",
    }
}

fn gen_fixtures(dir: &Path) -> std::io::Result<()> {
    let mut manifest: Vec<serde_json::Value> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for s in corpus() {
        let cat_dir = dir.join(s.category);
        fs::create_dir_all(&cat_dir)?;
        let file = cat_dir.join(format!("{}.resp", s.name));
        fs::write(&file, &s.bytes)?;
        *counts.entry(s.category).or_default() += 1;
        manifest.push(serde_json::json!({
            "category": s.category,
            "name": s.name,
            "file": format!("{}/{}.resp", s.category, s.name),
            "bytes": s.bytes.len(),
            "expectation": expectation_str(s.expectation),
        }));
    }
    let m = serde_json::json!({
        "schema": "penguin.fixtures.protocol.v1",
        "source": "xtask/resp-server hostile::corpus()",
        "categories": counts,
        "samples": manifest,
    });
    fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&m)?)?;
    for (c, n) in &counts {
        println!("{c:<20} {n}");
    }
    println!("total {}", manifest.len());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("gen-fixtures") => {
            if let Some(d) = args.get(2) {
                match gen_fixtures(Path::new(d)) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("gen-fixtures failed: {e}");
                        1
                    }
                }
            } else {
                eprintln!("usage: resp-server gen-fixtures <dir>");
                2
            }
        }
        Some("stats") => {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for s in corpus() {
                *counts.entry(s.category).or_default() += 1;
            }
            for (c, n) in &counts {
                println!("{c:<20} {n}");
            }
            0
        }
        Some("categories") => {
            for c in resp_server::hostile::categories() {
                println!("{c}");
            }
            0
        }
        _ => {
            eprintln!("usage: resp-server <gen-fixtures <dir> | stats | categories>");
            2
        }
    };
    std::process::exit(code);
}
