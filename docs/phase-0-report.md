# Phase 0 报告（P0-META-01）

> 依据 `penguin-redis-phase-plan.md` §2.3 / §2.4。每个 V 项一行：状态 + 证据链接。  
> 合法终态只有 `PASS` 与 `FALLBACK-ADOPTED(ADR-xxx)`。**没有 DEFERRED / SKIPPED 列。**  
> 状态说明：`IN-PROGRESS` = 进行中；`BLOCKED(<原因>)` = 被外部条件阻塞（必须写明解阻条件，且不是终态）。

## 汇总

| 状态 | 数量 |
|---|---|
| PASS | 18 |
| FALLBACK-ADOPTED | 1 |
| IN-PROGRESS | 44 |
| BLOCKED | 0 |

## 环境记录

| 项 | 值 |
|---|---|
| 主机 | Apple M3 · 24 GB · macOS 26.5.2 (aarch64) |
| Rust | 1.97.1 stable（`rust-toolchain.toml` 固定） |
| Docker | 29.7.2 client / 29.5.2 daemon · linux/arm64 |
| 本机 Redis | 8.4.0（仅作对照，不进矩阵） |
| gh | 2.88.1 · 已登录 `shiengng12345` |
| 远端 | `https://github.com/shiengng12345/penguin-redis` |
| Windows / Linux runner | 本机无；通过 GitHub Actions `windows-latest` / `ubuntu-latest`（V-A05） |

## Track A · 基础设施与 harness

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-A01 | PASS | `xtask/pty-harness/` · 8 tests · portable-pty 0.9 | 统一 Unix PTY + Windows ConPTY；send/resize/bracketed-paste/wait_for；录制可回放且 golden 确定性 |
| V-A02 | IN-PROGRESS | — | |
| V-A03 | PASS | `xtask/resp-server/` · `fixtures/protocol/` (316 samples / 12 categories) · 15 tests | RESP2/3 encoder incl. streamed+push+attribute; scripted delivery; 1 GiB streamer; hostile corpus每类≥20 |
| V-A04 | PASS | `xtask/src/fault.rs` · 8 tests | TCP 代理：客户端/服务端定点切断、blackhole、慢速、分片、单字节损坏、拒连；FS：只读/填充 |
| V-A05 | PASS | CI run 35053565259 · `.github/workflows/ci.yml` · `ci/*.sh` | **17/17 全绿**：三平台 build+lint 与 test、5 个 digest-pinned server、cluster、sentinel、4 个 gate |
| V-A06 | PASS | `xtask/src/measure.rs` · 8 tests · `cargo run -p xtask -- measure-baseline` | 计数 allocator（live/peak/calls）+ RSS 采样 + 预算表；heap 与 RSS 分列不混计 |

## Track B · 协议与执行内核

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-B01 | IN-PROGRESS | — | |
| V-B02 | PASS | `crates/pr-protocol/` · 23 tests（含 316 样本 corpus 逐字节喂入） | 增量 RESP2/3 解码含 streamed；预算与协议错误分离；词法保真；bare-LF 即时报错 |
| V-B03 | IN-PROGRESS | — | |
| V-B04 | IN-PROGRESS | — | `pr-core::outcome` 四维度+退出码映射已实现并测试；待 V-A03 组合矩阵全量 |
| V-B05 | PASS | `crates/pr-core/src/session.rs` · 18 tests | HELLO/AUTH/SELECT/RESET/MULTI/WATCH/CLIENT REPLY 状态机；`may_probe()` 在事务内与 reply-suppressed 时拒绝注入；断线返回 `ReconnectLosses` 且不静默恢复 |
| V-B06 | IN-PROGRESS | — | |
| V-B07 | PASS | `crates/prc/src/args.rs` · 17 tests | §3.2 顶层语法；§3.3 profile×flag 矩阵逐格用例；`-h` 恒为 host；输出模式互斥不做 last-wins |

## Track C · 终端与编辑器

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-C01 | FALLBACK-ADOPTED(ADR-026) | `docs/spikes/SPIKE-001.md` · `crates/pr-repl/src/buffer.rs` · 20 tests | Reedline 不满足 (b)：undo 在私有 `Editor` 上；且 mid-codepoint 光标会在库内 panic。采用预命名回退：自有 grapheme LineBuffer + EditTransaction |
| V-C02 | IN-PROGRESS | — | |
| V-C03 | IN-PROGRESS | — | |
| V-C04 | IN-PROGRESS | — | |
| V-C05 | IN-PROGRESS | — | |
| V-C06 | PASS | `crates/pr-render/src/theme.rs` · 14 tests | Dark/Light/High-contrast 三套 token 全定义；WCAG 对比度自动校验；24bit→256→16→mono 量化后 JSON token 仍可分辨；**mono 保留 bold、plain 零转义** |
| V-C07 | IN-PROGRESS | — | |

## Track D · 安全与凭证

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-D01 | PASS | `crates/pr-profiles/src/credentials.rs` · 9 tests · 本机 Keychain 实跑 | keyring 3 按平台 feature；`credential:<uuid>` 引用；无可用 store 时 **fail-closed**，回退选项里没有明文文件；keyutils 明确标为非持久 |
| V-D02 | PASS | `crates/pr-profiles/src/journal.rs` · 12 tests | §4.2 六步顺序 + fsync journal + 启动 reconciliation；崩溃点逐一测试：孤儿 secret 只报告不自动删、悬空 profile 标 credential missing、未完成轮换清理可重放且幂等 |
| V-D03 | IN-PROGRESS | — | `pr-security::approval` 实现+10 测试通过（含 SEC-10 同摘要不同字节）；待接入 kernel |
| V-D04 | IN-PROGRESS | — | `pr-security::trust` 实现+10 测试通过；待 Cluster/Sentinel fixture (V-G01/G02) |
| V-D05 | IN-PROGRESS | — | |
| V-D06 | IN-PROGRESS | — | |
| V-D07 | IN-PROGRESS | — | |
| V-D08 | IN-PROGRESS | — | |
| V-D09 | IN-PROGRESS | — | |

## Track E · 数据模型

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-E01 | PASS | `crates/pr-json/` · 17 tests · `docs/spikes/SPIKE-004.md` | occurrence DOM；重复成员保留+`#n` 寻址；数字词法逐字节保留；歧义路径拒绝；span 局部重写 |
| V-E02 | PASS | `crates/pr-render/src/{table,width}.rs` · 43 tests | 每 field 横线、表头双线、JSON 在 cell 内不加内部横线；宽度采样后冻结；窄屏改纵向块不隐藏列；宽字符/ZWJ/组合字符帧对齐；SEC-05 控制字节不外泄 |
| V-E03 | IN-PROGRESS | — | |
| V-E04 | IN-PROGRESS | — | |
| V-E05 | IN-PROGRESS | — | |

## Track F · 智能输入引擎

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-F01 | IN-PROGRESS | — | |
| V-F02 | IN-PROGRESS | — | |
| V-F03 | PASS | `crates/pr-repl/` · 26 tests | 权威 tokenizer 对齐 `sdssplitargs`；全 256 字节 quote 往返；`:` 本地命令独立 grammar（重复选项报错、零 shell 展开） |
| V-F04 | IN-PROGRESS | — | |
| V-F05 | IN-PROGRESS | — | |
| V-F06 | IN-PROGRESS | — | |
| V-F07 | IN-PROGRESS | — | |
| V-F08 | IN-PROGRESS | — | |
| V-F09 | IN-PROGRESS | — | |
| V-F10 | IN-PROGRESS | — | |

## Track G · 连接拓扑

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-G01 | PASS | `ci/topology/cluster-{up,down}.sh` · `crates/pr-routing/` 11 tests + 5 个 `topology_cluster_*` 实跑 | 6 节点 cluster_state:ok / 16384 slots；hash tag CRC16 与真服务器 7 个 key 逐一吻合；CROSSSLOT 不被当重定向 |
| V-G02 | PASS | `ci/topology/sentinel-{up,down}.sh` · 3 个 `topology_sentinel_*` | 1 primary + 1 replica + 3 sentinel；根因是缺 `sentinel resolve-hostnames yes`（6.2+ 默认拒绝主机名） |
| V-G03 | IN-PROGRESS | — | |
| V-G04 | IN-PROGRESS | — | |
| V-G05 | IN-PROGRESS | — | |

## Track H · 资源预算与性能基线

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-H01 | IN-PROGRESS | — | |
| V-H02 | IN-PROGRESS | — | |
| V-H03 | IN-PROGRESS | — | |
| V-H04 | IN-PROGRESS | — | `pr-core::scope::TaskScope` 实现，含 1000 次开关与阻塞任务测试；待接入真实会话 |
| V-H05 | IN-PROGRESS | — | |
| V-H06 | IN-PROGRESS | — | |

## Track I · 兼容矩阵、catalog 与许可

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-I01 | PASS | `compatibility/manifest.toml` · CI job `manifest-pinned` | 5 服务器 target 全部 digest 固定；无 `latest`；gate 脚本强制 |
| V-I02 | IN-PROGRESS | — | |
| V-I03 | IN-PROGRESS | — | |
| V-I04 | IN-PROGRESS | — | |

## Track J · 契约冻结与 ADR

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-J01 | PASS | `docs/adr/ADR-001..030.md` + `README.md` | 30 条全部 accepted；ADR-023/026 的**结论**分别由 V-I03 / SPIKE-001 填入 |
| V-J02 | IN-PROGRESS | — | 进行中：`pr-core`(SafeText/ExecutionOutcome/CommandRequest/TaskScope)、`pr-security`(TrustIdentity/ApprovalToken)、`pr-json`(JsonNode) 已冻结；`pr-intelligence`/`pr-repl` 待办 |
| V-J03 | IN-PROGRESS | — | |
| V-J04 | IN-PROGRESS | — | |

## Meta

| ID | 状态 | 证据 |
|---|---|---|
| P0-META-01 | IN-PROGRESS | 本文件 |
| P0-META-02 | IN-PROGRESS | — |
| P0-META-03 | IN-PROGRESS | — |

## 变更日志

| 日期 | 变更 |
|---|---|
| 2026-09-16 | 建立报告；workspace 骨架；环境记录 |
| 2026-09-16 | V-A03 / V-I01 / V-J01 → PASS；pr-core + pr-security 契约冻结；CI workflow 与 4 个 gate 脚本落地 |
| 2026-09-16 | V-E01 → PASS（pr-json occurrence DOM，SPIKE-004）；workspace 85 tests 全绿 |
| 2026-09-16 | V-A01 / V-A04 / V-A06 → PASS（PTY harness、故障注入、资源测量）；111 tests 全绿 |
| 2026-09-16 | V-B02 → PASS（增量 RESP decoder）；首次 CI 实跑暴露 4 个真实问题并修复；134 tests 全绿 |
| 2026-09-16 | V-F03 → PASS（quoting + 本地命令 grammar）；第二轮 CI 暴露 5 个问题并修复；Cluster/Sentinel 起停脚本落地；160 tests 全绿 |
| 2026-09-16 | V-B07 → PASS（prc 参数契约）；177 tests 全绿 |
| 2026-09-16 | CI run 35048391825：15/17 绿。cluster/sentinel 脚本首跑失败，已记入 V-G01/V-G02 |
| 2026-09-16 | V-G01 / V-G02 → PASS：两个 topology 根因定位并修复；新增 `pr-routing` slot/hash-tag 实现与 8 个实跑测试；196 tests 全绿 |
| 2026-09-16 | **CI 17/17 全绿** → V-A05 PASS；SPIKE-001 结论：Reedline 不达标，V-C01 走 ADR-026 回退并实现自有 LineBuffer；216 tests 全绿 |
| 2026-09-16 | V-B05 / V-D01 → PASS（会话状态机、三平台凭证存储）；244 tests 全绿 |
| 2026-09-16 | V-D02 → PASS（凭证 journal 与崩溃 reconciliation）；256 tests 全绿 |
| 2026-09-16 | V-C06 → PASS（三主题 token + 量化降级）；测试暴露 mono/plain 语义混淆并分离；270 tests 全绿 |
| 2026-09-16 | V-E02 → PASS（TableModel + 显示宽度策略）；299 tests 全绿 |
