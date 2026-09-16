//! The assistance working set and its budget (v2.1 §12.5, §16.4, §24.7, R08, R35, V-F09).
//!
//! §12.5 caps 「普通 assistance **堆**增量工作集」 at **12 MiB**, and breaks that down:
//!
//! | sub-item | budget |
//! |---|---|
//! | 已观察 key 名缓存 | 5,000 个或 2 MiB，先到者为准 |
//! | field/member 名缓存 | 2 MiB |
//! | metadata/docs 解析缓存 | 2 MiB |
//! | scratch 与候选/索引 | 2 MiB |
//! | 增量分析器状态 + 待返回候选 | 1 MiB |
//! | 发现响应处理 | 1 MiB |
//! | **合计** | **10 MiB + 2 MiB 余量 = 12 MiB** |
//!
//! Two things follow that are easy to get wrong:
//!
//! 1. **The cap is on the increment, measured with assistance on versus off**, after the
//!    catalog is loaded. Measuring assistance on its own would either include the catalog (and
//!    fail for the wrong reason) or exclude the allocator's own overhead (and pass for one).
//! 2. **The sub-budgets have to add up.** If every one of them is filled to its limit and the
//!    total exceeds 12 MiB, the budget is internally inconsistent — and that is a finding
//!    about the budget, not a failure of the code. [`WorkingSet::saturate`] exists so the
//!    measurement is of the worst case that is still *inside* the stated limits.
//!
//! §24.7 also requires the read-only mmap catalog to be reported separately and excluded from
//! the heap increment. It is excluded here by construction: the catalog is loaded before the
//! baseline sample is taken, so it appears in neither side of the difference.

use crate::scope::{Observation, ObservationScope, ObservationStore, Origin};

/// §12.5's sub-budgets, in bytes.
pub mod limits {
    /// Observed key names: 5,000 entries **or** 2 MiB, whichever comes first.
    pub const OBSERVED_KEYS_ENTRIES: usize = 5_000;
    /// The byte half of the same limit.
    pub const OBSERVED_KEYS_BYTES: usize = 2 * 1024 * 1024;
    /// Field and member names, across all keys.
    pub const FIELD_NAMES_BYTES: usize = 2 * 1024 * 1024;
    /// Parsed metadata and documentation, hot working set.
    pub const METADATA_BYTES: usize = 2 * 1024 * 1024;
    /// Scratch, candidates and the index that ranks them.
    pub const SCRATCH_BYTES: usize = 2 * 1024 * 1024;
    /// Incremental analyser state plus candidates not yet returned.
    pub const ANALYSER_BYTES: usize = 1024 * 1024;
    /// Discovery response handling.
    pub const DISCOVERY_BYTES: usize = 1024 * 1024;

    /// What the sub-items add up to.
    pub const SUB_ITEM_TOTAL: usize = OBSERVED_KEYS_BYTES
        + FIELD_NAMES_BYTES
        + METADATA_BYTES
        + SCRATCH_BYTES
        + ANALYSER_BYTES
        + DISCOVERY_BYTES;

    /// §12.5's cap on the whole increment: the sub-items plus 2 MiB of margin.
    pub const HEAP_INCREMENT: usize = SUB_ITEM_TOTAL + 2 * 1024 * 1024;
}

/// A byte-bounded cache of names.
///
/// Both halves of 「5,000 个或 2 MiB，先到者为准」 are enforced, because either alone is the
/// wrong limit: 5,000 keys of 400 bytes is 2 MiB, and 5,000 keys of 40 KiB is 200.
#[derive(Debug)]
pub struct BoundedNames {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
    /// How many were dropped to stay inside the budget. Shown rather than hidden: a cache
    /// that silently forgets looks, from the outside, exactly like a server that lost data.
    pub evicted: u64,
}

impl BoundedNames {
    /// A cache with both limits.
    #[must_use]
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
            max_entries,
            max_bytes,
            evicted: 0,
        }
    }

    /// Bytes currently held, **including the container's own overhead**.
    ///
    /// V-F09 measured a 2 MiB budget costing 5.2 MiB of real allocation. The gap was entirely
    /// bookkeeping the budget did not count: 48 bytes of `(Vec, Vec)` per entry, and a `Vec`
    /// whose capacity had doubled past what was in use. **A budget that ignores container
    /// overhead is not a budget** — reserved-but-unused capacity is resident memory, and the
    /// process pays for it whether or not the cache admits to it.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes + self.entries.capacity() * std::mem::size_of::<(Vec<u8>, Vec<u8>)>()
    }

    /// Entries currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert, evicting oldest-first until both limits hold.
    ///
    /// An item larger than the whole budget is refused rather than emptying the cache to make
    /// room for it: 「超长名字受单项限制，仍可手输」.
    pub fn insert(&mut self, owner: &[u8], name: &[u8]) -> bool {
        const SLOT: usize = std::mem::size_of::<(Vec<u8>, Vec<u8>)>();
        let cost = owner.len() + name.len() + SLOT;
        if cost > self.max_bytes {
            return false;
        }
        while !self.entries.is_empty()
            && (self.entries.len() + 1 > self.max_entries || self.bytes() + cost > self.max_bytes)
        {
            let (o, n) = self.entries.remove(0);
            self.bytes -= o.len() + n.len() + SLOT;
            self.evicted += 1;
        }
        self.entries.push((owner.to_vec(), name.to_vec()));
        self.bytes += cost;
        true
    }
}

/// Everything assistance holds beyond the catalog.
///
/// Held together in one struct so "turn assistance off" is `drop`, and so the measurement has
/// exactly one thing to switch.
#[derive(Debug)]
pub struct WorkingSet {
    /// Observed key names.
    pub keys: ObservationStore,
    /// Field and member names, bound to their key.
    pub fields: BoundedNames,
    /// Parsed metadata and docs.
    pub metadata: Vec<u8>,
    /// Scratch, candidates, ranking index.
    pub scratch: Vec<u8>,
    /// Analyser state and candidates not yet returned.
    pub analyser: Vec<u8>,
    /// Discovery response handling.
    pub discovery: Vec<u8>,
}

impl Default for WorkingSet {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkingSet {
    /// An empty working set — assistance on, nothing learned yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            keys: ObservationStore::new(limits::OBSERVED_KEYS_ENTRIES),
            fields: BoundedNames::new(usize::MAX, limits::FIELD_NAMES_BYTES),
            metadata: Vec::new(),
            scratch: Vec::new(),
            analyser: Vec::new(),
            discovery: Vec::new(),
        }
    }

    /// Fill every sub-budget to its stated limit.
    ///
    /// This is the measurement's worst case, and it is a case that is still *inside* the
    /// budget: every sub-item is at, not over, what §12.5 allows. If the total then exceeds
    /// 12 MiB, the sub-budgets and the cap disagree with each other, which is worth finding
    /// out in Phase 0 rather than in Phase 2.
    pub fn saturate(&mut self, scope: &ObservationScope) {
        self.saturate_keys(scope);
        self.saturate_fields();
        // The remaining sub-items are opaque byte budgets; filling them exactly is what the
        // measurement needs, and pretending they have structure would not make it truer.
        self.metadata = vec![0xab; limits::METADATA_BYTES];
        self.scratch = vec![0xcd; limits::SCRATCH_BYTES];
        self.analyser = vec![0xef; limits::ANALYSER_BYTES];
        self.discovery = vec![0x12; limits::DISCOVERY_BYTES];
    }

    /// Fill the observed-name cache until one of its two limits stops it.
    pub fn saturate_keys(&mut self, scope: &ObservationScope) {
        for i in 0..limits::OBSERVED_KEYS_ENTRIES * 2 {
            // A realistic length: long enough that the byte limit is reachable, short enough
            // that the entry limit is too. Which one binds is the thing being observed.
            let name = format!("penguin:fixture:player:{i:012}:profile").into_bytes();
            let before = self.keys.len();
            self.keys.record(Observation {
                name,
                scope: scope.clone(),
                origin: Origin::RetainedResult,
                parent_key: None,
            });
            if self.keys.len() == before && !self.keys.is_empty() {
                // Eviction started: the cache is at a limit and will stay there.
                break;
            }
        }
    }

    /// Fill the field cache to its byte budget.
    pub fn saturate_fields(&mut self) {
        let owner = b"penguin:fixture:player:10001";
        let mut i = 0u64;
        loop {
            let name = format!("field:{i:016}:value").into_bytes();
            let before = self.fields.len();
            if !self.fields.insert(owner, &name) {
                break;
            }
            if self.fields.len() == before {
                break;
            }
            i += 1;
            if i > 1_000_000 {
                break;
            }
        }
    }

    /// Bytes this working set accounts for, by its own reckoning.
    ///
    /// Useful as a cross-check against the allocator: if the two disagree wildly, one of them
    /// is wrong, and finding that out is the point of having both.
    #[must_use]
    pub fn accounted_bytes(&self) -> usize {
        self.fields.bytes()
            + self.metadata.len()
            + self.scratch.len()
            + self.analyser.len()
            + self.discovery.len()
            + self.keys.bytes()
    }
}
