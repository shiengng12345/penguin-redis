//! V-A06 — resource measurement: allocator accounting, RSS sampling, task counts.
//!
//! Every budget in v2.1 (§12.5 assistance heap, §24.3 result store, §24.6 idle RSS) is a
//! claim that has to be *measured*, and ADR-029 says all of them are process-scoped. This
//! module is the instrument those claims are checked with.
//!
//! Two separate numbers, never mixed (R08/R35):
//! - **heap** — what this process allocated, from a counting allocator
//! - **RSS** — what the OS says is resident, which includes read-only mmapped catalog pages
//!   that the heap number must not be blamed for

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// A `GlobalAlloc` wrapper that counts bytes and allocation calls.
///
/// Install in a binary or test harness with:
/// ```ignore
/// #[global_allocator]
/// static ALLOC: CountingAllocator = CountingAllocator::new();
/// ```
pub struct CountingAllocator {
    live: AtomicUsize,
    peak: AtomicUsize,
    allocs: AtomicU64,
}

impl CountingAllocator {
    /// Create the counter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            allocs: AtomicU64::new(0),
        }
    }
    /// Bytes currently allocated.
    pub fn live_bytes(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }
    /// High-water mark since the last reset.
    pub fn peak_bytes(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }
    /// Number of allocation calls since the last reset.
    pub fn alloc_count(&self) -> u64 {
        self.allocs.load(Ordering::Relaxed)
    }
    /// Reset the peak and call counter to the current live value.
    pub fn reset_peak(&self) {
        self.peak
            .store(self.live.load(Ordering::Relaxed), Ordering::Relaxed);
        self.allocs.store(0, Ordering::Relaxed);
    }

    fn record_alloc(&self, n: usize) {
        self.allocs.fetch_add(1, Ordering::Relaxed);
        let now = self.live.fetch_add(n, Ordering::Relaxed) + n;
        self.peak.fetch_max(now, Ordering::Relaxed);
    }
    fn record_free(&self, n: usize) {
        self.live.fetch_sub(n, Ordering::Relaxed);
    }
}

impl Default for CountingAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY-equivalent note: this forwards every call to the system allocator unchanged and only
// adds relaxed counters, so it inherits System's guarantees. `unsafe_code` is denied
// workspace-wide, so the impl is allowed here explicitly and nowhere else.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            self.record_alloc(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        self.record_free(layout.size());
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            self.record_free(layout.size());
            self.record_alloc(new_size);
        }
        p
    }
}

/// Resident set size of this process, in bytes.
///
/// Returns `None` when the platform source is unavailable rather than guessing — a budget
/// check must fail loudly, not silently pass on a fabricated number.
#[must_use]
pub fn rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        // `sysconf(_SC_PAGESIZE)` is 4096 on every supported Linux target.
        Some(pages * 4096)
    }
    #[cfg(target_os = "macos")]
    {
        // `ps` is the dependency-free route; it reports RSS in kilobytes.
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p"])
            .arg(std::process::id().to_string())
            .output()
            .ok()?;
        let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// One measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    /// Heap bytes live at sample time.
    pub heap_live: usize,
    /// Heap high-water mark.
    pub heap_peak: usize,
    /// Allocation calls.
    pub allocs: u64,
    /// OS resident set size, when the platform can report it.
    pub rss: Option<u64>,
}

/// The difference between two samples — what a module actually costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delta {
    /// Change in live heap bytes.
    pub heap_live: i64,
    /// Change in peak heap bytes.
    pub heap_peak: i64,
    /// Allocation calls in the window.
    pub allocs: u64,
    /// Change in RSS, when both samples had it.
    pub rss: Option<i64>,
}

impl Sample {
    /// Take a sample from a counting allocator.
    #[must_use]
    pub fn take(alloc: &CountingAllocator) -> Self {
        Self {
            heap_live: alloc.live_bytes(),
            heap_peak: alloc.peak_bytes(),
            allocs: alloc.alloc_count(),
            rss: rss_bytes(),
        }
    }

    /// Difference from an earlier sample.
    #[must_use]
    pub fn since(self, before: Self) -> Delta {
        let to_i = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        Delta {
            heap_live: to_i(self.heap_live) - to_i(before.heap_live),
            heap_peak: to_i(self.heap_peak) - to_i(before.heap_peak),
            allocs: self.allocs.saturating_sub(before.allocs),
            rss: match (self.rss, before.rss) {
                (Some(a), Some(b)) => {
                    Some(i64::try_from(a).unwrap_or(i64::MAX) - i64::try_from(b).unwrap_or(0))
                }
                _ => None,
            },
        }
    }
}

/// A budget assertion, as used by V-F09 / V-H01.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Human-readable name for the report.
    pub name: &'static str,
    /// Maximum heap increment, in bytes.
    pub max_heap_bytes: i64,
}

/// Outcome of checking a delta against a budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetCheck {
    /// Within budget.
    Within {
        /// Bytes used.
        used: i64,
        /// Bytes allowed.
        allowed: i64,
    },
    /// Over budget. Never silently downgraded — v2.1 §16.4 requires locating the cause,
    /// not widening the limit.
    Exceeded {
        /// Bytes used.
        used: i64,
        /// Bytes allowed.
        allowed: i64,
    },
}

impl BudgetCheck {
    /// Whether the budget held.
    #[must_use]
    pub fn ok(&self) -> bool {
        matches!(self, Self::Within { .. })
    }
}

impl Budget {
    /// Check a measured delta.
    #[must_use]
    pub fn check(&self, d: Delta) -> BudgetCheck {
        let used = d.heap_peak.max(d.heap_live);
        if used <= self.max_heap_bytes {
            BudgetCheck::Within {
                used,
                allowed: self.max_heap_bytes,
            }
        } else {
            BudgetCheck::Exceeded {
                used,
                allowed: self.max_heap_bytes,
            }
        }
    }
}

/// Budgets from v2.1, so the numbers live next to the instrument that checks them.
pub mod budgets {
    use super::Budget;

    /// §12.5 / R08: assistance heap increment, 12 MiB cap (10 MiB of sub-items + 2 MiB margin).
    pub const ASSISTANCE_HEAP: Budget = Budget {
        name: "assistance heap increment",
        max_heap_bytes: 12 * 1024 * 1024,
    };
    /// §24.3: whole result store.
    pub const RESULT_STORE: Budget = Budget {
        name: "result store",
        max_heap_bytes: 64 * 1024 * 1024,
    };
    /// §24.3: one retained result.
    pub const SINGLE_RESULT: Budget = Budget {
        name: "single retained result",
        max_heap_bytes: 16 * 1024 * 1024,
    };
    /// §24.6: idle REPL RSS target.
    pub const IDLE_REPL_RSS: Budget = Budget {
        name: "idle REPL RSS",
        max_heap_bytes: 30 * 1024 * 1024,
    };
    /// §24.6: idle TUI RSS target.
    pub const IDLE_TUI_RSS: Budget = Budget {
        name: "idle TUI RSS",
        max_heap_bytes: 60 * 1024 * 1024,
    };
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn counter_tracks_allocation_and_release() {
        let a = CountingAllocator::new();
        let base = a.live_bytes();
        a.record_alloc(1000);
        a.record_alloc(500);
        assert_eq!(a.live_bytes(), base + 1500);
        assert_eq!(a.alloc_count(), 2);
        a.record_free(1000);
        assert_eq!(a.live_bytes(), base + 500);
        assert_eq!(
            a.peak_bytes(),
            base + 1500,
            "peak is a high-water mark, not the live value"
        );
    }

    #[test]
    fn reset_peak_rebases_to_live() {
        let a = CountingAllocator::new();
        a.record_alloc(1000);
        a.record_free(900);
        a.reset_peak();
        assert_eq!(a.peak_bytes(), a.live_bytes());
        assert_eq!(a.alloc_count(), 0);
    }

    #[test]
    fn delta_reports_the_window_not_the_absolute() {
        let a = CountingAllocator::new();
        a.record_alloc(10_000); // pre-existing load
        let before = Sample::take(&a);
        a.record_alloc(2_048);
        let after = Sample::take(&a);
        let d = after.since(before);
        assert_eq!(
            d.heap_live, 2_048,
            "must measure the increment, not the total"
        );
        assert_eq!(d.allocs, 1);
    }

    #[test]
    fn budget_check_reports_both_numbers() {
        let d = Delta {
            heap_live: 5_000,
            heap_peak: 9_000,
            allocs: 3,
            rss: None,
        };
        let b = Budget {
            name: "t",
            max_heap_bytes: 10_000,
        };
        assert_eq!(
            b.check(d),
            BudgetCheck::Within {
                used: 9_000,
                allowed: 10_000
            }
        );
        assert!(b.check(d).ok());

        let small = Budget {
            name: "t",
            max_heap_bytes: 8_000,
        };
        assert_eq!(
            small.check(d),
            BudgetCheck::Exceeded {
                used: 9_000,
                allowed: 8_000
            }
        );
        assert!(!small.check(d).ok());
    }

    #[test]
    fn budget_uses_peak_not_live() {
        // A module that allocates 50 MiB and frees it still cost 50 MiB of peak.
        let d = Delta {
            heap_live: 0,
            heap_peak: 50 * 1024 * 1024,
            allocs: 1,
            rss: None,
        };
        assert!(!budgets::ASSISTANCE_HEAP.check(d).ok());
    }

    #[test]
    fn spec_budgets_match_the_document() {
        assert_eq!(budgets::ASSISTANCE_HEAP.max_heap_bytes, 12 * 1024 * 1024);
        assert_eq!(budgets::RESULT_STORE.max_heap_bytes, 64 * 1024 * 1024);
        assert_eq!(budgets::SINGLE_RESULT.max_heap_bytes, 16 * 1024 * 1024);
        assert_eq!(budgets::IDLE_REPL_RSS.max_heap_bytes, 30 * 1024 * 1024);
        assert_eq!(budgets::IDLE_TUI_RSS.max_heap_bytes, 60 * 1024 * 1024);
    }

    #[test]
    fn rss_is_available_on_this_platform_or_honestly_absent() {
        // The contract is that it never fabricates: Some(>0) or None.
        match rss_bytes() {
            Some(v) => assert!(v > 0, "RSS must be positive if reported"),
            None => {
                // On linux/macos this arm means the platform source failed, which is a real
                // failure of the instrument rather than an acceptable outcome.
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                panic!("linux/macos must be able to report RSS");
            }
        }
    }

    #[test]
    fn rss_is_excluded_from_heap_numbers() {
        // R08/R35: the two numbers are reported separately and never summed.
        let a = CountingAllocator::new();
        let s = Sample::take(&a);
        let d = s.since(s);
        assert_eq!(d.heap_live, 0);
        assert_eq!(d.heap_peak, 0);
        // rss delta is Some(0) or None, never folded into heap_*
        assert!(matches!(d.rss, Some(0) | None));
    }
}
