//! V-F08 diagnostics: what concepts does a phrase produce, and why did a command not win?
//!
//! `cargo run -p pr-intelligence --example find_debug -- "some phrase"`
//! `cargo run -p pr-intelligence --example find_debug -- --misses pairs.tsv`

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs
)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let f = pr_intelligence::find::Finder::new();
    if args.len() >= 2 && args[0] == "--misses" {
        let text = std::fs::read_to_string(&args[1]).expect("pairs file");
        for line in text.lines() {
            let Some((cmd, phrase)) = line.split_once('\t') else {
                continue;
            };
            let cs = f.concepts_of(phrase);
            let want: Vec<&str> = pr_intelligence::vocabulary::PURPOSES
                .iter()
                .find(|p| p.command == cmd)
                .map(|p| p.concepts.to_vec())
                .unwrap_or_default();
            let hit: Vec<&str> = want.iter().copied().filter(|c| cs.contains(c)).collect();
            let cold: Vec<&str> = want.iter().copied().filter(|c| !cs.contains(c)).collect();
            println!("{cmd}\t{phrase}");
            println!("  query: {cs:?}");
            println!("  hit:   {hit:?}");
            println!("  cold:  {cold:?}");
        }
        return;
    }
    let phrase = args.first().cloned().unwrap_or_default();
    println!("concepts: {:?}", f.concepts_of(&phrase));
    for h in f.search(&phrase, 8) {
        println!("  {:>8.4}  {:<24} {:?}", h.score, h.command, h.matched);
    }
}
