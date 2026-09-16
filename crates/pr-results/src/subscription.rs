//! The subscription ring buffer (v2.1 §24.3, §25.4, PERF-02, R17, V-H03).
//!
//! §24.3 gives a subscription record 10,000 entries **and** 16 MiB, whichever comes first, in
//! a ring buffer that records how many it dropped. Three separate decisions are packed into
//! that sentence, and each prevents a different failure:
//!
//! - **a ring**, not a growing list: a channel doing 100k messages a second fills any list, and
//!   an out-of-memory kill loses everything including what the user was looking at
//! - **two limits**, not one: ten thousand 2 KB messages is 20 MiB, and ten thousand 8-byte
//!   ones is 80 KB. A count alone bounds neither; bytes alone lets a flood of tiny messages
//!   cost 10,000 allocations for nothing useful
//! - **a dropped count**, always shown: §25.4 is explicit that "有限缓冲可能淘汰消息，必须显示
//!   计数". Redis Pub/Sub is at-most-once and a reconnect does not backfill (R17), so a gap in
//!   what the user sees is a gap in what they can know — and hiding it makes an incomplete
//!   record look complete.

use bytes::Bytes;
use std::collections::VecDeque;

/// §24.3: maximum entries retained.
pub const MAX_ENTRIES: usize = 10_000;
/// §24.3: maximum bytes retained.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// One delivery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Which channel or shard channel it arrived on.
    pub channel: Bytes,
    /// The pattern that matched, for a `psubscribe` delivery.
    pub pattern: Option<Bytes>,
    /// The payload, exactly as it arrived.
    pub payload: Bytes,
    /// Milliseconds since the subscription started.
    pub at_ms: u64,
}

impl Message {
    /// Bytes this message costs the buffer.
    ///
    /// Counts the payload *and* the names, because a flood addressed to very long channel
    /// names costs just as much memory as one with long payloads.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.channel.len()
            + self.pattern.as_ref().map_or(0, Bytes::len)
            + self.payload.len()
            + std::mem::size_of::<Self>()
    }
}

/// Why a message was evicted, for the counter the user sees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dropped {
    /// Evicted because the entry limit was reached.
    pub by_count: u64,
    /// Evicted because the byte limit was reached.
    pub by_bytes: u64,
    /// Refused outright: a single message larger than the whole buffer.
    pub oversized: u64,
}

impl Dropped {
    /// Everything that did not make it into the record.
    #[must_use]
    pub fn total(self) -> u64 {
        self.by_count + self.by_bytes + self.oversized
    }
}

/// A bounded subscription record.
#[derive(Debug)]
pub struct Subscription {
    entries: VecDeque<Message>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
    received: u64,
    dropped: Dropped,
}

impl Default for Subscription {
    fn default() -> Self {
        Self::new()
    }
}

impl Subscription {
    /// A record with §24.3's limits.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(MAX_ENTRIES, MAX_BYTES)
    }

    /// A record with explicit limits, so a test can reach them without 16 MiB of fixtures.
    #[must_use]
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1),
            received: 0,
            dropped: Dropped::default(),
        }
    }

    /// Record a message, evicting the oldest as needed.
    ///
    /// A message larger than the whole buffer is refused rather than allowed to evict
    /// everything else: emptying the record to hold one thing nobody asked for is worse than
    /// not holding it, and the refusal is counted so the user is told.
    pub fn push(&mut self, m: Message) {
        self.received += 1;
        let w = m.weight();
        if w > self.max_bytes {
            self.dropped.oversized += 1;
            return;
        }
        while self.entries.len() + 1 > self.max_entries {
            self.evict(true);
        }
        while self.bytes + w > self.max_bytes {
            if self.entries.is_empty() {
                break;
            }
            self.evict(false);
        }
        self.bytes += w;
        self.entries.push_back(m);
    }

    fn evict(&mut self, by_count: bool) {
        if let Some(old) = self.entries.pop_front() {
            self.bytes = self.bytes.saturating_sub(old.weight());
            if by_count {
                self.dropped.by_count += 1;
            } else {
                self.dropped.by_bytes += 1;
            }
        }
    }

    /// Messages currently retained, oldest first.
    #[must_use]
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &Message> {
        self.entries.iter()
    }

    /// How many are retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Bytes currently retained.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// How many messages have arrived, retained or not.
    #[must_use]
    pub fn received(&self) -> u64 {
        self.received
    }

    /// How many did not make it into the record, and why.
    #[must_use]
    pub fn dropped(&self) -> Dropped {
        self.dropped
    }

    /// The line §25.4 requires the subscription view to show.
    ///
    /// Always shown, including when nothing was dropped: a counter that appears only on bad
    /// news teaches the user to read its absence as good news, and its absence is also what a
    /// bug looks like.
    #[must_use]
    pub fn status(&self) -> String {
        format!(
            "received {} · retained {} · dropped {} ({} by count, {} by size, {} too large)",
            self.received,
            self.entries.len(),
            self.dropped.total(),
            self.dropped.by_count,
            self.dropped.by_bytes,
            self.dropped.oversized,
        )
    }

    /// Whether every message that arrived is still retained.
    ///
    /// The question a user actually has: "am I looking at all of it?"
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.dropped.total() == 0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn msg(n: u64, payload_len: usize) -> Message {
        Message {
            channel: Bytes::from_static(b"news"),
            pattern: None,
            payload: Bytes::from(vec![b'x'; payload_len]),
            at_ms: n,
        }
    }

    #[test]
    fn an_empty_record_says_so_rather_than_saying_nothing() {
        let s = Subscription::new();
        assert!(s.is_empty());
        assert!(s.is_complete());
        assert!(s.status().contains("received 0"));
        assert!(s.status().contains("dropped 0"));
    }

    #[test]
    fn the_entry_limit_evicts_the_oldest_and_counts_it() {
        let mut s = Subscription::with_limits(100, MAX_BYTES);
        for n in 0..250 {
            s.push(msg(n, 8));
        }
        assert_eq!(s.len(), 100, "bounded");
        assert_eq!(s.received(), 250);
        assert_eq!(s.dropped().by_count, 150, "and the count is real");
        assert_eq!(s.dropped().by_bytes, 0);
        // A ring keeps the newest, because the newest is what the user is watching.
        assert_eq!(s.entries().next().unwrap().at_ms, 150);
        assert_eq!(s.entries().last().unwrap().at_ms, 249);
    }

    #[test]
    fn the_byte_limit_bites_first_when_messages_are_large() {
        // Ten thousand 2 KB messages is 20 MiB. A count limit alone would not bound this.
        let mut s = Subscription::with_limits(MAX_ENTRIES, 64 * 1024);
        for n in 0..200 {
            s.push(msg(n, 2048));
        }
        assert!(s.bytes() <= 64 * 1024, "{} bytes retained", s.bytes());
        assert!(s.len() < 200);
        assert_eq!(s.dropped().by_count, 0, "the count limit was never reached");
        assert!(s.dropped().by_bytes > 0);
        assert_eq!(s.received(), 200);
    }

    #[test]
    fn the_count_limit_bites_first_when_messages_are_tiny() {
        // The other direction: ten thousand 8-byte messages is 80 KB, nowhere near 16 MiB.
        let mut s = Subscription::with_limits(50, MAX_BYTES);
        for n in 0..120 {
            s.push(msg(n, 1));
        }
        assert_eq!(s.len(), 50);
        assert_eq!(s.dropped().by_count, 70);
        assert_eq!(s.dropped().by_bytes, 0);
    }

    #[test]
    fn a_message_bigger_than_the_whole_buffer_is_refused_not_admitted() {
        // Emptying the record to hold one thing nobody asked for is worse than not holding it.
        let mut s = Subscription::with_limits(MAX_ENTRIES, 4096);
        s.push(msg(0, 64));
        s.push(msg(1, 1_000_000));
        assert_eq!(s.len(), 1, "the earlier message survived");
        assert_eq!(s.entries().next().unwrap().at_ms, 0);
        assert_eq!(s.dropped().oversized, 1);
        assert!(!s.is_complete());
        assert!(s.status().contains("1 too large"));
    }

    #[test]
    fn the_accounting_adds_up_after_a_storm() {
        // The property that makes the counter trustworthy: nothing is unaccounted for.
        let mut s = Subscription::with_limits(500, 128 * 1024);
        for n in 0..20_000 {
            let len = usize::try_from(n % 700).unwrap_or(0);
            s.push(msg(n, len));
        }
        let d = s.dropped();
        assert_eq!(
            s.received(),
            d.total() + s.len() as u64,
            "received {} != dropped {} + retained {}",
            s.received(),
            d.total(),
            s.len()
        );
        assert!(s.len() <= 500);
        assert!(s.bytes() <= 128 * 1024);
    }

    #[test]
    fn perf_02_a_hundred_thousand_messages_stay_bounded() {
        // PERF-02, at §24.3's real limits. A second of a busy channel.
        let mut s = Subscription::new();
        for n in 0..100_000 {
            s.push(msg(n, 256));
        }
        assert_eq!(s.len(), MAX_ENTRIES, "exactly the entry limit");
        assert!(
            s.bytes() <= MAX_BYTES,
            "{} bytes retained, limit {MAX_BYTES}",
            s.bytes()
        );
        assert_eq!(s.received(), 100_000);
        assert_eq!(
            s.dropped().total(),
            100_000 - MAX_ENTRIES as u64,
            "every message is either retained or counted as dropped"
        );
        assert!(!s.is_complete(), "and the user is told it is incomplete");
    }

    #[test]
    fn perf_02_large_payloads_hit_the_byte_limit_not_the_entry_limit() {
        // 100k × 4 KB is 400 MB arriving. The record must stay at 16 MiB, and the *reason* it
        // stopped must be the one that actually applied.
        let mut s = Subscription::new();
        for n in 0..100_000 {
            s.push(msg(n, 4096));
        }
        assert!(s.bytes() <= MAX_BYTES, "{}", s.bytes());
        assert!(
            s.len() < MAX_ENTRIES,
            "the byte limit should have bitten first, retained {}",
            s.len()
        );
        assert!(s.dropped().by_bytes > 0);
        assert_eq!(s.received(), 100_000);
        assert_eq!(s.received(), s.dropped().total() + s.len() as u64);
    }

    #[test]
    fn perf_02_the_record_keeps_up_with_a_hundred_thousand_messages_a_second() {
        // The buffer is not allowed to *be* the bottleneck. If recording a message costs more
        // than 10 µs, a channel doing 100k/s spends its whole second in here and the terminal
        // never gets a frame.
        let messages: Vec<Message> = (0..100_000).map(|n| msg(n, 256)).collect();
        let mut s = Subscription::new();
        let t = std::time::Instant::now();
        for m in messages {
            s.push(m);
        }
        let elapsed = t.elapsed();
        println!(
            "PERF-02: 100,000 messages of 256 B in {elapsed:?} ({:.0} msg/s), \
             retained {}, {} bytes, dropped {}",
            100_000.0 / elapsed.as_secs_f64(),
            s.len(),
            s.bytes(),
            s.dropped().total()
        );
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "recording a second of traffic took {elapsed:?}, so the buffer cannot keep up"
        );
        assert_eq!(s.received(), 100_000);
    }

    #[test]
    fn the_status_line_is_shown_even_when_nothing_was_dropped() {
        // A counter that appears only on bad news teaches people to read its absence as good
        // news — and its absence is also what a bug looks like.
        let mut s = Subscription::new();
        s.push(msg(0, 8));
        assert!(s.is_complete());
        let line = s.status();
        assert!(line.contains("received 1"), "{line}");
        assert!(line.contains("retained 1"), "{line}");
        assert!(line.contains("dropped 0"), "{line}");
    }

    #[test]
    fn a_pattern_delivery_keeps_both_names_and_both_count_towards_the_budget() {
        // A flood addressed to very long channel names costs the same memory as one with long
        // payloads, so the names are weighed too.
        let m = Message {
            channel: Bytes::from(vec![b'c'; 1000]),
            pattern: Some(Bytes::from(vec![b'p'; 1000])),
            payload: Bytes::from_static(b"x"),
            at_ms: 0,
        };
        assert!(m.weight() > 2000, "the names are counted: {}", m.weight());

        let mut s = Subscription::with_limits(MAX_ENTRIES, 8192);
        for _ in 0..20 {
            s.push(m.clone());
        }
        assert!(s.bytes() <= 8192);
        assert!(s.dropped().by_bytes > 0);
        assert_eq!(s.entries().next().unwrap().pattern, m.pattern);
    }

    #[test]
    fn payload_bytes_are_kept_exactly_including_binary() {
        // R17: the record is what arrived. A payload mangled on the way in cannot be compared
        // with what a producer sent.
        let mut s = Subscription::new();
        let payload = Bytes::from_static(&[0x00, 0xff, 0x1b, b'\n']);
        s.push(Message {
            channel: Bytes::from_static(b"bin"),
            pattern: None,
            payload: payload.clone(),
            at_ms: 0,
        });
        assert_eq!(s.entries().next().unwrap().payload, payload);
    }
}
