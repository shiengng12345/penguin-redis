//! V-G05 worker: one process that appends a line to a shared file under the protocol.
//!
//! A separate **process**, not a thread, because an advisory lock is a promise between
//! processes. Threads in one process share the lock on most platforms and would pass a test
//! that a second process fails.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::unwrap_used,
    missing_docs
)]

use pr_profiles::shared::SharedFile;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: shared_writer <path> <tag> [rounds]");
        std::process::exit(2);
    };
    let Some(tag) = args.next() else {
        eprintln!("usage: shared_writer <path> <tag> [rounds]");
        std::process::exit(2);
    };
    let rounds: usize = args.next().unwrap_or_else(|| "20".into()).parse().unwrap();

    let file = SharedFile::new(&path);
    let mut done = 0usize;
    for i in 0..rounds {
        // Retry on a conflict: the point of the test is that no update is *lost*, not that no
        // update is ever refused. A refusal that preserves the edit is the protocol working.
        for _ in 0..200 {
            match file.update(|body| format!("{body}{tag}-{i}\n")) {
                Ok(_) => {
                    done += 1;
                    break;
                }
                Err(e) => {
                    eprintln!("{tag}: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        }
    }
    println!("{tag} wrote {done}");
}
