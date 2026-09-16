# Penguin Redis — 分阶段执行计划
## Phase 0 前置验证版 · v1.0

> **依据：`penguin-redis-final-plan-v2.1.md`（下称 v2.1）**  
> **日期：2026-09-16**  
> **原则：Phase 0 把 Phase 1–8 的全部阻塞点先验证完；Phase 0 不全绿，Phase 1 不开工。标准零下调，测试零省略，成本与时间不是输入。**

---

## 0 · 本计划的规则

| # | 规则 |
|---|---|
| G1 | **对应关系。** Phase 0 = v2.1 §36 的 E0，扩展为「全量前置验证」；Phase 1–8 = E1–E8（Phase 2A = E2A）。范围与 v2.1 完全一致，一条能力都不少。 |
| G2 | **硬门禁。** Phase N 的代码在 Phase N-1 出口门禁未全绿前不得合入 `main`。分支上可以探索，`main` 只接收已过门禁的阶段。 |
| G3 | **Phase 0 的定义。** 任何会在 Phase 1–8 造成阻塞、返工或设计推翻的不确定性——库能力、平台能力、预算是否成立、两章是否矛盾、协议边界——都必须在 Phase 0 得到两种结论之一：`PASS`（已验证可行，有测试证明）或 `FALLBACK-ADOPTED`（已用 ADR 采纳回退方案，回退方案本身也有测试证明）。不存在第三种状态。 |
| G4 | **不降低标准。** v2.1 的每个数字、每条 ASSIST / 验收案例、每个「永远不接受」原样保留。Phase 0 的结论只能导致「换方案」，不能导致「删要求」。若验证证明某条要求物理上不可实现（例如终端不报告 IME 事件），处理方式是 ADR 记录 + 改写为**等价可实现**的契约（v2.1 §14.6 对 Unicode 宽度就是这样做的），不是删除。 |
| G5 | **完整测试。** 每个验证项必须产出可重复运行的测试 / fixture / benchmark，进入仓库并在 CI 运行。「手工试过了」不是通过。系统 IME、屏幕阅读器、真实终端字体等无法自动化的项，产出带平台版本、录屏与操作步骤的人工记录模板，并同样列入发布门禁（v2.1 §32.6）。 |
| G6 | **成本与时间不是输入。** 本计划不含工时估算，Phase 的先后只由依赖关系决定。 |
| G7 | **状态机。** 每条能力、每个命令都按 v2.1 §36.4 记录 `designed → implemented → automated-tested → platform-verified → released`；每个命令分别记录 `execution / render / assistance / guide` 四个状态。 |

---

## 1 · Phase 总览

| Phase | 对应 v2.1 Epic | 一句话 | 出口门禁摘要 |
|---|---|---|---|
| **0** | E0 Contracts（扩展） | 全量前置验证：harness、spike、契约冻结、ADR、CI 矩阵、benchmark 基线 | §2.3 全部 V 项 PASS / FALLBACK-ADOPTED；CI 3 OS 全绿 |
| 1 | E1 Native kernel | byte request、RESP2/3 增量 decoder、session、outcome、TLS | fuzz 无 panic；1 GiB 流式；四维度 outcome 映射 100% |
| 2 | E2 Daily CLI + Assistance | `prc @dev`、凭证、Hash/JSON 表格、自动建议、签名、F1/F2、history、one-shot | v2.1 §36 列出的 E2 必过 ASSIST 子集 + PIPE-01~07 + §37.2 纵向链路可演示 |
| 2A | E2A Deep intelligence | grammar/overlay、key/field scope、Guide、错误说明、Learn、显式发现 | ASSIST-001~088 逐项有证据 |
| 3 | E3 Production topology | Cluster/Sentinel、TrustIdentity、SSH、ACL/审批、取消恢复 | NET-01~07、STATE-01~08、SEC-01~11 全过 |
| 4 | E4 Deep TUI | browser、inspector、editor、diff、virtualization、任务面板 | PTY/soak/焦点/编辑冲突全过；与 CLI 共用 intelligence |
| 5 | E5 Operational suite | diagnostics、Streams、Pub/Sub、archive、bulk、Lua、`--pipe`、官方特殊模式 | 每个特殊模式独立契约测试；PIPE-08 |
| 6 | E6 Automation | recipes、签名 plan、structured report、MCP/AI | 所有入口同一 policy；注入与权限测试 |
| 7 | E7 Product hardening | 全平台分发、文档、支持矩阵、benchmark、升级/回滚 | Release evidence bundle 完整；无 P0 |
| 8 | E8 Studio integration | core SDK、schema migration、shared fixtures | Studio 未安装时 CLI 零退化 |

依赖关系沿用 v2.1 §36.1；唯一变化是 **Phase 0 成为所有 Phase 的前置**，而不只是 E1 的前置。

---

## 2 · Phase 0 — 全量前置验证

### 2.1 Phase 0 的产出物类型

| 类型 | 落点 | 说明 |
|---|---|---|
| ADR | `docs/adr/` | v2.1 §37.1 的 ADR-001~030 + 每个 spike 结论一条 |
| 冻结契约 | `crates/*/src/contract.rs` + `docs/contracts/` | 核心类型骨架、doc test、schema 版本号 |
| Spike 报告 | `docs/spikes/SPIKE-xxx.md` | 采用 / 拒绝 / 回退 + 反例 + benchmark + fixture 链接 |
| Fixture corpus | `fixtures/**` | 协议、renderer、assistance、catalog、hostile-input、quoting |
| 测试 harness | `tests/pty/`、`tests/differential/`、`tests/fault/`、`xtask/` | PTY、differential、synthetic RESP server、fault injection |
| CI 基础设施 | `.github/` 或等价 | 多版本 Redis/Valkey、Cluster、Sentinel、TLS、3 OS、soak runner |
| Benchmark 基线 | `benches/baseline/` | 骨架进程的启动、RSS、分配基线 |
| 人工记录模板 | `docs/manual-verification/` | IME、屏幕阅读器、真实终端宽度 |

### 2.2 验证项格式

每个验证项（V 项）都必须填满下列字段，缺一项不得标 PASS：

```text
ID          V-<Track><NN>
验证什么     一句话
排雷对象     受益的 Phase（若不在 Phase 0 验证，会在哪个 Phase 爆）
v2.1 依据    章节 / 修订编号 R / 验收编号
方法         怎么验
通过标准     可判定的条件（数字、fixture 编号、状态）
产出         进入仓库的测试 / fixture / 报告路径
不通过时     回退方案 + 记录到哪条 ADR
状态         PASS | FALLBACK-ADOPTED(ADR-xxx) | IN-PROGRESS
```

### 2.3 验证项清单

按轨道（Track）组织。Track A 是其他所有轨道的前提。

#### Track A · 基础设施与 harness

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-A01 | **PTY harness**：macOS/Linux PTY + Windows ConPTY 统一驱动；resize、粘贴（含 bracketed）、颜色能力声明、录制回放 | 2、2A、4、7 | §32.1 PTY E2E、§32.4、R20 | 用 harness 在 3 平台跑 10 个基线场景（回显、resize、Ctrl+C、alternate screen 进出、粘贴、颜色 reset） | 3 平台同一 fixture 全过；录制可回放 diff | `tests/pty/harness/`、`fixtures/pty/baseline/` | 换 PTY 驱动库；**无 PTY harness 不得进入 Phase 2** |
| V-A02 | **Differential harness** vs 固定 `redis-cli` baseline：两套独立同初态 Redis，四层比对（argv bytes / response / 最终状态 / 输出+exit） | 1、2、5、7 | §32.2、§20.4 | 用骨架 `prc`（只做透传）跑 CMD-01~05 | 5 条案例产出四层比对报告；写命令不共用实例 | `tests/differential/`、附录 B.2 模板实例化 | 修 harness；不允许用「同实例先后执行」替代 |
| V-A03 | **Synthetic RESP server**：可编程发送任意 RESP2/3 帧（streamed string/aggregate、push、attribute、畸形、1 GiB blob、半包、乱序 push） | 1、5 | §19.3、§32.3、R16、R17 | 生成 §32.3 列出的全部 fuzz 类别 | 每类至少 20 个 fixture；server 可按脚本延迟/断开 | `xtask/resp-server/`、`fixtures/protocol/` | 无回退；这是 Phase 1 的必需工具 |
| V-A04 | **Fault injection**：断网、半包、丢响应、超时、DNS/TLS 失败、磁盘满、只读 FS、权限撤销 可脚本化 | 1、3、5、7 | §32.1 Fault injection、§30.5 | 对骨架进程注入每类故障 | 每类故障有可重复脚本；进程不 panic | `tests/fault/` | 无回退 |
| V-A05 | **多目标 CI 矩阵**：Redis 7.2/7.4/8.0、Valkey 8.0/8.1、Cluster（6 节点）、Sentinel（3+1）、TLS（自签 CA + client cert）、macOS/Linux/Windows runner、8h soak runner、24h 发布 soak runner | 全部 | §20.4 矩阵、§32.1、§34.2 | 用 hello-world crate 跑满矩阵 | 矩阵全绿；每个 target 的 image digest 已写入 `compatibility/manifest.toml` | CI 配置、`compatibility/manifest.toml` | 缺哪个 target 就不许声称支持哪个；不允许用 `latest` |
| V-A06 | **资源测量工具**：allocator 统计 hook、RSS 采样、任务计数暴露、只读 mmap 单独计量 | 2、4、7 + 所有预算项 | §12.5、§24.7、§16.4、R08、R35 | 在骨架进程上产出「开/关某模块」的 RSS 差报告 | 报告可复现（±5%）；mmap 与堆分开 | `xtask/measure/`、`benches/baseline/` | 无回退 |

#### Track B · 协议与执行内核

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-B01 | **SPIKE-002 redis-rs 低层 API**：是否保留 RESP frame 类型、增量读、push 分流、取消后状态、重定向控制 | 1 | §31.2、§37.3 | 对 V-A03 的全部帧类别用 redis-rs 低层 API 读取，比对保真 | §31.2 清单全部满足 | `docs/spikes/SPIKE-002.md` | **ADR 采纳 `pr-protocol` 自研 codec 为唯一 kernel 边界**；redis-rs 降为互操作测试对象 |
| V-B02 | **增量 RESP decoder 原型**（无论 B01 结论都要做，§19.3 的 streamed 与有界解码不依赖第三方） | 1、5 | §19.3、§24.2、R16 | 对 V-A03 全部类型解码；1 GiB synthetic blob；半包/畸形 fuzz | 类型全覆盖含 streamed；1 GiB 峰值内存 ≤ 块预算；fuzz 10⁶ 次无 panic/无界分配 | `crates/pr-protocol/`、`fixtures/protocol/` | 无回退 |
| V-B03 | **`--pipe` 逐帧路径**：stdin RESP → `CommandRequest` → policy → send → per-frame outcome；背压队列；畸形 fail-closed | 5、3 | §18.5、R01、SEC-09、PIPE-08 | prod-deny profile 下喂含写帧的流；喂截断帧 | 在第一条写帧停止，退出码 5；截断帧退出码 2 并报帧序号；无静默跳过 | `tests/pipe/`、`fixtures/hostile-input/pipe/` | 无回退；这是 BLOCKER 修复的验证 |
| V-B04 | **`ExecutionOutcome` 四维度模型 + 退出码映射表** | 1、2 | §19.2、§18.6、§22.3、R27、R30、R42 | 用 V-A03 制造每种 delivery×reply×effects 组合 | 映射表 fixture 100%；Ctrl+C vs unknown 优先级按 §22.3 | `crates/pr-core/src/outcome.rs`、`fixtures/outcome/` | 无回退 |
| V-B05 | **会话状态机**：CLIENT REPLY OFF/SKIP、MULTI/EXEC/WATCH、HELLO/AUTH/SELECT/RESET；断线后不重放 | 1、3 | §22.4~22.6、STATE-01~08 | synthetic server 上跑状态转换与断线 | STATE-01~08 全过 | `tests/session-state/` | 无回退 |
| V-B06 | **push 帧处理**：one-shot `--json` 不进 stdout；`--show-pushes`；ndjson 事件；`--output resp` 再编码 | 2、5 | §18.1、R17、R40 | synthetic server 在回复前后插 push | stdout 只有回复；stderr 计数；ndjson 顺序正确 | `tests/output/push/` | 无回退 |
| V-B07 | **CLI 参数边界 parser**：`prc [opts] [@profile] [opts] [--] CMD …`；profile × flag 允许/冲突矩阵 | 2 | §3.2、§3.3、R26、CMD-01 | 表驱动测试 | §3.3 矩阵每格一个用例；`SET k '--raw'` 原样 | `crates/prc/tests/args.rs` | 无回退 |

#### Track C · 终端与编辑器

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-C01 | **SPIKE-001 Reedline 作编辑引擎**：`LineBuffer` 接受外部 `TextEdit` 不丢字节；撤销栈把「接受候选」当单事务；与自绘菜单组合 | 2、2A、4 | §14.5、§31.1、R03、R28 | 在 V-A01 上跑 ASSIST-020~028、062~064、087 | 全过 | `docs/spikes/SPIKE-001.md`、`tests/editor-state/` | **ADR-026 回退：`pr-repl` 自有 LineBuffer**（grapheme-aware，复用 Reedline history 格式），同一套测试必须通过 |
| V-C02 | **单一 terminal owner 原型**：`pr-terminal` 同时驱动 REPL 编辑、自绘下拉、异步消息排队、TUI alternate screen 切换 | 2、4 | §14.4、ADR-022 | Pub/Sub 消息涌入 + 打字 + 菜单 + 切 TUI 再回 | ASSIST-062、UX-12、场景 H 过；无 stdin/stdout 竞争 | `crates/pr-terminal/`、`tests/pty/owner/` | 无回退 |
| V-C03 | **bracketed paste 矩阵**：macOS Terminal、iTerm2、kitty、Alacritty、WezTerm、Windows Terminal、conhost、tmux、SSH 中的 `CSI ?2004` 支持；无支持时 10 ms 时序启发式的漏检/误判 | 2 | §14.1、R05、ASSIST-082、UX-11 | 每个终端粘贴 3 行写命令 | 有支持终端 100% 进 staging；无支持终端启发式在 fixture 上 0 漏检；误判率记录 | `tests/pty/paste/`、人工记录 | 若某终端两者皆失败 → 该终端标 `plain-only`，在支持矩阵明示 |
| V-C04 | **Unicode 宽度矩阵**：ambiguous / ZWJ / 组合字符在上述终端的实测宽度；`CSI 6 n` 探测可行性；「每格重置光标」降级绘制 | 2、4 | §14.6、R33、UX-08、ASSIST-018/067 | 固定策略 + 仿真终端 + 真实终端人工记录 | 仿真下 UX-08 过；降级绘制在宽度判错时框线仍对齐 | `tests/render/width/`、人工记录 | 无回退（策略已是可实现契约） |
| V-C05 | **按键可达性**：F1–F6、Ctrl+R/P/N、Ctrl+Space 在各终端与 IME（macOS 拼音、Windows IME、fcitx）下是否被拦截；文字入口回退 | 2 | §14.2、ASSIST-068 | 每终端 + IME 组合逐键记录 | 每个功能至少一条文字入口在所有组合可用 | 人工记录 + `tests/pty/keys/` | 被拦截的键在该平台默认重映射并写入文档 |
| V-C06 | **颜色层级与 reset**：24bit→256→16→mono 量化后相邻语义可分辨；表头背景不泄漏；管道无 escape；Light/High-contrast token 对比度 | 2 | §7.1~7.3、R25、UX-04/06、PIPE-01 | golden tests × 4 色阶 × 3 主题 | 全过；WCAG 4.5:1 自动校验过 | `fixtures/renderer/colour/` | 无回退 |
| V-C07 | **SPIKE-003 Windows ConPTY 专项**：V-A01 全部场景 + 二进制 stdio + `SetConsoleCtrlHandler` + Ctrl+Break | 2、7 | §35.1、R20、WIN-01/02 | Windows runner 上跑 | WIN-01、WIN-02 过 | `docs/spikes/SPIKE-003.md` | 若 legacy conhost 不达 → conhost 标 `plain-only`；Windows Terminal 必须过 |

#### Track D · 安全与凭证

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-D01 | **`keyring` 平台矩阵**：macOS `apple-native`、Windows `windows-native`、Linux Secret Service + keyutils 回退；headless fail-closed；解锁/授权提示行为 | 2 | §4.3、§31.1、R23 | 三平台存/取/删 + headless 容器 | 三平台通过；headless 无明文落盘且给出明确错误 | `tests/credentials/` | 某平台缺 store → 该平台只允许 askpass/会话内秘密，并在支持矩阵明示 |
| V-D02 | **跨资源提交 journal + reconciliation**：在 §4.2 六步的每一步注入崩溃 | 2 | §4.2、§30.2、R07 | fault injection 崩溃矩阵 | 每种崩溃后启动 reconciliation 收敛；孤儿/悬空全部被报告 | `tests/credentials/journal/` | 无回退 |
| V-D03 | **`ApprovalToken` 字节哈希**：「同摘要不同字节」对照集；epoch/TrustIdentity 变化失效 | 3、5、6 | §23.2、R02、SEC-10 | 表驱动 | SEC-10 过 | `crates/pr-security/tests/approval.rs` | 无回退；BLOCKER 修复验证 |
| V-D04 | **`TrustIdentity` 绑定**：证书轮换过渡窗、failover 到已登记节点不重绑、边界外重定向确认 | 3 | §21.3、R14、SEC-05 扩展 | Cluster/Sentinel fixture + 证书轮换脚本 | SEC-05 过；边界外重定向必停 | `tests/topology/trust/` | 无回退 |
| V-D05 | **SafeText 类型边界**：renderer/TUI widget 签名只接受 `SafeText`；含 OSC/CSI 的 key/field/channel/`CLIENT LIST` 字段在所有视图不触发副作用；`--bytes`/`--output resp` 直写 TTY 警告 | 2、4 | §23.5、§12.9、R34、R47、SEC-05 | 编译期断言 + PTY 注入 | 类型检查阻止 `&str` 进 renderer；PTY 上无标题/超链接/剪贴板副作用 | `crates/pr-render/`、`tests/security/terminal-injection/` | 无回退 |
| V-D06 | **秘密路径审计**：URI parse error、TLS error、`Debug` 派生、trace、history、clipboard、crash report、`--trace-wire` 十条路径注入含密码输入 | 2、7 | §23.4、§19.5、R24、R41、SEC-01 | 每路径注入已知 token 后全量 grep | 0 泄漏 | `tests/security/secret-path/` | 无回退 |
| V-D07 | **本地 policy 权威性**：伪造 `COMMAND`/`COMMAND DOCS` 把写命令标 readonly | 2A、3 | §11.2、R19、SEC-11、ASSIST-086 | synthetic server 返回伪造 metadata | 本地 effects/策略不变；UI 标 `server reports` | `tests/catalog/precedence/` | 无回退 |
| V-D08 | **MCP stdio bootstrap + 策略批准边界**：stdout 只有 JSON-RPC；策略型批准只覆盖只读；`execute_approved_plan` 校验 `plan_hash` | 6 | §29.4、R12、R46 | PTY-less 集成测试 | stdout 无杂字节；写类请求无人签令牌被拒 | `crates/pr-mcp/tests/` | 无回退 |
| V-D09 | **L2 插件外部进程隔离**：超时 kill、输出截断、崩溃/OOM 不影响主进程 | 7 | §30.4、R45 | 故意崩溃的插件 | 主进程存活，退回 generic view | `tests/plugins/` | 无回退（L1 声明式不需验证隔离） |

#### Track E · 数据模型

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-E01 | **SPIKE-004 `pr-json` occurrence DOM**：重复成员、大数、词法保留、`$.a#2`、增量重写；16 MiB 单值解析/格式化时间与内存 | 2、4、5 | §8.3、§8.5、R06、DATA-02/03/04、JSON-05 | property test + 16 MiB fixture | 全过；16 MiB 解析 + 格式化在 §24.3 时间预算内、峰值 ≤ 2× 输入 | `crates/pr-json/`、`docs/spikes/SPIKE-004.md` | 无回退（已确认 serde_json 不可用） |
| V-E02 | **TableModel 流式宽度算法**：采样冻结列宽、后续换行、极窄纵向块、每 field 横线、JSON cell 内不加分隔线 | 2、4 | §6.1~6.3、UX-03/04/05/07 | golden × 5 宽度 × 3 色阶 | 全过 | `fixtures/renderer/table/` | 无回退 |
| V-E03 | **有界 result store + spool**：64 MiB/16 MiB 淘汰；显式 spool 私有权限、symlink/路径遍历防护、加密可选；`not retained` 标记；§8.2 阈值顺序 | 2、4 | §24.3、§24.4、§30.5、R31、UX-10、LIFE-02 | 超预算结果 + 崩溃后重启 | 淘汰正确；spool 权限正确；已知长度首字节前询问、未知长度不中途询问 | `crates/pr-results/tests/` | 无回退 |
| V-E04 | **JSON projection v1** vs `redis-cli --json` 差异 | 2 | §18.3、R17、PIPE-02 | §18.3 表逐行 fixture | 每行一个用例过；差异写入 manifest | `tests/output/json-projection/` | 无回退 |
| V-E05 | **renderer generic fallback**：未知命令/模块结果不 panic 不重发 | 2、5 | §9.5、CMD-10/12、SEC-07 | synthetic 未知结构 | 全过 | `fixtures/renderer/generic/` | 无回退 |

#### Track F · 智能输入引擎

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-F01 | **catalog 编译管线**：pinned snapshot → `CommandSpec`；Redis/Valkey 双分支；`unsupported grammar node` 标记；签名/校验和 | 2、2A | §11.3、§11.4、R29 | 编译全部 baseline 命令 | 0 编译错；Redis vs Valkey 差异清单生成 | `crates/pr-catalog/`、`fixtures/catalog/` | 无回退 |
| V-F02 | **双解析器等价性** property test | 2、2A | §11.5、R11、ASSIST-081 | 随机字节缓冲区 10⁶ 次 | span ↔ argv 一一对应；未闭合引号区间正确 | `crates/pr-intelligence/tests/equivalence.rs` | 无回退 |
| V-F03 | **quoting corpus** vs 固定 `redis-cli` tag 的 `sdssplitargs`；本地 `:` 命令 tokenizer | 2 | §11.5、§3.5、R04、R18 | 从 redis-cli 源码生成对照 corpus | 100% 一致；`:` 命令 fixture 过 | `fixtures/catalog/quoting/`、`tests/local-commands/` | 无回退 |
| V-F04 | **语法复杂度 §11.7 全表**：SET/ZADD/XADD/EVAL/XREAD/子命令/keyword 同形 key 的 signature 与候选 | 2A | §11.7、ASSIST-004~015 | 每行 fixture | 全过 | `fixtures/assistance/grammar/` | 无回退 |
| V-F05 | **异步 broker**：revision/epoch 隔离、迟到丢弃、`CandidateId` 稳定、粘贴期间暂停 | 2、2A | §12.8、ASSIST-026~028、072 | 模型测试（乱序事件序列） | 全过 | `tests/assistance-network/async/` | 无回退 |
| V-F06 | **`ObservationScope` 隔离**含 topology epoch | 2A、3 | §12.6、R13、ASSIST-029~033、088 | 多 profile/identity/DB/拓扑并发 fixture | 0 泄漏 | `tests/assistance-network/scope/` | 无回退 |
| V-F07 | **显式发现在百万 key fixture**：50 SCAN / 10 s 返回非空；prefix vs glob 模式；NOPERM 冷却；空页非零 cursor | 2A | §12.4、§12.5、R09、R32、ASSIST-041~045、084、085 | 隔离 Redis 灌 10⁶ key | 稀疏前缀有结果并报告完成度；`player:[` 两模式字节正确 | `tests/assistance-network/discovery/` | 若默认预算在 10⁶ key 上仍空 → 调高默认值（不是删功能）并记 ADR |
| V-F08 | **用途搜索双集合**：canonical ≥95%、held-out ≥80%/90%；集合公开 | 2A | §16.4、R10、ASSIST-050/051 | 独立人员撰写 held-out 集 | 两行指标达标 | `fixtures/assistance/find/{canonical,heldout}/` | 未达标 → 扩词表后 held-out 集**换新**（旧集作废，防止泄漏） |
| V-F09 | **12 MiB 堆增量实测**：catalog 加载后开/关提示 RSS 差 | 2、7 | §12.5、§16.4、R08、R35 | V-A06 工具 | ≤ 12 MiB；mmap 单独报告 | `benches/assistance/` | 超出 → 先定位；预算调整必须带证据与 ADR |
| V-F10 | **零发送不变量**：断网下 F1/F2/F3/Guide/`:view`/`:copy`/主题切换/接受候选 全路径 | 2A | §14.10、ASSIST-020、037、050~056、070 | transport spy | 0 条业务命令 | `tests/assistance-network/zero-send/` | 无回退 |

#### Track G · 连接拓扑

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-G01 | **Cluster 路由**：hash tag、MOVED/ASK 同连接、重定向预算、CROSSSLOT 不拆分、部分节点不可达标 partial | 3 | §21.2、R43、NET-03/04/05 | 6 节点 Cluster + 重分片脚本 | NET-03/04/05 过 | `tests/topology/cluster/` | 无回退 |
| V-G02 | **Sentinel failover**：重新发现、未知写不重放、观察上下文清除 | 3 | §21.4、NET-06 | 强制 failover | NET-06 过 | `tests/topology/sentinel/` | 无回退 |
| V-G03 | **SSH per-node 与 SOCKS5**：OpenSSH `-L`/`-D` 程序化控制、known-host、退出清理、TLS 校验内部名、Cluster-over-SSH | 3 | §21.5、R15、NET-07 | bastion 容器 + Cluster | 两模式都能跟随 MOVED；NET-07 过 | `tests/topology/ssh/` | 某模式在某平台不可用 → 该平台该模式标 unsupported，另一模式必须可用 |
| V-G04 | **TLS 矩阵**：错名/过期/未知 CA/client cert/SNI 独立/放松类 flag 拒绝 | 3 | §21.1、NET-01 | 自签 CA 生成各类证书 | NET-01 过；无静默 insecure | `tests/topology/tls/` | 无回退 |
| V-G05 | **Windows 凭证 + `LockFileEx` + ACL**；共享文件 advisory lock 三平台 | 2、7 | §30.2、§12.10、R20、R39、WIN-03、LIFE-03 | 多进程并发写 | WIN-03、LIFE-03 过 | `tests/config/locking/` | 无回退 |

#### Track H · 资源预算与性能基线

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-H01 | **启动与空闲基线**：全部依赖链接后的骨架进程 `--help` p95、REPL 空闲 RSS、TUI 空闲 RSS | 2、4、7 | §24.6 | V-A06 工具，固定机器 | `--help` ≤100 ms；REPL ≤30 MiB；TUI ≤60 MiB —— **确认依赖本身没吃掉预算** | `benches/baseline/` | 超出 → 换/裁依赖 feature；预算不变 |
| V-H02 | **1 GiB synthetic blob 流式 + 真 Redis 512 MB 样本** | 1、5 | §24.7、PERF-01 | V-A03 + 真 Redis | 峰值 RSS 与块预算相符；两类来源分开报告 | `benches/protocol/` | 无回退 |
| V-H03 | **订阅风暴 ring buffer** | 5 | §24.3、PERF-02 | 每秒 10⁵ 消息 | 有界；淘汰计数真实 | `benches/pubsub/` | 无回退 |
| V-H04 | **任务监督 leak detection**：`TaskScope` drop 后任务计数回零；TUI 开关 1000 次 | 4 | §31.1、§31.4、R21、PERF-03 | V-A06 任务计数 | 回零；无增长 | `tests/soak/tasks/` | 无回退 |
| V-H05 | **8h soak runner 跑通**（骨架功能） | 7 | §32.1 Soak、PERF-04 | runner 上跑 8h | 指标采集完整；无线性增长 | CI soak job | 无回退 |
| V-H06 | **rusqlite WAL 多进程并发写 + 0600 + `user_version` 迁移** | 2 | §31.1、R22、R24 | 4 进程并发写 history | 无丢更新；权限正确；迁移前向 | `tests/history/` | 无回退 |

#### Track I · 兼容矩阵、catalog 与许可

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-I01 | **`compatibility/manifest.toml` 冻结**：§20.4 初始矩阵每项 image digest / tag 填入 | 全部 | §20.4、R29 | 拉取并 pin | 无 `latest`；每项可复现拉取 | `compatibility/manifest.toml` | 无回退 |
| V-I02 | **Valkey vs Redis `COMMAND DOCS` 差异清单** | 2、2A | §20.3、R29 | 双分支 introspection diff | 清单生成并进入 catalog fixture | `fixtures/catalog/valkey-diff/` | 无回退 |
| V-I03 | **catalog 再分发许可结论**（CAT-001 / ADR-023）：每个 snapshot 三选一（可打包 / 用户运行时下载 / 本仓独立撰写） | 2、7 | §35.5、R29 | 逐来源审查 | 无空结论；`prc` 自身许可已定 | `docs/adr/ADR-023.md`、`pr-catalog/LICENSES.md` | 不可再分发的部分 → 本仓独立撰写并以 fixture 对照真实服务器 |
| V-I04 | **`redis-cli` baseline 特殊模式 inventory**：从 help/source 生成 §28.4 的命令/flag/退出码清单 | 5 | §28.4、R36 | 解析固定 tag 源码 | 清单与 §28.4 契约表逐项对应 | `compatibility/redis-cli-modes.toml` | 无回退 |

#### Track J · 契约冻结与 ADR

| ID | 验证什么 | 排雷对象 | v2.1 依据 | 方法 | 通过标准 | 产出 | 不通过时 |
|---|---|---|---|---|---|---|---|
| V-J01 | **ADR-001~030 全部写入并评审** | 全部 | §37.1 | 逐条评审 | 30 条状态 `accepted` | `docs/adr/` | 有争议 → 在 Phase 0 内决出，不带入 Phase 1 |
| V-J02 | **冻结接口骨架**：`CommandRequest`、`ExecutionOutcome`、`ResultRecord`、`CompletionRequest`、`CompletionCandidate`、`AssistanceSnapshot`、`TrustIdentity`、`ApprovalToken`、`LocalCommandSpec`、`JsonNode`、`SafeText`、`TaskScope` | 全部 | §11.11、§19.2、§21.3、§23.2、§3.5、§8.3、§12.9、§31.4 | Rust 类型 + doc test + schema 版本号 | 编译通过；每个类型有 doc test | `crates/*/src/contract.rs` | 无回退 |
| V-J03 | **威胁模型**：每条信任边界对应至少一个 SEC 用例 | 3、7 | §23、§32.1 Security | 边界 → 用例映射表 | 无未覆盖边界 | `docs/threat-model/` | 无回退 |
| V-J04 | **Phase 0 新增 ADR**：每个 spike 一条结论（SPIKE-001~004 + V-C03/C04/C05 的平台结论） | 全部 | §37.3 | — | 每个 spike 有 ADR | `docs/adr/` | 无回退 |

### 2.4 Phase 0 出口门禁

全部满足才算通过，任一不满足则 Phase 1 不得合入 `main`：

- [ ] §2.3 全部 V 项状态 ∈ {`PASS`, `FALLBACK-ADOPTED(ADR-xxx)`}；**不存在 `IN-PROGRESS`、`DEFERRED`、`SKIPPED`**。
- [ ] V-A05 CI 矩阵在 macOS / Linux / Windows 三 runner 上全绿，含 Cluster、Sentinel、TLS 三种拓扑。
- [ ] 每个 harness（PTY、differential、synthetic server、fault）至少已跑通 v2.1 §33 中属于自己类别的一条验收案例。
- [ ] `benches/baseline/` 基线报告存档，含硬件/OS/commit/依赖锁。
- [ ] 人工记录模板建立，且 V-C03/C04/C05 已有首批真实终端记录。
- [ ] `compatibility/manifest.toml` 无 `latest`。
- [ ] ADR-001~030 + spike ADR 全部 `accepted`。
- [ ] **Phase 0 报告**：每个 V 项一行 = 状态 + 证据链接（测试路径 / 报告路径 / ADR 编号）。

---

## 3 · Phase 1 — Native kernel（E1）

**范围（v2.1 §36）：** byte request、RESP2/3、session、bounded decode、TLS、outcomes。

**本阶段的 issue / blocker 与它们在 Phase 0 的验证：**

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| redis-rs 能否作 kernel 边界 | V-B01 → 结论已定（采用或 ADR 回退自研） | 按结论实现 `pr-transport` / `pr-protocol` |
| streamed string/aggregate、attribute、push 分流 | V-B02、V-A03 | 生产级 decoder + 全部 fuzz corpus 常驻 CI |
| 断线后「看起来失败实际执行两次」 | V-B04、V-B05 | `UnknownAfterSend` 进入所有路径；差异测试 STATE-01/02 |
| 1 GiB 流式峰值内存 | V-H02 | 常驻 benchmark 门禁 |
| 任务泄漏 | V-H04 | `TaskScope` 用于所有会话 |
| 秘密进 trace | V-D06 | 字节数而非内容；`--trace-wire` 单独路径 |
| 与官方 CLI 行为漂移 | V-A02 | differential 常驻 CI；每条 kernel 行为差异写入 manifest |
| 半包/丢响应/超时/DNS/TLS 失败路径 | V-A04 | 每类故障对应一个 STATE/NET 用例 |
| 架构决策未定就开工 | V-J01、V-J04 | 所有 kernel 代码引用对应 ADR 编号 |

**本阶段新增测试：** §32.3 全部 fuzz 类别常驻；DATA-01；STATE-01~04；PERF-01；SEC-01 的 kernel 部分。

**出口门禁：** 上述测试全绿；`pr-protocol` fuzz 24h 无 panic；四维度 outcome 映射 fixture 100%；`ExecutionOutcome` 在 typed output 中完整暴露。

---

## 4 · Phase 2 — Daily CLI + Assistance（E2）

**范围：** `prc @dev`、Keychain、Hash/JSON table、colour、自动建议、签名、F1/F2、history、one-shot/raw/JSON。

**本阶段的 issue / blocker 与它们在 Phase 0 的验证：**

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| Reedline 撑不起自动下拉 | V-C01（含回退 ADR-026） | 按结论集成；ASSIST-020~028 常驻 |
| REPL / 菜单 / 异步消息抢终端 | V-C02 | `pr-terminal` 生产化 |
| 粘贴多行被逐行执行 | V-C03 | paste-staging UI |
| 二进制参数无法进编辑器 | V-F03、V-B02 | §3.2 六条入口全部实现 |
| Keychain 与 profile 崩溃不一致 | V-D01、V-D02 | journal 生产化；`--add`/`--edit` 向导 |
| 无损 JSON 用错模型 | V-E01 | `pr-json` 接入 renderer 与 `:copy` |
| 表格宽度、JSON cell、颜色 | V-E02、V-C06 | 全部 §9.1 专用 renderer |
| 大结果何时询问 | V-E03 | §8.2 顺序实现 |
| 预算算不过来 | V-F09、V-H01 | 常驻 RSS 门禁 |
| `--json` 投影不确定 | V-E04、V-B06 | 生产化 + manifest 登记差异 |
| history 泄漏 key | V-D06、V-H06 | 按环境脱敏策略 |
| profile × flag 冲突 | V-B07 | 生产化 |
| REPL 退出码 | V-B04 | `--exit-on-error`、`:status` |
| Windows 上的 REPL | V-C07、V-G05 | Windows 全路径接入 CI |
| 预算测不准 | V-A06 | 每次 PR 输出 RSS/分配差报告 |
| 未知命令/模块结果 panic | V-E05 | generic RESP 视图接入全部 renderer 分派 |
| catalog 编译/签名 | V-F01 | 发布管线生产化；`HG` 下拉的数据来源 |

**本阶段新增测试：** v2.1 §36 列出的 **E2 必过 ASSIST 子集**：001–008、016、017、020–028、029、037、047、048、062–064、066、076、077、080、081、082、087；PIPE-01~07；UX-01~12；CMD-01~09；DATA-02~07；SEC-01/05/08。

**演示门禁：** v2.1 §37.2 的第一条纵向链路在 3 平台 PTY 上端到端可演示并有录制。

**出口门禁：** 上述全部测试绿；纵向链路录制存档；`prc --demo` 与 `prc --learn hash` 离线可用；无 Phase 0 结论被绕过。

---

## 5 · Phase 2A — Deep intelligence（E2A）

**范围：** grammar/overlay、key/field scope、Guide、错误说明、offline Learn、显式 discovery。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| 语法复杂度（SET/ZADD/XADD/EVAL/XREAD） | V-F04 | 全部命令 overlay 完成并评审 |
| 候选跨 dev/prod 泄漏 | V-F06 | 生产化 |
| 显式发现在真实规模无用 | V-F07 | UI 进度、取消、生产确认流程 |
| 用途搜索指标是循环的 | V-F08 | held-out 集维护流程 |
| 远端 metadata 覆盖本地策略 | V-D07 | 生产化 |
| Guide/单位/错误帮助偷发命令 | V-F10 | 全部 §13 功能 |
| 迟到候选污染输入 | V-F05 | 生产化 |
| Valkey 命令差异被当成 Redis | V-I02、V-F01 | `server_family` 分支 overlay 完成 |

**本阶段新增测试：** ASSIST-001~088 **全部**；每条一行证据（自动化路径或人工记录编号）。

**出口门禁：** 88 条全部有证据；§16.4 全部性能门禁绿；catalog 每次变更触发 §16.5 全套。

---

## 6 · Phase 3 — Production topology（E3）

**范围：** Cluster/Sentinel、TrustIdentity、SSH、ACL/approvals、取消恢复。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| 审批可被摘要碰撞绕过 | V-D03 | 审批 UI、`:approval show --bytes` |
| 凭证被发到被替换的服务 | V-D04 | 轮换向导、边界外重定向流程 |
| SSH 下 Cluster 只能连 seed | V-G03 | per-node / SOCKS5 生产化 |
| hash tag 路由错 | V-G01 | 生产化 |
| Sentinel failover 重放写 | V-G02 | 生产化 |
| TLS 静默降级 | V-G04 | 生产化 |
| 事务中切 profile | V-B05 | 生产化 |
| ACL channel 维度缺失 | V-D03、V-B05 | Pub/Sub 策略维度 |
| 信任边界无用例 | V-J03 | 威胁模型每条边界 → SEC 用例常驻 |
| 拓扑故障不可复现 | V-A04 | Cluster/Sentinel 故障注入 deterministic 场景 |

**本阶段新增测试：** NET-01~07；STATE-05~08；SEC-02~06、SEC-09~11；ASSIST-029~036、088 在真实拓扑上重跑。

**出口门禁：** 全绿；每类拓扑故障有 deterministic 场景与日志定位。

---

## 7 · Phase 4 — Deep TUI（E4）

**范围：** browser、inspector、editor、diff、virtualization、任务面板、同等智能命令框。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| 数据页快捷键抢输入 | V-C02 | 焦点分派生产化 |
| 反复开关 TUI 泄漏 | V-H04 | 常驻 PERF-03 |
| 几十万行 widget 树 | V-E02、V-E03 | virtualization |
| 编辑覆盖并发修改 | V-B05（WATCH） | §25.3 提交流程 |
| 重复 JSON 成员编辑到错的那个 | V-E01 | occurrence 路径 UI |
| 宽字符错位 | V-C04 | 降级绘制接入 |

**本阶段新增测试：** UX-12；STATE-05；JSON-05；PERF-03；DATA-05；全部 TUI 页面的 PTY 场景；8h soak 含 TUI。

**出口门禁：** 全绿；TUI 命令框与 CLI 共用同一 `pr-intelligence` 测试集（ASSIST-002）。

---

## 8 · Phase 5 — Operational suite（E5）

**范围：** diagnostics、Streams、Pub/Sub、archive、bulk、Lua、`--pipe`、官方特殊模式。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| `--pipe` 绕过策略 | V-B03 | 生产化；`--plan-from-pipe` |
| 特殊模式无边界 | V-I04 | §28.4 每模式契约实现 |
| MONITOR / `--rdb` / `--replica` 数据外泄 | V-I04、V-D03 | prod/agent deny 默认；审批 |
| 订阅风暴 | V-H03 | 生产化 |
| Pub/Sub 断线假装补齐 | V-B05 | gap marker |
| 大范围比较预算 | V-E03 | 外部排序、指纹 |
| 模块结果版本不识别 | V-E05 | 每个模块 adapter 的 fallback 测试 |

**本阶段新增测试：** PIPE-05、PIPE-08；STATE-08；PERF-02；CMD-11；每个特殊模式的独立 fixture + 退出码 + 审批测试。

**出口门禁：** §28.4 契约表每格有测试；`redis-cli` baseline 的每个模式在 manifest 中标 verified / unsupported，无空白。

---

## 9 · Phase 6 — Automation（E6）

**范围：** recipes、signed/hashed plans、structured reports、可选 MCP/AI。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| agent 自批自执 | V-D08、V-D03 | 生产化 |
| MCP stdout 混入 banner | V-D08 | 生产化 |
| plan 过期/替换 | V-D03 | plan 版本与 hash 校验 |
| AI 指令注入 | V-D05 | 数据/指令边界测试 |

**本阶段新增测试：** SEC-03、SEC-06、SEC-08；MCP 工具面每个 tool 的策略测试；recipe 参数 bytes 注入测试。

**出口门禁：** 所有入口经同一 kernel/policy 的追踪测试；无 MCP 路径能生成 `ApprovalToken`。

---

## 10 · Phase 7 — Product hardening（E7）

**范围：** 全平台分发、文档、支持矩阵、benchmark、升级/回滚。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| Windows 是纸面支持 | V-C07、V-G05 | 全功能矩阵按功能打勾 |
| 许可阻止发布 | V-I03 | notices、SBOM |
| 插件崩溃拖垮进程 | V-D09 | L1/L2 插件 API 版本化 |
| 24h soak 没跑过 | V-H05 | 24h 发布 soak |
| 升级破坏配置 | V-D02、V-H06 | 迁移 + 回滚脚本 |
| benchmark 不可复现 | V-A06 | §34.1 可复现 benchmark 报告 |

**本阶段新增测试：** LIFE-01~04；PERF-04；Packaging 层全部；每平台安装/升级/卸载在干净机器。

**出口门禁：** v2.1 §34.3 Release evidence bundle 完整；§38.1 每条有证据；无未知 P0。

---

## 11 · Phase 8 — Studio integration（E8）

**范围：** core SDK、schema migration、adapter 与共享 fixtures。

| 本阶段会遇到的问题 | Phase 0 已验证 | 本阶段剩余工作 |
|---|---|---|
| Studio 反向依赖 core | V-J02（接口冻结） | SDK 边界测试 |
| fixture 不共享 | V-A01~A04 | fixture 打包 |

**出口门禁：** Studio 未安装时 CLI 全部 §33 案例仍绿（回归全跑一遍）。

---

## 12 · 追溯矩阵：v2.1 修订 → Phase 0 验证项 → 归属 Phase

| v2.1 修订 | 内容 | Phase 0 验证项 | 归属 Phase |
|---|---|---|---|
| R01 | `--pipe` 逐帧策略 | V-B03 | 5（策略部分 3） |
| R02 | 审批绑字节哈希 | V-D03 | 3 |
| R03 | Reedline 边界 + SPIKE-001 | V-C01、V-C02 | 2 |
| R04 | 二进制入口 | V-F03、V-B02 | 2 |
| R05 | bracketed paste | V-C03 | 2 |
| R06 | `pr-json` occurrence DOM | V-E01 | 2 / 4 |
| R07 | journal + reconciliation | V-D02 | 2 |
| R08 | 预算算术 | V-F09、V-H01 | 2 |
| R09 | 自动/显式发现预算 | V-F07 | 2A |
| R10 | recall 双集合 | V-F08 | 2A |
| R11 | 双解析器等价 | V-F02、V-F03 | 2 |
| R12 | MCP 策略批准边界 | V-D08 | 6 |
| R13 | topology epoch | V-F06 | 2A / 3 |
| R14 | `TrustIdentity` | V-D04 | 3 |
| R15 | SSH per-node/SOCKS5 | V-G03 | 3 |
| R16 | RESP3 streamed | V-B02 | 1 |
| R17 | push / projection | V-B06、V-E04 | 2 |
| R18 | 本地命令 tokenizer | V-F03 | 2 |
| R19 | 本地 catalog 权威 | V-D07 | 2A |
| R20 | Windows 契约 | V-C07、V-G05 | 2 / 7 |
| R21 | 任务监督 | V-H04 | 1 |
| R22 | rusqlite | V-H06 | 2 |
| R23 | keyring 平台 | V-D01 | 2 |
| R24 | history 脱敏 | V-D06 | 2 |
| R25 | Light/HC 主题 | V-C06 | 2 |
| R26 | flag 矩阵 | V-B07 | 2 |
| R27 | REPL 退出码 | V-B04 | 2 |
| R28 | Down / Ctrl+N | V-C01、V-C05 | 2 |
| R29 | Valkey / 许可 / 矩阵 | V-I01~I03 | 0 / 7 |
| R30 | 四维度 outcome | V-B04 | 1 |
| R31 | 阈值顺序 | V-E03 | 2 |
| R32 | prefix vs glob | V-F07 | 2A |
| R33 | 宽度策略 | V-C04 | 2 |
| R34 | SafeText 边界 | V-D05 | 2 |
| R35 | 进程级预算 | V-F09、V-H01 | 2 |
| R36 | 特殊模式契约 | V-I04、V-B03 | 5 |
| R37 | E2 门禁清单 | — | 2（§4 门禁） |
| R38 | 新增用例/ADR/issue | 分布于各 V 项 | 各 |
| R39 | 共享文件锁 | V-G05、V-H06 | 2 |
| R40 | `--output resp` 用途 | V-B06 | 2 |
| R41 | 字节数不记内容 | V-D06 | 1 |
| R42 | 退出码优先级 | V-B04 | 1 |
| R43 | hash tag | V-G01 | 3 |
| R44 | ACL channel | V-D03、V-B05 | 3 / 5 |
| R45 | 插件两级隔离 | V-D09 | 7 |
| R46 | `--mcp-stdio` bootstrap | V-D08 | 6 |
| R47 | raw TTY 警告 / 秘密位置 | V-D05、V-D01 | 2 |

**每一条 v2.1 修订都有 Phase 0 验证项。** 没有任何一条被留到「做到那个 Phase 再看」。

---

## 13 · 每个 Phase 通用的完成定义

一个 Phase 声称完成，必须同时满足：

1. 该 Phase 所有列出的验收案例在 CI 绿，且在 3 平台的 PTY 上绿（Windows 对 legacy conhost 允许 `plain-only` 标注，但 Windows Terminal 必须全过）。
2. 每个新功能都有**失败路径**测试（不只是 happy path）：断网、超时、权限拒绝、畸形输入、预算超限。
3. 该 Phase 引入的每条命令在支持矩阵中分别登记 `send / route / state / render / suggest / signature / guide / docs / tested` 状态（v2.1 §20.5）。
4. 无新增 P0 安全/数据错误；已知差异写入 release notes。
5. 帮助文本、`--help`、Command Card、文档与实现同步（v2.1 §34.2 Documentation 门禁）。
6. 资源门禁（RSS、启动、typing latency）在该 Phase 结束时重新测量并与 Phase 0 基线对比；退化必须有解释与 ADR，不允许静默放宽。
7. Phase 报告：每个验收案例一行 = 状态 + 证据链接。

**不接受**：「先合进去，测试下个 Phase 补」、「这个平台先跳过」、「预算先放宽等优化」。这些都是 v2.1 §38.2 列出的退化。
