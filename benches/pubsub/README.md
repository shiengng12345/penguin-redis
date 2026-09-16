# Subscription storm baseline (V-H03, PERF-02, v2.1 §24.3 / §25.4)

Measured by `crates/pr-results/src/subscription.rs`, which runs in CI on all three platforms.
This file records the numbers a human read, so a later regression is a *diff* rather than an
argument.

## What §24.3 actually asks for

> 订阅记录 | 10,000 条且 16 MiB，先到者为准 | ring buffer，记录淘汰数量

Three decisions in one line, each preventing a different failure:

| decision | what it prevents |
|---|---|
| a **ring**, not a growing list | a channel at 100k msg/s fills any list; an OOM kill loses everything, including what the user was reading |
| **two** limits, not one | 10,000 × 2 KB is 20 MiB; 10,000 × 8 B is 80 KB. Neither limit alone bounds both cases |
| a **dropped count**, always shown | Redis Pub/Sub is at-most-once and a reconnect does not backfill (R17). A gap in what the user sees is a gap in what they can know |

## 2026-09-16 — Apple M3, macOS 26.5.2 (aarch64), rustc 1.97.1

| scenario | arriving | retained | bytes | dropped | limit that bit |
|---|---|---|---|---|---|
| 100,000 × 256 B | 25 MB | 10,000 | 3.64 MB | 90,000 | entry count |
| 100,000 × 4 KB | 400 MB | < 10,000 | ≤ 16 MiB | rest | byte size |
| 250 × 8 B, limit 100 | — | 100 | — | 150 | entry count |
| 200 × 2 KB, limit 64 KB | — | < 200 | ≤ 64 KB | rest | byte size |

**Throughput**: 100,000 messages of 256 B recorded in **14.9 ms** debug / **4.4 ms** release —
~6.7 M and ~22.7 M msg/s. CI asserts the debug figure, which is the conservative direction. The bar the test enforces is far lower and is about the right thing: recording
a second of 100k msg/s traffic must take well under a second, or the buffer *is* the
bottleneck and the terminal never gets a frame.

## The property that makes the counter trustworthy

    received == retained + dropped

Asserted after a 20,000-message storm with varying payload sizes, so nothing can be silently
unaccounted for. A dropped count that does not add up is worse than no count: it looks like
information.

## Deliberate choices a reader might question

- **A message larger than the whole buffer is refused, not admitted.** Admitting it would
  evict everything else to hold one thing nobody asked for. It is counted separately as
  `oversized` so the user is told which kind of loss happened.
- **Channel and pattern names count towards the byte budget.** A flood addressed to very long
  channel names costs the same memory as one with long payloads.
- **The status line is shown even when nothing was dropped.** A counter that appears only on
  bad news teaches people to read its absence as good news — and its absence is also what a
  bug looks like.
