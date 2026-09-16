# Startup and idle baselines (V-H01, v2.1 §24.6)

Measured by `crates/prc/tests/budgets.rs`, which runs in CI on all three platforms. This file
records the numbers a human read, so a later regression is a *diff* rather than an argument.

`prc` links the whole crate graph on purpose. V-H01 does not ask whether our code fits — there
is barely any feature code yet — it asks whether the dependencies have already eaten the
budget. Linking half of them could not answer that.

## 2026-09-16 — Apple M3, macOS 26.5.2 (aarch64), rustc 1.97.1

| measurement | budget (§24.6) | debug | release | headroom (release) |
|---|---|---|---|---|
| `prc --help` p50 | — | 2.47 ms | 3.72 ms | — |
| `prc --help` p95 | ≤ 100 ms | 3.47 ms | 4.86 ms | **95 ms** |
| idle REPL RSS | ≤ 30 MiB | 5.2 MiB | 5.0 MiB | **25 MiB** |
| idle TUI RSS | ≤ 60 MiB | 7.0 MiB | 6.5 MiB | **53 MiB** |
| catalog alone RSS | — | 5.2 MiB | 5.0 MiB | — |
| `prc` binary | — | — | 1.6 MiB | — |

The debug figures are the ones CI asserts. That is the conservative direction: a debug build
is larger and slower, so a debug build that fits proves a release build does.

## What is linked, and what is not

Linked: `tokio`, `rusqlite` (bundled SQLite), `keyring`, `crossterm`, `ratatui`, `blake3`,
`serde`/`serde_json`/`toml`, `unicode-width`, and the embedded catalog (~600 KB of pinned
snapshots for Redis 8.0.6 and Valkey 8.1.10, 586 merged commands).

**Not yet linked**, because the choice has not been made: the TLS stack and `redis-rs`.
`pr-transport` is still an empty crate. The headroom above — 95 ms and 25 MiB — is what those
have to fit inside, and this table must be re-measured when they land. Recording the headroom
rather than only the pass/fail is the point: it is the number a later decision is made from.

## Why `--help` is cheap, structurally

The catalog compiles lazily. That is a budget decision, not an optimisation: parsing and
compiling 586 commands does not fit in 100 ms and does not need to, because `--help` needs
none of it. `pr_catalog::embedded::is_loaded()` exists so a test can prove the fast path has
not touched it, and `budgets.rs` asserts that `--help` prints no catalog provenance while
`--version` does. A timing check alone would pass on a fast laptop and fail later on a loaded
runner; the structural check will not.
