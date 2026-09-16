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

## 2026-09-16, re-measured — after V-B01 (ADR-031) and V-A02's passthrough landed

Same machine and toolchain. Re-measured because the previous run's caveat said to: at that
point `pr-transport` was empty and the protocol-library question was open.

| measurement | budget (§24.6) | debug | release | headroom (release) | vs first run |
|---|---|---|---|---|---|
| `prc --help` p50 | — | 2.05 ms | 1.77 ms | — | −1.95 ms |
| `prc --help` p95 | ≤ 100 ms | 2.30 ms | 1.91 ms | **98 ms** | −2.95 ms |
| idle REPL RSS | ≤ 30 MiB | 5.2 MiB | 4.3 MiB | **25.7 MiB** | −0.7 MiB |
| idle TUI RSS | ≤ 60 MiB | 6.9 MiB | 6.2 MiB | **53.8 MiB** | −0.3 MiB |
| catalog alone RSS | — | 5.0 MiB | 4.5 MiB | — | −0.5 MiB |
| `prc` binary | — | — | 1.69 MiB | — | +0.09 MiB |

Nothing regressed, which needs saying carefully rather than celebrating: the run-to-run spread
on an idle laptop is larger than most of these deltas, so the honest reading is "the transport
and the passthrough cost nothing measurable", not "startup got 2 ms faster". The binary grew
90 KB, which is `pr-transport` plus the passthrough renderer, and that number is real.

The debug figures are the ones CI asserts. That is the conservative direction: a debug build
is larger and slower, so a debug build that fits proves a release build does.

## 2026-09-16, third measurement — TLS（V-G04）与 SSH（V-G03）都已链接

上一节自己写着「这两项落地时必须重测」。它们落地了，所以这一节存在。

| 字段 | 值 |
|---|---|
| 硬件 | Apple M3 · 24 GB |
| OS | macOS 26.5.2（aarch64） |
| 工具链 | rustc 1.97.1 (8bab26f4f 2026-07-14)，由 `rust-toolchain.toml` 固定 |
| commit | `bdc15b2` |
| 依赖锁 | `Cargo.lock` sha256 `7a8f27ca7770932b…`，281 个 package |
| 并发负载 | **有**：V-H05 的 8 小时 soak 正在同一台机器上跑（见下） |

| measurement | budget (§24.6) | debug | release | headroom (release) | vs 第二次 |
|---|---|---|---|---|---|
| `prc --help` p50 | — | 4.11 ms | 2.09 ms | — | +0.32 ms |
| `prc --help` p95 | ≤ 100 ms | 8.15 ms | 5.12 ms | **94.9 ms** | +3.21 ms |
| idle REPL RSS | ≤ 30 MiB | 5.2 MiB | 4.5 MiB | **25.5 MiB** | +0.2 MiB |
| idle TUI RSS | ≤ 60 MiB | 7.2 MiB | 6.1 MiB | **53.9 MiB** | −0.1 MiB |
| catalog alone RSS | — | 5.1 MiB | 4.5 MiB | — | ±0 |
| `prc` binary | — | 5.70 MiB | 1.85 MiB | — | **+0.16 MiB** |

**只有一个数字是可以直接解读的：二进制 +0.16 MiB。** 那是 rustls 0.23 + `ring` 加上 SSH 那条
路径的代价，它与机器忙不忙无关。

**时间数字这次不干净，必须说清楚。** p95 从 1.91 ms 涨到 5.12 ms，看起来像 TLS 让启动变慢了
2.7 倍——但 TLS 栈在 `--help` 路径上一行都不执行，而这次测量是在 soak 占着 CPU 的情况下做的。
正确的结论是「这一列本次不可比」，不是「启动变慢了」。干净的复测在下一节，soak 跑完之后补。
把一个知道有污染的数字写成结论，比不写更糟：后面的人会拿它当基线去对比。

RSS 的变化（+0.2 MiB / −0.1 MiB）小于同一台空闲笔记本上的 run-to-run 抖动，读作「没有可测量的
变化」。

**上一节的「尚未链接」注记到此清空**：TLS 与 SSH 都在里面了，没有留待将来吃掉的 headroom。

## What is linked, and what is not

Linked: `tokio`, `rusqlite` (bundled SQLite), `keyring`, `crossterm`, `ratatui`, `blake3`,
`serde`/`serde_json`/`toml`, `unicode-width`, and the embedded catalog (~600 KB of pinned
snapshots for Redis 8.0.6 and Valkey 8.1.10, 586 merged commands).

`pr-transport` is no longer empty: V-A02 needed a passthrough to run the differential harness
against, so it now carries `oneshot` — a blocking TCP client and the RESP command encoder. It
is deliberately small and Phase 1 replaces it.

**`redis-rs` will never be linked.** ADR-031 settled that: it is a dev-dependency and an
interop test subject, and `ci/check-redis-rs-is-dev-only.sh` resolves the real link graph to
keep it one. So the headroom below does not have to hold room for it.

**Now linked, and measured**: the TLS stack (rustls 0.23 + `ring`, ADR-032, V-G04) and the
controlled SSH path (V-G03). They were the outstanding claim on the headroom; the third
measurement above is what they actually cost — **+0.16 MiB of binary and nothing measurable
anywhere else**. Recording the headroom rather than only the pass/fail is what made that
answerable: 94.9 ms and 25.5 MiB are still free.

## Why `--help` is cheap, structurally

The catalog compiles lazily. That is a budget decision, not an optimisation: parsing and
compiling 586 commands does not fit in 100 ms and does not need to, because `--help` needs
none of it. `pr_catalog::embedded::is_loaded()` exists so a test can prove the fast path has
not touched it, and `budgets.rs` asserts that `--help` prints no catalog provenance while
`--version` does. A timing check alone would pass on a fast laptop and fail later on a loaded
runner; the structural check will not.
