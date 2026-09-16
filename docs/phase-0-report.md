# Phase 0 报告（P0-META-01）

> 依据 `penguin-redis-phase-plan.md` §2.3 / §2.4。每个 V 项一行：状态 + 证据链接。  
> 合法终态只有 `PASS` 与 `FALLBACK-ADOPTED(ADR-xxx)`。**没有 DEFERRED / SKIPPED 列。**  
> 状态说明：`IN-PROGRESS` = 进行中；`BLOCKED(<原因>)` = 被外部条件阻塞（必须写明解阻条件，且不是终态）。

## 汇总

| 状态 | 数量 |
|---|---|
| PASS | 33 |
| FALLBACK-ADOPTED | 1 |
| IN-PROGRESS | 29 |
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
| V-B03 | PASS | `crates/pr-application/src/pipe.rs` · 14 tests | 逐帧解码→同一 catalog 分类→同一 policy；prod 遇写帧即停不跳过（SEC-09）；畸形/截断/非命令帧 fail-closed 并报帧号与字节偏移（PIPE-08）；agent 完全不可用；停止帧之后一帧都不放行 |
| V-B04 | PASS | `crates/pr-protocol/tests/outcome_matrix.rs` · 12 tests | 经真实 wire 字节驱动：截断回复/连接关闭/协议错误 → `UnknownAfterSend`；分片不改变结论；push 不占回复槽；全组合映射表 + 每个退出码可达性 |
| V-B05 | PASS | `crates/pr-core/src/session.rs` · 18 tests | HELLO/AUTH/SELECT/RESET/MULTI/WATCH/CLIENT REPLY 状态机；`may_probe()` 在事务内与 reply-suppressed 时拒绝注入；断线返回 `ReconnectLosses` 且不静默恢复 |
| V-B06 | IN-PROGRESS | — | |
| V-B07 | PASS | `crates/prc/src/args.rs` · 17 tests | §3.2 顶层语法；§3.3 profile×flag 矩阵逐格用例；`-h` 恒为 host；输出模式互斥不做 last-wins |

## Track C · 终端与编辑器

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-C01 | FALLBACK-ADOPTED(ADR-026) | `docs/spikes/SPIKE-001.md` · `crates/pr-repl/src/buffer.rs` · 20 tests | Reedline 不满足 (b)：undo 在私有 `Editor` 上；且 mid-codepoint 光标会在库内 panic。采用预命名回退：自有 grapheme LineBuffer + EditTransaction |
| V-C02 | PASS | `crates/pr-terminal/`（ownership/event/paint/coordinator + `pr-terminal-demo`）· `tests/owner.rs` 19 tests · `tests/pty_owner.rs` 7 PTY tests | 「无第二个 stdin 读者/并发 stdout 写者」做成资源而非约定：`Ownership` 进程级 token，第二个 `Coordinator::new` 返回 `AlreadyOwned`，`Painter` 必须同时持 token 与 `&mut Sink`；worker 只能 `Mailbox::post`，由 coordinator 决定何时 print-and-redraw。ASSIST-062：25/20 条 push 涌入与打字交错，buffer、光标、菜单焦点均不变，通知逐条进 scrollback 且从不落在 prompt 行内；UX-12/场景 H：F1 帮助不动命令框，Esc 先关帮助再退 TUI，草稿与来源 `@r2 / DB 0 / result #17` 原样带回且**不执行**；alternate screen 进/出各恰好一次，TUI 期间通知排队、回到 REPL 后按序补印 |
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
| V-D05 | PASS | `crates/pr-render/tests/safetext_boundary.rs` · 10 tests | 15 个真实终端控制 payload × value/key/列标题/generic renderer/4 主题×4 色深；剥离自有 SGR 后断言无 ESC/BEL/NUL/C1/bidi；Debug 与 Display 同样惰性；转义幂等 |
| V-D06 | PASS（history 路径） | `crates/pr-repl/src/history.rs` 泄漏测试 · `pr-profiles` journal/凭证测试 | 按环境分级脱敏（dev 留名 / staging 哈希 / prod 全占位）；AUTH·HELLO·CONFIG·ACL·MIGRATE 无视环境一律脱敏；**直接 grep SQLite 文件断言密码与 PII 不落盘**；搜索只能看到脱敏后文本 |
| V-D07 | PASS | `crates/pr-catalog/src/precedence.rs` · 10 tests | 本地 catalog 是 effects 唯一权威；服务器谎称 write 为 readonly 时分类不变、仅标 `server reports`；未分类命令即使服务器说 readonly 仍算 mutating |
| V-D08 | IN-PROGRESS | — | |
| V-D09 | IN-PROGRESS | — | |

## Track E · 数据模型

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-E01 | PASS | `crates/pr-json/` · 17 tests · `docs/spikes/SPIKE-004.md` | occurrence DOM；重复成员保留+`#n` 寻址；数字词法逐字节保留；歧义路径拒绝；span 局部重写 |
| V-E02 | PASS | `crates/pr-render/src/{table,width}.rs` · 43 tests | 每 field 横线、表头双线、JSON 在 cell 内不加内部横线；宽度采样后冻结；窄屏改纵向块不隐藏列；宽字符/ZWJ/组合字符帧对齐；SEC-05 控制字节不外泄 |
| V-E03 | PASS | `crates/pr-results/src/store.rs` · 14 tests | 64 MiB/16 MiB 预算、oldest-first 淘汰；`Evicted`/`TooLarge`/`Unknown`/`ScopeExpired` 四种原因分开报；**§8.2 顺序**：已知长度且未开始输出才可询问，非 TTY 永不询问 |
| V-E04 | IN-PROGRESS | — | |
| V-E05 | PASS | `crates/pr-render/src/generic.rs` · 16 tests | 未知/模块结果走 generic RESP tree：double 保留词法、map 保序且保留重复 key、非 UTF-8 转义并报字节数、attribute 不丢弃、深度有界；任意 shape 不 panic（CMD-10/12、SEC-07） |

## Track F · 智能输入引擎

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-F01 | PASS | `crates/pr-catalog/snapshots/` (redis 8.0.6 / valkey 8.1.10, digest-pinned) · `src/snapshot.rs` + `src/compile.rs` · `ci/catalog-snapshot.sh` · `ci/check-catalog-snapshots.sh` · 38 unit + 19 integration tests | 固定镜像抓取 → blake3 指纹 → 编译成 `CommandSpec`；577 / 379 条命令全部分类，仅 EVAL/EVALSHA/EXEC/FCALL 刻意留 `unknown`（其效果等于被要求执行的内容）；override 表只能加限制，`unknown` 仅在标注 `classified` 时清除；双分支 merge 报告 Redis-only(HEXPIRE/VADD/模块) 与 Valkey-only(COMMANDLOG/CLIENT CAPA/SCRIPT SHOW)，效果冲突为 0；CI 门禁要求快照能从 pinned image 逐字节重现 |
| V-F02 | PASS | `crates/pr-intelligence/src/analyser.rs` · 11 tests（含 3 个 proptest） | 宽容分析器与权威 tokenizer 的 argv 等价性 property；span 恒在范围内且 analyse 不 panic；按 span 替换后参数个数不变、替换落在正确位置（ASSIST-081） |
| V-F03 | PASS | `crates/pr-repl/` · 26 tests | 权威 tokenizer 对齐 `sdssplitargs`；全 256 字节 quote 往返；`:` 本地命令独立 grammar（重复选项报错、零 shell 展开） |
| V-F04 | PASS | `crates/pr-catalog/tests/grammar_table.rs` · 19 tests（§11.7 十一行逐行）· `src/spec.rs` grammar walker | 真实语法树 walker 而非位置表：子命令非 key、HSET field/value 交替且 value 位不枚举、ZADD score/member 不互换（带选项时仍正确）、SET/ZADD 互斥组按版本过滤且冲突可解释不暗改、XADD 嵌套 choice、EVAL numkeys（解析失败时取 0 个 key）、XREAD STREAMS 成对切分、key 名为 NX/GET 由位置决定、ZRANGE BYSCORE/BYLEX 模式、二进制参数无损、模块命令分类 |
| V-F05 | PASS | `crates/pr-intelligence/src/broker.rs` · 11 tests | 到达时校验 revision+scope+序号；过期结果丢弃且**不显示**；焦点跟 `CandidateId` 不跟索引（ASSIST-028）；候选消失时焦点不静默落到别处 |
| V-F06 | PASS | `crates/pr-intelligence/src/scope.rs` · 14 tests | 六元组 scope（profile UUID + 服务身份 + DB + auth/policy/**topology** epoch）；逐项验证任一变化都不泄漏；field 绑定 parent key；schema hint 不主张存在性 |
| V-F07 | IN-PROGRESS | — | |
| V-F08 | IN-PROGRESS | — | |
| V-F09 | IN-PROGRESS | — | |
| V-F10 | PASS | `crates/pr-intelligence/tests/zero_send.rs` · 11 tests | transport spy 计**尝试次数**；分析/候选/接受/焦点/可提交性检查/Guide 组装/换主题/丢弃过期结果/scope 失效，以及完整离线编辑会话 —— 全部 0 条业务命令 |

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
| V-H06 | PASS | `crates/pr-repl/src/history.rs` · 19 tests | rusqlite bundled + WAL 多连接共享 + `user_version` schema 版本（更新的文件拒绝打开）；0600 权限；scope 按 profile/identity/db 隔离 |

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
| 2026-09-16 | V-E05 / V-B04 → PASS（generic RESP 渲染、outcome 组合矩阵）；327 tests 全绿 |
| 2026-09-16 | V-H06 / V-D06 → PASS（history 存储与按环境脱敏）；346 tests 全绿 |
| 2026-09-16 | V-E03 / V-D07 → PASS（有界 result store、catalog 权威性）；370 tests 全绿 |
| 2026-09-16 | V-B03 → PASS（`--pipe` 逐帧策略，BLOCKER #1 验证完成）；384 tests 全绿 |
| 2026-09-16 | V-F02 → PASS（双解析器等价性 property）；395 tests 全绿 |
| 2026-09-16 | V-F05 / V-F06 → PASS（异步 broker、observation scope 隔离）；420 tests 全绿 |
| 2026-09-16 | V-F10 / V-D05 → PASS（零发送不变量、SafeText 显示边界）；441 tests 全绿 |
| 2026-09-16 | V-F01 / V-F04 → PASS（catalog 编译流水线 + §11.7 语法全表）；walker 抓到两个真 bug（Choice 选不到无关键字分支、numkeys 解析失败仍吞 key）；`deny_unknown_fields` 抓到快照残留字段；新增 `catalog` CI 门禁；488 tests 全绿 |
| 2026-09-16 | V-C02 → PASS（单一 terminal owner）；引入 crossterm（ADR-022 指定）作 raw mode 与事件源；真实 PTY 上跑 ASSIST-062 / UX-12 / 场景 H；524 tests 全绿 |
