//! V-A02 — differential harness: `prc` against a pinned `redis-cli`, four layers, two servers.
//!
//! §32.2 is specific about the shape of this, and the specificity is the point:
//!
//! > 测试同一个写命令时，官方和 Penguin 使用两个独立但相同初始状态的 Redis 环境。不能先在同一
//! > 实例执行官方 `INCR` 再执行 Penguin `INCR`，然后比较结果并误判。
//!
//! Running both clients against one server and comparing what each printed looks like a
//! differential test and is not one: the second command sees the first one's effects, so
//! `INCR` "differs" for a reason that has nothing to do with either client. This harness runs
//! two servers from the same pinned digest, seeds both identically, and compares:
//!
//! 1. **argv bytes** — recorded on the wire, not inferred from what was typed
//! 2. **reply bytes** — likewise, verbatim, including framing
//! 3. **final server state** — read back from both servers by the *same* reader, so the layer
//!    measures the servers rather than the clients
//! 4. **output and exit code** — stdout, stderr and the process's status
//!
//! Layers 1–3 must be identical or the case fails. Layer 4 is allowed to differ, because
//! `prc` is not trying to be `redis-cli` — but every difference must be **named in the case**
//! before the run, so a new difference appearing tomorrow is a failure rather than a shrug.

pub mod cases;
pub mod compare;
pub mod docker;
pub mod proxy;
pub mod report;

pub use cases::{Case, StateProbe, cases};
pub use compare::{LayerVerdict, Outcome, compare};
pub use report::{CaseReport, Report};
