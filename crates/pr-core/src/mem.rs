//! Process memory measurement (v2.1 §24.6, V-A06, V-H01).
//!
//! §24.6 states RSS budgets — 30 MiB for an idle REPL, 60 MiB for an idle TUI — and a budget
//! nobody can measure on the platform the user runs is not a budget. So all three supported
//! platforms are covered here, each by the route that needs no extra dependency and no
//! `unsafe`:
//!
//! | platform | source |
//! |---|---|
//! | Linux | `/proc/self/statm`, field 2, in pages |
//! | macOS | `ps -o rss=`, in kilobytes |
//! | Windows | `tasklist /FO CSV`, "Mem Usage" in kilobytes |
//!
//! When the source is unavailable the answer is `None` rather than a guess. A budget check
//! that silently passes on a fabricated number is worse than one that fails.

/// Resident set size of this process, in bytes.
#[must_use]
pub fn rss_bytes() -> Option<u64> {
    rss_of(std::process::id())
}

/// Resident set size of an arbitrary process, in bytes.
///
/// Measuring a *child* is what the startup budget needs: a probe that measures itself pays
/// for the measuring harness too.
#[must_use]
pub fn rss_of(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let path = if pid == std::process::id() {
            "/proc/self/statm".to_owned()
        } else {
            format!("/proc/{pid}/statm")
        };
        let s = std::fs::read_to_string(path).ok()?;
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        // `sysconf(_SC_PAGESIZE)` is 4096 on every supported Linux target.
        Some(pages * 4096)
    }
    #[cfg(target_os = "macos")]
    {
        // `ps` is the dependency-free route; it reports RSS in kilobytes.
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p"])
            .arg(pid.to_string())
            .output()
            .ok()?;
        let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(windows)]
    {
        // `tasklist` in CSV form: the last column is "Mem Usage" as `"12,345 K"`.
        let out = std::process::Command::new("tasklist")
            .args(["/NH", "/FO", "CSV", "/FI"])
            .arg(format!("PID eq {pid}"))
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let last = text.lines().find(|l| l.contains(&pid.to_string()))?;
        let field = last.rsplit(',').next()?.trim().trim_matches('"');
        let digits: String = field.chars().filter(char::is_ascii_digit).collect();
        let kb: u64 = digits.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = pid;
        None
    }
}

/// Whether this platform can report RSS at all.
///
/// A test asserts this is true on every supported target, so a platform quietly losing its
/// measurement route is a failing test rather than a budget that stops being checked.
#[must_use]
pub fn rss_available() -> bool {
    cfg!(any(target_os = "linux", target_os = "macos", windows))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_platform_can_report_rss() {
        assert!(
            rss_available(),
            "a budget nobody can measure here is not a budget"
        );
        let r = rss_bytes().expect("this platform reports RSS");
        assert!(r > 512 * 1024, "implausibly small: {r}");
        assert!(r < 4 * 1024 * 1024 * 1024, "implausibly large: {r}");
    }

    #[test]
    fn measuring_a_process_that_does_not_exist_returns_none() {
        // A missing process must not be reported as zero bytes, which would look like a
        // process that passed every budget.
        assert_eq!(rss_of(0xffff_fffe), None);
    }

    #[test]
    fn allocating_moves_the_number() {
        let before = rss_bytes().unwrap();
        let big: Vec<u8> = vec![7u8; 48 * 1024 * 1024];
        // Touch it so the pages are really resident rather than merely reserved.
        let sum: u64 = big.iter().step_by(4096).copied().map(u64::from).sum();
        assert!(sum > 0);
        let after = rss_bytes().unwrap();
        assert!(
            after > before,
            "48 MiB of touched pages did not show up: {before} -> {after}"
        );
        drop(big);
    }
}
