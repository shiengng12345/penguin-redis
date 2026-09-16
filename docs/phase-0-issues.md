# Penguin Redis — Phase 0 Issue 草稿
## 依据 `penguin-redis-phase-plan.md` §2.3 生成 · v1.0

> **63 个验证项 issue + 3 个 meta issue。** 验证什么 / 方法 / 通过标准 / 产出 / 不通过时 五个字段**直接来自 phase-plan 表格**（脚本生成，不会漂移）；依赖与任务分解为本文新增。
> 机器可读版本：`phase-0-issues.json`（title / labels / body / deps），可直接喂给 `gh issue create` 或任何 tracker。
> 状态规则：结束时只能是 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`。不存在 DEFERRED / SKIPPED。

## 依赖顺序（拓扑）

无依赖、可立即开工的 issue：**V-A01、V-A03、V-A04、V-A05、V-A06、V-I01、V-I03、V-J01、P0-META-01、P0-META-03**

推荐波次（同一波内可并行）：

- **Wave 1**：P0-META-01、P0-META-03、V-A01、V-A03、V-A04、V-A05、V-A06、V-I01、V-I03、V-J01
- **Wave 2**：V-A02、V-B01、V-B02、V-B05、V-C03、V-C04、V-C05、V-C06、V-C07、V-D01、V-D09、V-F03、V-G01、V-G02、V-G04、V-H01、V-H05、V-H06、V-I02、V-I04、V-J02、V-J03
- **Wave 3**：V-B03、V-B04、V-B06、V-B07、V-C01、V-D02、V-D03、V-D04、V-D05、V-D06、V-E01、V-E02、V-E03、V-E04、V-E05、V-F01、V-F02、V-F05、V-G03、V-G05、V-H02、V-H03、V-H04
- **Wave 4**：V-C02、V-D07、V-D08、V-F04、V-F06、V-F08、V-F09、V-F10、V-J04
- **Wave 5**：V-F07
- **Wave 6**：P0-META-02

## Labels

`phase-0` · `track-a`…`track-j` · `harness` / `ci` / `spike` / `prototype` / `fixture` / `freeze` / `audit` / `meta` / `gate` / `manual-verification`

---

## Track A · 基础设施与 harness

### V-A01 · PTY harness

`phase-0` `track-a` `harness`

**Track** A · 基础设施与 harness
**类型** harness
**排雷对象** Phase 2、2A、4、7
**v2.1 依据** §32.1 PTY E2E、§32.4、R20
**依赖** 无

### 验证什么
**PTY harness**：macOS/Linux PTY + Windows ConPTY 统一驱动；resize、粘贴（含 bracketed）、颜色能力声明、录制回放

### 方法
用 harness 在 3 平台跑 10 个基线场景（回显、resize、Ctrl+C、alternate screen 进出、粘贴、颜色 reset）

### 任务
- [ ] 选定 PTY 驱动（Unix `openpty` + Windows ConPTY），封装统一 `PtySession` API
- [ ] 实现录制（输入事件 + 输出字节 + 时间戳）与回放 diff
- [ ] 编写 10 个基线场景 fixture
- [ ] 在 macOS / Linux / Windows runner 各跑一遍并归档录制

### 通过标准（验收）
3 平台同一 fixture 全过；录制可回放 diff

### 产出
`tests/pty/harness/`、`fixtures/pty/baseline/`

### 不通过时
换 PTY 驱动库；**无 PTY harness 不得进入 Phase 2**

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-A02 · Differential harness

`phase-0` `track-a` `harness`

**Track** A · 基础设施与 harness
**类型** harness
**排雷对象** Phase 1、2、5、7
**v2.1 依据** §32.2、§20.4
**依赖** V-A05

### 验证什么
**Differential harness** vs 固定 `redis-cli` baseline：两套独立同初态 Redis，四层比对（argv bytes / response / 最终状态 / 输出+exit）

### 方法
用骨架 `prc`（只做透传）跑 CMD-01~05

### 任务
- [ ] 容器化两套独立 Redis（同 image digest、同初始 dump）
- [ ] 实现四层比对器：argv bytes / response frames / 最终 `DEBUG` 状态快照 / stdout+stderr+exit
- [ ] 实例化附录 B.2 模板为 JSON 报告格式
- [ ] 跑 CMD-01~05，确认写命令不共用实例

### 通过标准（验收）
5 条案例产出四层比对报告；写命令不共用实例

### 产出
`tests/differential/`、附录 B.2 模板实例化

### 不通过时
修 harness；不允许用「同实例先后执行」替代

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-A03 · Synthetic RESP server

`phase-0` `track-a` `harness`

**Track** A · 基础设施与 harness
**类型** harness
**排雷对象** Phase 1、5
**v2.1 依据** §19.3、§32.3、R16、R17
**依赖** 无

### 验证什么
**Synthetic RESP server**：可编程发送任意 RESP2/3 帧（streamed string/aggregate、push、attribute、畸形、1 GiB blob、半包、乱序 push）

### 方法
生成 §32.3 列出的全部 fuzz 类别

### 任务
- [ ] 实现可脚本化 RESP2/3 帧生成器（含 streamed、push、attribute、畸形）
- [ ] 支持按脚本延迟、半包切分、乱序 push、断开
- [ ] 生成 §32.3 每类 ≥ 20 个 fixture
- [ ] 提供 1 GiB blob 流式发送而不在内存物化

### 通过标准（验收）
每类至少 20 个 fixture；server 可按脚本延迟/断开

### 产出
`xtask/resp-server/`、`fixtures/protocol/`

### 不通过时
无回退；这是 Phase 1 的必需工具

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-A04 · Fault injection

`phase-0` `track-a` `harness`

**Track** A · 基础设施与 harness
**类型** harness
**排雷对象** Phase 1、3、5、7
**v2.1 依据** §32.1 Fault injection、§30.5
**依赖** 无

### 验证什么
**Fault injection**：断网、半包、丢响应、超时、DNS/TLS 失败、磁盘满、只读 FS、权限撤销 可脚本化

### 方法
对骨架进程注入每类故障

### 任务
- [ ] 网络层：断网 / 半包 / 丢响应 / 超时（可编程 proxy）
- [ ] 系统层：磁盘满 / 只读 FS / 权限撤销（容器 + 挂载选项）
- [ ] DNS / TLS 失败注入
- [ ] 每类故障一个可重复脚本 + 骨架进程不 panic 断言

### 通过标准（验收）
每类故障有可重复脚本；进程不 panic

### 产出
`tests/fault/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-A05 · 多目标 CI 矩阵

`phase-0` `track-a` `ci`

**Track** A · 基础设施与 harness
**类型** ci
**排雷对象** Phase 全部
**v2.1 依据** §20.4 矩阵、§32.1、§34.2
**依赖** 无

### 验证什么
**多目标 CI 矩阵**：Redis 7.2/7.4/8.0、Valkey 8.0/8.1、Cluster（6 节点）、Sentinel（3+1）、TLS（自签 CA + client cert）、macOS/Linux/Windows runner、8h soak runner、24h 发布 soak runner

### 方法
用 hello-world crate 跑满矩阵

### 任务
- [ ] 为 §20.4 每个 target 拉取并 pin image digest，写入 `compatibility/manifest.toml`
- [ ] 搭 Cluster（6 节点）与 Sentinel（3+1）compose
- [ ] 自签 CA 生成 server/client cert，含错名/过期样本
- [ ] 接入 macOS / Linux / Windows runner
- [ ] 建 8h soak job 与 24h release-soak job（手动触发）
- [ ] hello-world crate 跑满矩阵全绿

### 通过标准（验收）
矩阵全绿；每个 target 的 image digest 已写入 `compatibility/manifest.toml`

### 产出
CI 配置、`compatibility/manifest.toml`

### 不通过时
缺哪个 target 就不许声称支持哪个；不允许用 `latest`

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-A06 · 资源测量工具

`phase-0` `track-a` `harness`

**Track** A · 基础设施与 harness
**类型** harness
**排雷对象** Phase 2、4、7 + 所有预算项
**v2.1 依据** §12.5、§24.7、§16.4、R08、R35
**依赖** 无

### 验证什么
**资源测量工具**：allocator 统计 hook、RSS 采样、任务计数暴露、只读 mmap 单独计量

### 方法
在骨架进程上产出「开/关某模块」的 RSS 差报告

### 任务
- [ ] allocator 统计 hook（分配次数/字节）
- [ ] RSS 采样器（跨平台）
- [ ] 任务计数暴露接口（供 `TaskScope` 与 `:tasks` 使用）
- [ ] mmap 区域单独计量并在报告中分列
- [ ] 「开/关模块」差值报告可复现 ±5%

### 通过标准（验收）
报告可复现（±5%）；mmap 与堆分开

### 产出
`xtask/measure/`、`benches/baseline/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track B · 协议与执行内核

### V-B01 · SPIKE-002 redis-rs 低层 API

`phase-0` `track-b` `spike`

**Track** B · 协议与执行内核
**类型** spike
**排雷对象** Phase 1
**v2.1 依据** §31.2、§37.3
**依赖** V-A03

### 验证什么
**SPIKE-002 redis-rs 低层 API**：是否保留 RESP frame 类型、增量读、push 分流、取消后状态、重定向控制

### 方法
对 V-A03 的全部帧类别用 redis-rs 低层 API 读取，比对保真

### 任务
- [ ] 枚举 §31.2 验收清单为逐项测试
- [ ] 用 redis-rs 低层 API 读取 V-A03 全部帧类别，比对保真
- [ ] 测取消后读取行为、push channel 预算、内部重试
- [ ] 撰写 `docs/spikes/SPIKE-002.md`：采用 / 拒绝 + 反例；若拒绝则起草 ADR 采纳自研 codec

### 通过标准（验收）
§31.2 清单全部满足

### 产出
`docs/spikes/SPIKE-002.md`

### 不通过时
**ADR 采纳 `pr-protocol` 自研 codec 为唯一 kernel 边界**；redis-rs 降为互操作测试对象

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B02 · 增量 RESP decoder 原型

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 1、5
**v2.1 依据** §19.3、§24.2、R16
**依赖** V-A03、V-A06

### 验证什么
**增量 RESP decoder 原型**（无论 B01 结论都要做，§19.3 的 streamed 与有界解码不依赖第三方）

### 方法
对 V-A03 全部类型解码；1 GiB synthetic blob；半包/畸形 fuzz

### 任务
- [ ] 实现增量 decoder 状态机，覆盖 §19.3 全部类型含 streamed
- [ ] 有界解码：最大嵌套 / 声明长度 / aggregate 个数 / 总预算
- [ ] 1 GiB synthetic blob 流式，记录峰值内存
- [ ] fuzz 10⁶ 次（畸形长度、截断 CRLF、UTF-8 跨块、压缩炸弹）无 panic

### 通过标准（验收）
类型全覆盖含 streamed；1 GiB 峰值内存 ≤ 块预算；fuzz 10⁶ 次无 panic/无界分配

### 产出
`crates/pr-protocol/`、`fixtures/protocol/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B03 · `--pipe` 逐帧路径

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 5、3
**v2.1 依据** §18.5、R01、SEC-09、PIPE-08
**依赖** V-B02、V-J02

### 验证什么
**`--pipe` 逐帧路径**：stdin RESP → `CommandRequest` → policy → send → per-frame outcome；背压队列；畸形 fail-closed

### 方法
prod-deny profile 下喂含写帧的流；喂截断帧

### 任务
- [ ] stdin RESP 流 → `CommandRequest` 逐帧转换
- [ ] 非 array-of-bulk 帧 / 截断 / 超长 → fail-closed，报帧序号与偏移
- [ ] 接入同一 catalog 分类与 policy；prod-deny 下写帧停止（退出码 5）
- [ ] 有界背压队列（1,000 帧 / 8 MiB）与断线后 `UnknownAfterSend`
- [ ] SEC-09、PIPE-08 fixture

### 通过标准（验收）
在第一条写帧停止，退出码 5；截断帧退出码 2 并报帧序号；无静默跳过

### 产出
`tests/pipe/`、`fixtures/hostile-input/pipe/`

### 不通过时
无回退；这是 BLOCKER 修复的验证

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B04 · `ExecutionOutcome` 四维度模型 + 退出码映射表

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 1、2
**v2.1 依据** §19.2、§18.6、§22.3、R27、R30、R42
**依赖** V-A03、V-J02

### 验证什么
**`ExecutionOutcome` 四维度模型 + 退出码映射表**

### 方法
用 V-A03 制造每种 delivery×reply×effects 组合

### 任务
- [ ] 实现四维度 `ExecutionOutcome` 类型
- [ ] 枚举 delivery×reply×effects 组合，用 V-A03 逐一制造
- [ ] 编写退出码映射表 fixture（含 §22.3 Ctrl+C 优先级）
- [ ] typed output `outcome` 字段与退出码同源断言

### 通过标准（验收）
映射表 fixture 100%；Ctrl+C vs unknown 优先级按 §22.3

### 产出
`crates/pr-core/src/outcome.rs`、`fixtures/outcome/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B05 · 会话状态机

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 1、3
**v2.1 依据** §22.4~22.6、STATE-01~08
**依赖** V-A03

### 验证什么
**会话状态机**：CLIENT REPLY OFF/SKIP、MULTI/EXEC/WATCH、HELLO/AUTH/SELECT/RESET；断线后不重放

### 方法
synthetic server 上跑状态转换与断线

### 任务
- [ ] 状态机覆盖 protocol / auth identity / DB / reply mode / subscription / transaction / tracking
- [ ] CLIENT REPLY OFF/SKIP 下辅助请求不占回复位
- [ ] MULTI/EXEC/WATCH 逐项关联；断线不重建事务
- [ ] STATE-01~08 在 synthetic server 上全过

### 通过标准（验收）
STATE-01~08 全过

### 产出
`tests/session-state/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B06 · push 帧处理

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 2、5
**v2.1 依据** §18.1、R17、R40
**依赖** V-A03、V-B02

### 验证什么
**push 帧处理**：one-shot `--json` 不进 stdout；`--show-pushes`；ndjson 事件；`--output resp` 再编码

### 方法
synthetic server 在回复前后插 push

### 任务
- [ ] one-shot `--json`/`--raw`/`--csv` 下 push 丢弃 + stderr 计数
- [ ] `--show-pushes` NDJSON 到 stderr；`--output ndjson|typed-json` 作为事件入 stdout
- [ ] `--output resp` 规范化再编码器 + `--trace-wire` 分离
- [ ] push 在回复前/后/独立到达三种时序 fixture

### 通过标准（验收）
stdout 只有回复；stderr 计数；ndjson 顺序正确

### 产出
`tests/output/push/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-B07 · CLI 参数边界 parser

`phase-0` `track-b` `prototype`

**Track** B · 协议与执行内核
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §3.2、§3.3、R26、CMD-01
**依赖** V-J02

### 验证什么
**CLI 参数边界 parser**：`prc [opts] [@profile] [opts] [--] CMD …`；profile × flag 允许/冲突矩阵

### 方法
表驱动测试

### 任务
- [ ] 表驱动实现 §3.3 profile × flag 允许/冲突矩阵
- [ ] `--` 终止客户端参数区；`SET k '--raw'` 原样
- [ ] `-h` 保留 host 含义；帮助走 `--help`
- [ ] 每格一个用例 + CMD-01

### 通过标准（验收）
§3.3 矩阵每格一个用例；`SET k '--raw'` 原样

### 产出
`crates/prc/tests/args.rs`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track C · 终端与编辑器

### V-C01 · SPIKE-001 Reedline 作编辑引擎

`phase-0` `track-c` `spike`

**Track** C · 终端与编辑器
**类型** spike
**排雷对象** Phase 2、2A、4
**v2.1 依据** §14.5、§31.1、R03、R28
**依赖** V-A01、V-J02

### 验证什么
**SPIKE-001 Reedline 作编辑引擎**：`LineBuffer` 接受外部 `TextEdit` 不丢字节；撤销栈把「接受候选」当单事务；与自绘菜单组合

### 方法
在 V-A01 上跑 ASSIST-020~028、062~064、087

### 任务
- [ ] 用 Reedline `LineBuffer`/`Editor` 接受外部 `TextEdit`，验证 UTF-8 边界与字节不丢
- [ ] 「接受候选」封装为单个可撤销事务
- [ ] 与自绘菜单组合跑 ASSIST-020~028、062~064、087（PTY）
- [ ] 撰写 SPIKE-001 报告；不满足则起草 ADR-026 回退自有 LineBuffer 并跑同一套测试

### 通过标准（验收）
全过

### 产出
`docs/spikes/SPIKE-001.md`、`tests/editor-state/`

### 不通过时
**ADR-026 回退：`pr-repl` 自有 LineBuffer**（grapheme-aware，复用 Reedline history 格式），同一套测试必须通过

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C02 · 单一 terminal owner 原型

`phase-0` `track-c` `prototype`

**Track** C · 终端与编辑器
**类型** prototype
**排雷对象** Phase 2、4
**v2.1 依据** §14.4、ADR-022
**依赖** V-A01、V-C01

### 验证什么
**单一 terminal owner 原型**：`pr-terminal` 同时驱动 REPL 编辑、自绘下拉、异步消息排队、TUI alternate screen 切换

### 方法
Pub/Sub 消息涌入 + 打字 + 菜单 + 切 TUI 再回

### 任务
- [ ] `pr-terminal` 唯一 crossterm event source + painter
- [ ] REPL 编辑 / 自绘下拉 / 异步消息队列 / TUI alternate screen 切换共用 coordinator
- [ ] Pub/Sub 涌入 + 打字 + 菜单 + 切 TUI 再回 PTY 场景（ASSIST-062、UX-12、场景 H）
- [ ] 断言无第二个 stdin 读者、无并发 stdout 写者

### 通过标准（验收）
ASSIST-062、UX-12、场景 H 过；无 stdin/stdout 竞争

### 产出
`crates/pr-terminal/`、`tests/pty/owner/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C03 · bracketed paste 矩阵

`phase-0` `track-c` `prototype`

**Track** C · 终端与编辑器
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §14.1、R05、ASSIST-082、UX-11
**依赖** V-A01

### 验证什么
**bracketed paste 矩阵**：macOS Terminal、iTerm2、kitty、Alacritty、WezTerm、Windows Terminal、conhost、tmux、SSH 中的 `CSI ?2004` 支持；无支持时 10 ms 时序启发式的漏检/误判

### 方法
每个终端粘贴 3 行写命令

### 任务
- [ ] 在 9 种终端启用 `CSI ?2004 h` 并粘贴 3 行写命令，记录是否收到成对标记
- [ ] 实现 10 ms 时序启发式，在无支持终端上测漏检/误判率
- [ ] paste-staging 原型（逐行显示、编辑、三种提交选择）
- [ ] ASSIST-082 fixture + 每终端人工记录

### 通过标准（验收）
有支持终端 100% 进 staging；无支持终端启发式在 fixture 上 0 漏检；误判率记录

### 产出
`tests/pty/paste/`、人工记录

### 不通过时
若某终端两者皆失败 → 该终端标 `plain-only`，在支持矩阵明示

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C04 · Unicode 宽度矩阵

`phase-0` `track-c` `prototype`

**Track** C · 终端与编辑器
**类型** prototype
**排雷对象** Phase 2、4
**v2.1 依据** §14.6、R33、UX-08、ASSIST-018/067
**依赖** V-A01

### 验证什么
**Unicode 宽度矩阵**：ambiguous / ZWJ / 组合字符在上述终端的实测宽度；`CSI 6 n` 探测可行性；「每格重置光标」降级绘制

### 方法
固定策略 + 仿真终端 + 真实终端人工记录

### 任务
- [ ] `unicode-width` grapheme 计算 + ZWJ 序列按 2 列 + ambiguous 可配置
- [ ] `CSI 6 n` 光标探测原型（TTY + 用户触发）
- [ ] 「每格重置光标」降级绘制原型
- [ ] 仿真终端 UX-08 / ASSIST-018 / 067；真实终端人工记录

### 通过标准（验收）
仿真下 UX-08 过；降级绘制在宽度判错时框线仍对齐

### 产出
`tests/render/width/`、人工记录

### 不通过时
无回退（策略已是可实现契约）

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C05 · 按键可达性

`phase-0` `track-c` `prototype`

**Track** C · 终端与编辑器
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §14.2、ASSIST-068
**依赖** V-A01

### 验证什么
**按键可达性**：F1–F6、Ctrl+R/P/N、Ctrl+Space 在各终端与 IME（macOS 拼音、Windows IME、fcitx）下是否被拦截；文字入口回退

### 方法
每终端 + IME 组合逐键记录

### 任务
- [ ] 每终端 × IME（macOS 拼音、Windows IME、fcitx）逐键记录 F1–F6、Ctrl+R/P/N、Ctrl+Space
- [ ] 确认每个功能至少一条文字入口（`:help`/`:find`/`:guide`/`:complete`/`:history`）可用
- [ ] 被拦截键在该平台默认重映射并写入文档
- [ ] PTY 键位 fixture + 人工记录

### 通过标准（验收）
每个功能至少一条文字入口在所有组合可用

### 产出
人工记录 + `tests/pty/keys/`

### 不通过时
被拦截的键在该平台默认重映射并写入文档

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C06 · 颜色层级与 reset

`phase-0` `track-c` `prototype`

**Track** C · 终端与编辑器
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §7.1~7.3、R25、UX-04/06、PIPE-01
**依赖** V-A01

### 验证什么
**颜色层级与 reset**：24bit→256→16→mono 量化后相邻语义可分辨；表头背景不泄漏；管道无 escape；Light/High-contrast token 对比度

### 方法
golden tests × 4 色阶 × 3 主题

### 任务
- [ ] 实现 24bit→256→16→mono 量化
- [ ] Penguin Dark / Light / High-contrast 三主题 token 表落地
- [ ] 相邻语义可分辨测试 + WCAG 对比度自动校验
- [ ] 表头背景不泄漏、每段 reset、管道无 escape 的 golden tests

### 通过标准（验收）
全过；WCAG 4.5:1 自动校验过

### 产出
`fixtures/renderer/colour/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-C07 · SPIKE-003 Windows ConPTY 专项

`phase-0` `track-c` `spike`

**Track** C · 终端与编辑器
**类型** spike
**排雷对象** Phase 2、7
**v2.1 依据** §35.1、R20、WIN-01/02
**依赖** V-A01、V-A05

### 验证什么
**SPIKE-003 Windows ConPTY 专项**：V-A01 全部场景 + 二进制 stdio + `SetConsoleCtrlHandler` + Ctrl+Break

### 方法
Windows runner 上跑

### 任务
- [ ] Windows runner 上跑 V-A01 全部场景（ConPTY）
- [ ] stdin/stdout 二进制模式：`-x`/`-X`/`--bytes`/`--pipe` 无 CRLF 转换
- [ ] `SetConsoleCtrlHandler` + Ctrl+Break 等价两次 Ctrl+C
- [ ] WIN-01、WIN-02 过；legacy conhost 不达则标 `plain-only`；撰写 SPIKE-003

### 通过标准（验收）
WIN-01、WIN-02 过

### 产出
`docs/spikes/SPIKE-003.md`

### 不通过时
若 legacy conhost 不达 → conhost 标 `plain-only`；Windows Terminal 必须过

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track D · 安全与凭证

### V-D01 · `keyring` 平台矩阵

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §4.3、§31.1、R23
**依赖** V-A05

### 验证什么
**`keyring` 平台矩阵**：macOS `apple-native`、Windows `windows-native`、Linux Secret Service + keyutils 回退；headless fail-closed；解锁/授权提示行为

### 方法
三平台存/取/删 + headless 容器

### 任务
- [ ] `keyring` 固定版本；macOS `apple-native` / Windows `windows-native` / Linux `sync-secret-service` + `linux-native` 回退
- [ ] 三平台存 / 取 / 删 + service 命名 `penguin-redis/<uuid>`
- [ ] headless 容器（无 D-Bus、无 UI）下 fail-closed，无明文落盘
- [ ] 记录每平台解锁/授权提示行为

### 通过标准（验收）
三平台通过；headless 无明文落盘且给出明确错误

### 产出
`tests/credentials/`

### 不通过时
某平台缺 store → 该平台只允许 askpass/会话内秘密，并在支持矩阵明示

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D02 · 跨资源提交 journal + reconciliation

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §4.2、§30.2、R07
**依赖** V-D01、V-A04

### 验证什么
**跨资源提交 journal + reconciliation**：在 §4.2 六步的每一步注入崩溃

### 方法
fault injection 崩溃矩阵

### 任务
- [ ] 实现 §4.2 六步提交 + journal（追加、fsync、私有权限、大小上限）
- [ ] 用 V-A04 在每一步之后注入崩溃
- [ ] 启动 reconciliation：孤儿报告不自动删 / 悬空标 `credential missing` / 重试清理
- [ ] 崩溃矩阵全部收敛断言

### 通过标准（验收）
每种崩溃后启动 reconciliation 收敛；孤儿/悬空全部被报告

### 产出
`tests/credentials/journal/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D03 · `ApprovalToken` 字节哈希

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 3、5、6
**v2.1 依据** §23.2、R02、SEC-10
**依赖** V-J02

### 验证什么
**`ApprovalToken` 字节哈希**：「同摘要不同字节」对照集；epoch/TrustIdentity 变化失效

### 方法
表驱动

### 任务
- [ ] 实现 `ApprovalToken`：`blake3(len-prefixed argv)` + `TrustIdentity` 哈希 + epoch + expiry
- [ ] 构造「同摘要不同字节」对照集（Unicode 规范化、转义、截断、掩码）
- [ ] epoch / TrustIdentity / DB 变化失效测试
- [ ] SEC-10

### 通过标准（验收）
SEC-10 过

### 产出
`crates/pr-security/tests/approval.rs`

### 不通过时
无回退；BLOCKER 修复验证

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D04 · `TrustIdentity` 绑定

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 3
**v2.1 依据** §21.3、R14、SEC-05 扩展
**依赖** V-A05、V-J02

### 验证什么
**`TrustIdentity` 绑定**：证书轮换过渡窗、failover 到已登记节点不重绑、边界外重定向确认

### 方法
Cluster/Sentinel fixture + 证书轮换脚本

### 任务
- [ ] 实现 `TrustIdentity` 四元组与凭证绑定（TLS 身份 + 服务身份，不绑 endpoint）
- [ ] 证书轮换过渡窗脚本；failover 到已登记节点不重绑
- [ ] Cluster 边界外重定向 → 暂停确认
- [ ] SEC-05 扩展 fixture

### 通过标准（验收）
SEC-05 过；边界外重定向必停

### 产出
`tests/topology/trust/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D05 · SafeText 类型边界

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 2、4
**v2.1 依据** §23.5、§12.9、R34、R47、SEC-05
**依赖** V-J02、V-A01

### 验证什么
**SafeText 类型边界**：renderer/TUI widget 签名只接受 `SafeText`；含 OSC/CSI 的 key/field/channel/`CLIENT LIST` 字段在所有视图不触发副作用；`--bytes`/`--output resp` 直写 TTY 警告

### 方法
编译期断言 + PTY 注入

### 任务
- [ ] `SafeText` 类型：转义 C0/C1/ESC/OSC/DCS/APC/孤立 surrogate，保留原始 bytes 引用
- [ ] renderer / TUI widget 文本参数签名改为只接受 `SafeText`（编译期断言）
- [ ] PTY 注入含 OSC/CSI 的 key/field/channel/`CLIENT LIST` 字段，断言无副作用
- [ ] `--bytes`/`--output resp` 直写 TTY 的 stderr 警告与 plain 模式拒绝

### 通过标准（验收）
类型检查阻止 `&str` 进 renderer；PTY 上无标题/超链接/剪贴板副作用

### 产出
`crates/pr-render/`、`tests/security/terminal-injection/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D06 · 秘密路径审计

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 2、7
**v2.1 依据** §23.4、§19.5、R24、R41、SEC-01
**依赖** V-A04、V-D01

### 验证什么
**秘密路径审计**：URI parse error、TLS error、`Debug` 派生、trace、history、clipboard、crash report、`--trace-wire` 十条路径注入含密码输入

### 方法
每路径注入已知 token 后全量 grep

### 任务
- [ ] 十条路径（URI parse error、TLS error、`Debug`、trace、history、clipboard、crash report、`--trace-wire`、diag bundle、telemetry-off 断言）各注入已知 token
- [ ] 全量 grep 断言 0 泄漏
- [ ] §19.5 只记字节数不记内容
- [ ] SEC-01 自动化

### 通过标准（验收）
0 泄漏

### 产出
`tests/security/secret-path/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D07 · 本地 policy 权威性

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 2A、3
**v2.1 依据** §11.2、R19、SEC-11、ASSIST-086
**依赖** V-A03、V-F01

### 验证什么
**本地 policy 权威性**：伪造 `COMMAND`/`COMMAND DOCS` 把写命令标 readonly

### 方法
synthetic server 返回伪造 metadata

### 任务
- [ ] V-A03 返回伪造 `COMMAND`/`COMMAND DOCS`（写命令标 readonly、危险命令无 flag）
- [ ] 断言本地 effects / 危险性 / 审批等级不变
- [ ] UI 标 `server reports` 而非改分类
- [ ] SEC-11、ASSIST-086

### 通过标准（验收）
本地 effects/策略不变；UI 标 `server reports`

### 产出
`tests/catalog/precedence/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D08 · MCP stdio bootstrap + 策略批准边界

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 6
**v2.1 依据** §29.4、R12、R46
**依赖** V-J02、V-D03

### 验证什么
**MCP stdio bootstrap + 策略批准边界**：stdout 只有 JSON-RPC；策略型批准只覆盖只读；`execute_approved_plan` 校验 `plan_hash`

### 方法
PTY-less 集成测试

### 任务
- [ ] `--mcp-stdio` bootstrap：不初始化 REPL/TUI/终端协调器，无 banner，缺凭证返回 JSON-RPC 错误
- [ ] 策略型批准只覆盖 `reads_data` + key 模式 + 次数上限
- [ ] `redis.execute_approved_plan` 校验人签 `ApprovalToken.plan_hash`
- [ ] PTY-less 集成测试断言 stdout 只有 JSON-RPC 帧

### 通过标准（验收）
stdout 无杂字节；写类请求无人签令牌被拒

### 产出
`crates/pr-mcp/tests/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-D09 · L2 插件外部进程隔离

`phase-0` `track-d` `prototype`

**Track** D · 安全与凭证
**类型** prototype
**排雷对象** Phase 7
**v2.1 依据** §30.4、R45
**依赖** V-A04

### 验证什么
**L2 插件外部进程隔离**：超时 kill、输出截断、崩溃/OOM 不影响主进程

### 方法
故意崩溃的插件

### 任务
- [ ] L2 外部进程插件协议（stdio typed 输入/输出）原型
- [ ] 超时 kill、输出大小截断、故意崩溃/OOM/死循环样本
- [ ] 主进程存活并退回 generic view 断言
- [ ] 确认 L1 声明式解释器的深度/大小/时间上限可精确执行

### 通过标准（验收）
主进程存活，退回 generic view

### 产出
`tests/plugins/`

### 不通过时
无回退（L1 声明式不需验证隔离）

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track E · 数据模型

### V-E01 · SPIKE-004 `pr-json` occurrence DOM

`phase-0` `track-e` `spike`

**Track** E · 数据模型
**类型** spike
**排雷对象** Phase 2、4、5
**v2.1 依据** §8.3、§8.5、R06、DATA-02/03/04、JSON-05
**依赖** V-J02、V-A06

### 验证什么
**SPIKE-004 `pr-json` occurrence DOM**：重复成员、大数、词法保留、`$.a#2`、增量重写；16 MiB 单值解析/格式化时间与内存

### 方法
property test + 16 MiB fixture

### 任务
- [ ] 实现 `JsonNode` occurrence DOM（span、lexeme、保序重复成员）
- [ ] 路径解析：`$.a#2`；重复成员无 `#n` 拒绝
- [ ] 增量重写只改目标 span；pretty 视图不回写
- [ ] property test（DATA-02/03/04、JSON-05）+ 16 MiB fixture 时间/内存；撰写 SPIKE-004

### 通过标准（验收）
全过；16 MiB 解析 + 格式化在 §24.3 时间预算内、峰值 ≤ 2× 输入

### 产出
`crates/pr-json/`、`docs/spikes/SPIKE-004.md`

### 不通过时
无回退（已确认 serde_json 不可用）

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-E02 · TableModel 流式宽度算法

`phase-0` `track-e` `prototype`

**Track** E · 数据模型
**类型** prototype
**排雷对象** Phase 2、4
**v2.1 依据** §6.1~6.3、UX-03/04/05/07
**依赖** V-A01、V-C06

### 验证什么
**TableModel 流式宽度算法**：采样冻结列宽、后续换行、极窄纵向块、每 field 横线、JSON cell 内不加分隔线

### 方法
golden × 5 宽度 × 3 色阶

### 任务
- [ ] TableModel：有限采样估计 field 宽 + 上下界，value 取剩余
- [ ] 流式：冻结当前片段列宽，后续换行；极窄 → 纵向块
- [ ] 每 field 横线、表头双线、JSON cell 内不加分隔线
- [ ] golden × 5 宽度 × 3 色阶（UX-03/04/05/07）

### 通过标准（验收）
全过

### 产出
`fixtures/renderer/table/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-E03 · 有界 result store + spool

`phase-0` `track-e` `prototype`

**Track** E · 数据模型
**类型** prototype
**排雷对象** Phase 2、4
**v2.1 依据** §24.3、§24.4、§30.5、R31、UX-10、LIFE-02
**依赖** V-A04、V-A06、V-J02

### 验证什么
**有界 result store + spool**：64 MiB/16 MiB 淘汰；显式 spool 私有权限、symlink/路径遍历防护、加密可选；`not retained` 标记；§8.2 阈值顺序

### 方法
超预算结果 + 崩溃后重启

### 任务
- [ ] 有界 result store：64 MiB 总量 / 16 MiB 单结果，按结果粒度淘汰并标 `not retained`
- [ ] 显式 spool：私有权限、不可预测名、symlink/路径遍历防护、可选加密
- [ ] §8.2 阈值顺序：已知长度首字节前询问；未知长度流式不中途询问；非 TTY 永不询问
- [ ] 崩溃后重启清理 + UX-10、LIFE-02

### 通过标准（验收）
淘汰正确；spool 权限正确；已知长度首字节前询问、未知长度不中途询问

### 产出
`crates/pr-results/tests/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-E04 · JSON projection v1

`phase-0` `track-e` `prototype`

**Track** E · 数据模型
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §18.3、R17、PIPE-02
**依赖** V-A02、V-B02

### 验证什么
**JSON projection v1** vs `redis-cli --json` 差异

### 方法
§18.3 表逐行 fixture

### 任务
- [ ] §18.3 projection 表逐行 fixture
- [ ] 与 V-A02 baseline `redis-cli --json` 差异测试
- [ ] 差异写入 `compatibility/manifest.toml`
- [ ] PIPE-02（`-2`/`-3`）

### 通过标准（验收）
每行一个用例过；差异写入 manifest

### 产出
`tests/output/json-projection/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-E05 · renderer generic fallback

`phase-0` `track-e` `prototype`

**Track** E · 数据模型
**类型** prototype
**排雷对象** Phase 2、5
**v2.1 依据** §9.5、CMD-10/12、SEC-07
**依赖** V-A03、V-B02

### 验证什么
**renderer generic fallback**：未知命令/模块结果不 panic 不重发

### 方法
synthetic 未知结构

### 任务
- [ ] V-A03 构造未知命令 / 模块 / 新版本结构结果
- [ ] 专用 renderer 不识别 → generic RESP tree → 保留原始 → 诊断 ID
- [ ] 断言无 panic、无空白输出、无重发
- [ ] CMD-10/12、SEC-07

### 通过标准（验收）
全过

### 产出
`fixtures/renderer/generic/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track F · 智能输入引擎

### V-F01 · catalog 编译管线

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2、2A
**v2.1 依据** §11.3、§11.4、R29
**依赖** V-I01、V-I02、V-I03

### 验证什么
**catalog 编译管线**：pinned snapshot → `CommandSpec`；Redis/Valkey 双分支；`unsupported grammar node` 标记；签名/校验和

### 方法
编译全部 baseline 命令

### 任务
- [ ] pinned upstream snapshot → schema adapter → normalized `CommandSpec`
- [ ] Redis / Valkey `server_family` 双分支
- [ ] 无法理解的节点标 `unsupported grammar node`；未知属性保留
- [ ] 签名 / 校验和；全部 baseline 命令 0 编译错 + 差异 fixture

### 通过标准（验收）
0 编译错；Redis vs Valkey 差异清单生成

### 产出
`crates/pr-catalog/`、`fixtures/catalog/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F02 · 双解析器等价性

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2、2A
**v2.1 依据** §11.5、R11、ASSIST-081
**依赖** V-F03

### 验证什么
**双解析器等价性** property test

### 方法
随机字节缓冲区 10⁶ 次

### 任务
- [ ] property test 生成随机字节缓冲区（转义、空参数、Unicode、NUL 转义、未闭合引号）
- [ ] 断言 `decode(B[span_i]) == argv_i`；未闭合区间正确
- [ ] 10⁶ 次无失败
- [ ] ASSIST-081

### 通过标准（验收）
span ↔ argv 一一对应；未闭合引号区间正确

### 产出
`crates/pr-intelligence/tests/equivalence.rs`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F03 · quoting corpus

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §11.5、§3.5、R04、R18
**依赖** V-I01

### 验证什么
**quoting corpus** vs 固定 `redis-cli` tag 的 `sdssplitargs`；本地 `:` 命令 tokenizer

### 方法
从 redis-cli 源码生成对照 corpus

### 任务
- [ ] 从固定 `redis-cli` tag 的 `sdssplitargs` 生成 quoting 对照 corpus
- [ ] 权威 tokenizer 100% 一致
- [ ] 本地 `:` 命令 tokenizer（POSIX-like quoting、`--`、无 shell 展开、尾部自由文本）fixture
- [ ] `LocalCommandSpec` 登记表与 §3.5 一一对应

### 通过标准（验收）
100% 一致；`:` 命令 fixture 过

### 产出
`fixtures/catalog/quoting/`、`tests/local-commands/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F04 · 语法复杂度 §11.7 全表

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2A
**v2.1 依据** §11.7、ASSIST-004~015
**依赖** V-F01

### 验证什么
**语法复杂度 §11.7 全表**：SET/ZADD/XADD/EVAL/XREAD/子命令/keyword 同形 key 的 signature 与候选

### 方法
每行 fixture

### 任务
- [ ] §11.7 每行一组 fixture：子命令 / 交替重复 / score-member / 互斥 / 嵌套 block / numkeys / 成对列表 / keyword 同形 key / 范围模式
- [ ] signature 高亮位置与候选集合断言
- [ ] ASSIST-004~015

### 通过标准（验收）
全过

### 产出
`fixtures/assistance/grammar/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F05 · 异步 broker

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2、2A
**v2.1 依据** §12.8、ASSIST-026~028、072
**依赖** V-J02

### 验证什么
**异步 broker**：revision/epoch 隔离、迟到丢弃、`CandidateId` 稳定、粘贴期间暂停

### 方法
模型测试（乱序事件序列）

### 任务
- [ ] buffer revision / target·identity·policy epoch 标签
- [ ] 迟到结果丢弃；`CandidateId` 稳定不被异步追加替换
- [ ] 粘贴期间暂停每字符分析
- [ ] 乱序事件序列模型测试（ASSIST-026~028、072）

### 通过标准（验收）
全过

### 产出
`tests/assistance-network/async/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F06 · `ObservationScope` 隔离

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2A、3
**v2.1 依据** §12.6、R13、ASSIST-029~033、088
**依赖** V-F05、V-J02

### 验证什么
**`ObservationScope` 隔离**含 topology epoch

### 方法
多 profile/identity/DB/拓扑并发 fixture

### 任务
- [ ] `ObservationScope` = profile_uuid + service_identity + db + auth_epoch + policy_epoch + topology_epoch
- [ ] 多 profile / identity / DB / 拓扑并发 fixture
- [ ] AUTH/HELLO/SELECT/RESET 与拓扑变化触发失效
- [ ] ASSIST-029~033、088；0 泄漏

### 通过标准（验收）
0 泄漏

### 产出
`tests/assistance-network/scope/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F07 · 显式发现在百万 key fixture

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2A
**v2.1 依据** §12.4、§12.5、R09、R32、ASSIST-041~045、084、085
**依赖** V-A05、V-F06

### 验证什么
**显式发现在百万 key fixture**：50 SCAN / 10 s 返回非空；prefix vs glob 模式；NOPERM 冷却；空页非零 cursor

### 方法
隔离 Redis 灌 10⁶ key

### 任务
- [ ] 隔离 Redis 灌 10⁶ key（稀疏与密集前缀各半）
- [ ] 显式发现默认预算（50 SCAN / COUNT 1000 / 10 s / 20 rps）返回非空并报告完成度
- [ ] 字面前缀转义 vs raw glob 两模式字节断言（`player:[`）
- [ ] NOPERM 冷却、空页非零 cursor（ASSIST-041~045、084、085）；预算不足则调高并记 ADR

### 通过标准（验收）
稀疏前缀有结果并报告完成度；`player:[` 两模式字节正确

### 产出
`tests/assistance-network/discovery/`

### 不通过时
若默认预算在 10⁶ key 上仍空 → 调高默认值（不是删功能）并记 ADR

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F08 · 用途搜索双集合

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2A
**v2.1 依据** §16.4、R10、ASSIST-050/051
**依赖** V-F01

### 验证什么
**用途搜索双集合**：canonical ≥95%、held-out ≥80%/90%；集合公开

### 方法
独立人员撰写 held-out 集

### 任务
- [ ] canonical 集：每命令 ≥ 3 条中/英规范表述（用于构建词表）
- [ ] held-out 集：独立人员撰写改写/口语表述，任一进入词表即移除
- [ ] 本地词法 + 同义词 + 中文 n-gram 检索原型
- [ ] canonical ≥95% top-3；held-out ≥80% top-3 / ≥90% top-5；集合入仓

### 通过标准（验收）
两行指标达标

### 产出
`fixtures/assistance/find/{canonical,heldout}/`

### 不通过时
未达标 → 扩词表后 held-out 集**换新**（旧集作废，防止泄漏）

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F09 · 12 MiB 堆增量实测

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2、7
**v2.1 依据** §12.5、§16.4、R08、R35
**依赖** V-A06、V-F01

### 验证什么
**12 MiB 堆增量实测**：catalog 加载后开/关提示 RSS 差

### 方法
V-A06 工具

### 任务
- [ ] catalog 加载后测「开/关提示」RSS 差（V-A06）
- [ ] 分列子项：key 名 / field 名 / metadata / scratch / parser 状态 / 发现响应
- [ ] 只读 mmap catalog 单独报告
- [ ] ≤ 12 MiB；超出先定位，预算调整需证据 + ADR

### 通过标准（验收）
≤ 12 MiB；mmap 单独报告

### 产出
`benches/assistance/`

### 不通过时
超出 → 先定位；预算调整必须带证据与 ADR

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-F10 · 零发送不变量

`phase-0` `track-f` `prototype`

**Track** F · 智能输入引擎
**类型** prototype
**排雷对象** Phase 2A
**v2.1 依据** §14.10、ASSIST-020、037、050~056、070
**依赖** V-F01、V-A01

### 验证什么
**零发送不变量**：断网下 F1/F2/F3/Guide/`:view`/`:copy`/主题切换/接受候选 全路径

### 方法
transport spy

### 任务
- [ ] transport spy 断言 0 条业务命令
- [ ] 断网下 F1 / F2 / F3 / Guide / `:view` / `:copy` / 主题切换 / 接受候选全路径
- [ ] `--learn` / `--demo` 离线
- [ ] ASSIST-020、037、050~056、070

### 通过标准（验收）
0 条业务命令

### 产出
`tests/assistance-network/zero-send/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track G · 连接拓扑

### V-G01 · Cluster 路由

`phase-0` `track-g` `prototype`

**Track** G · 连接拓扑
**类型** prototype
**排雷对象** Phase 3
**v2.1 依据** §21.2、R43、NET-03/04/05
**依赖** V-A05、V-A04

### 验证什么
**Cluster 路由**：hash tag、MOVED/ASK 同连接、重定向预算、CROSSSLOT 不拆分、部分节点不可达标 partial

### 方法
6 节点 Cluster + 重分片脚本

### 任务
- [ ] hash tag slot 计算 + 带 tag 的 MOVED/ASK fixture
- [ ] ASKING 与命令同连接；重定向链预算
- [ ] CROSSSLOT 不拆分；部分节点不可达标 partial
- [ ] NET-03/04/05 + 重分片脚本

### 通过标准（验收）
NET-03/04/05 过

### 产出
`tests/topology/cluster/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-G02 · Sentinel failover

`phase-0` `track-g` `prototype`

**Track** G · 连接拓扑
**类型** prototype
**排雷对象** Phase 3
**v2.1 依据** §21.4、NET-06
**依赖** V-A05、V-A04

### 验证什么
**Sentinel failover**：重新发现、未知写不重放、观察上下文清除

### 方法
强制 failover

### 任务
- [ ] 强制 failover 脚本
- [ ] 重新发现 endpoint、清除观察上下文、可能已发送写标 unknown 不重放
- [ ] `master_name` 变化视为新服务
- [ ] NET-06

### 通过标准（验收）
NET-06 过

### 产出
`tests/topology/sentinel/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-G03 · SSH per-node 与 SOCKS5

`phase-0` `track-g` `prototype`

**Track** G · 连接拓扑
**类型** prototype
**排雷对象** Phase 3
**v2.1 依据** §21.5、R15、NET-07
**依赖** V-A05、V-G01

### 验证什么
**SSH per-node 与 SOCKS5**：OpenSSH `-L`/`-D` 程序化控制、known-host、退出清理、TLS 校验内部名、Cluster-over-SSH

### 方法
bastion 容器 + Cluster

### 任务
- [ ] bastion 容器 + Cluster；OpenSSH `-L`（per-node，惰性、`max_tunnels`）与 `-D`（SOCKS5）程序化控制
- [ ] known-host 验证；TLS 校验内部真实主机名
- [ ] 跟随 MOVED 到非 seed 节点；退出只清理自建资源
- [ ] NET-07 + Cluster-over-SSH fixture；某模式某平台不可用则标 unsupported

### 通过标准（验收）
两模式都能跟随 MOVED；NET-07 过

### 产出
`tests/topology/ssh/`

### 不通过时
某模式在某平台不可用 → 该平台该模式标 unsupported，另一模式必须可用

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-G04 · TLS 矩阵

`phase-0` `track-g` `prototype`

**Track** G · 连接拓扑
**类型** prototype
**排雷对象** Phase 3
**v2.1 依据** §21.1、NET-01
**依赖** V-A05

### 验证什么
**TLS 矩阵**：错名/过期/未知 CA/client cert/SNI 独立/放松类 flag 拒绝

### 方法
自签 CA 生成各类证书

### 任务
- [ ] 自签 CA 生成：正常 / 错名 / 过期 / 未知 CA / client cert / 独立 SNI
- [ ] 放松类 flag 与 profile 同用报错
- [ ] 无静默 insecure
- [ ] NET-01

### 通过标准（验收）
NET-01 过；无静默 insecure

### 产出
`tests/topology/tls/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-G05 · Windows 凭证 + `LockFileEx` + ACL

`phase-0` `track-g` `prototype`

**Track** G · 连接拓扑
**类型** prototype
**排雷对象** Phase 2、7
**v2.1 依据** §30.2、§12.10、R20、R39、WIN-03、LIFE-03
**依赖** V-A05、V-D01

### 验证什么
**Windows 凭证 + `LockFileEx` + ACL**；共享文件 advisory lock 三平台

### 方法
多进程并发写

### 任务
- [ ] Windows Credential Manager 存取删
- [ ] `LockFileEx` advisory lock + `MoveFileEx(REPLACE_EXISTING)` 原子替换
- [ ] 文件 ACL 仅当前用户；POSIX 侧 `flock` + rename 同一套测试
- [ ] 4 进程并发写 profile（LIFE-03）+ WIN-03

### 通过标准（验收）
WIN-03、LIFE-03 过

### 产出
`tests/config/locking/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track H · 资源预算与性能基线

### V-H01 · 启动与空闲基线

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 2、4、7
**v2.1 依据** §24.6
**依赖** V-A06、V-A05

### 验证什么
**启动与空闲基线**：全部依赖链接后的骨架进程 `--help` p95、REPL 空闲 RSS、TUI 空闲 RSS

### 方法
V-A06 工具，固定机器

### 任务
- [ ] 链接全部计划依赖的骨架进程（不实现功能）
- [ ] 固定机器测 `--help` p95、REPL 空闲 RSS、TUI 空闲 RSS
- [ ] 超出则按 feature 裁依赖并重测；预算不变
- [ ] `benches/baseline/` 报告含硬件 / OS / commit / 依赖锁

### 通过标准（验收）
`--help` ≤100 ms；REPL ≤30 MiB；TUI ≤60 MiB —— **确认依赖本身没吃掉预算**

### 产出
`benches/baseline/`

### 不通过时
超出 → 换/裁依赖 feature；预算不变

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-H02 · 1 GiB synthetic blob 流式 + 真 Redis 512 MB 样本

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 1、5
**v2.1 依据** §24.7、PERF-01
**依赖** V-A03、V-B02、V-A05

### 验证什么
**1 GiB synthetic blob 流式 + 真 Redis 512 MB 样本**

### 方法
V-A03 + 真 Redis

### 任务
- [ ] V-A03 1 GiB blob 流式，记录峰值 RSS 与块预算
- [ ] 真 Redis 512 MB 上限样本（`proto-max-bulk-len` 默认）
- [ ] 两类来源分开报告
- [ ] PERF-01

### 通过标准（验收）
峰值 RSS 与块预算相符；两类来源分开报告

### 产出
`benches/protocol/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-H03 · 订阅风暴 ring buffer

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 5
**v2.1 依据** §24.3、PERF-02
**依赖** V-A03、V-B02

### 验证什么
**订阅风暴 ring buffer**

### 方法
每秒 10⁵ 消息

### 任务
- [ ] 每秒 10⁵ 消息订阅风暴
- [ ] ring buffer 10,000 条 / 16 MiB 先到者为准
- [ ] 淘汰计数与实际一致
- [ ] PERF-02

### 通过标准（验收）
有界；淘汰计数真实

### 产出
`benches/pubsub/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-H04 · 任务监督 leak detection

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 4
**v2.1 依据** §31.1、§31.4、R21、PERF-03
**依赖** V-J02、V-A06

### 验证什么
**任务监督 leak detection**：`TaskScope` drop 后任务计数回零；TUI 开关 1000 次

### 方法
V-A06 任务计数

### 任务
- [ ] `TaskScope`：`JoinSet` + `CancellationToken` + Drop 中 cancel → 有界等待 → `abort_all`
- [ ] V-A06 任务计数在 scope drop 后回零
- [ ] TUI 开关 1,000 次无增长
- [ ] PERF-03

### 通过标准（验收）
回零；无增长

### 产出
`tests/soak/tasks/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-H05 · 8h soak runner 跑通

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 7
**v2.1 依据** §32.1 Soak、PERF-04
**依赖** V-A05、V-A06

### 验证什么
**8h soak runner 跑通**（骨架功能）

### 方法
runner 上跑 8h

### 任务
- [ ] 8h soak job 用骨架功能跑通
- [ ] 指标采集（RSS / 任务数 / 分配 / 请求数）完整
- [ ] 无线性增长断言
- [ ] 为 24h release-soak 准备同一脚本

### 通过标准（验收）
指标采集完整；无线性增长

### 产出
CI soak job

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-H06 · rusqlite WAL 多进程并发写 + 0600 + `user_version` 迁移

`phase-0` `track-h` `prototype`

**Track** H · 资源预算与性能基线
**类型** prototype
**排雷对象** Phase 2
**v2.1 依据** §31.1、R22、R24
**依赖** V-A05

### 验证什么
**rusqlite WAL 多进程并发写 + 0600 + `user_version` 迁移**

### 方法
4 进程并发写 history

### 任务
- [ ] `rusqlite` `bundled` 固定版本 + WAL + `user_version`
- [ ] 4 进程并发写 history 无丢更新
- [ ] 文件 0600（Windows ACL）
- [ ] 前向迁移脚本测试

### 通过标准（验收）
无丢更新；权限正确；迁移前向

### 产出
`tests/history/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track I · 兼容矩阵、catalog 与许可

### V-I01 · `compatibility/manifest.toml` 冻结

`phase-0` `track-i` `freeze`

**Track** I · 兼容矩阵、catalog 与许可
**类型** freeze
**排雷对象** Phase 全部
**v2.1 依据** §20.4、R29
**依赖** 无

### 验证什么
**`compatibility/manifest.toml` 冻结**：§20.4 初始矩阵每项 image digest / tag 填入

### 方法
拉取并 pin

### 任务
- [ ] §20.4 初始矩阵每项 image digest / `redis-cli` tag / 模块版本填入 `compatibility/manifest.toml`
- [ ] 禁止 `latest`；每项可复现拉取
- [ ] manifest schema 校验进 CI

### 通过标准（验收）
无 `latest`；每项可复现拉取

### 产出
`compatibility/manifest.toml`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-I02 · Valkey vs Redis `COMMAND DOCS` 差异清单

`phase-0` `track-i` `fixture`

**Track** I · 兼容矩阵、catalog 与许可
**类型** fixture
**排雷对象** Phase 2、2A
**v2.1 依据** §20.3、R29
**依赖** V-I01、V-A05

### 验证什么
**Valkey vs Redis `COMMAND DOCS` 差异清单**

### 方法
双分支 introspection diff

### 任务
- [ ] Redis 与 Valkey 各 target 抓 `COMMAND` / `COMMAND DOCS`
- [ ] 生成差异清单（新增 / 缺失 / 参数差异 / 版本引入差异）
- [ ] 入 `fixtures/catalog/valkey-diff/`

### 通过标准（验收）
清单生成并进入 catalog fixture

### 产出
`fixtures/catalog/valkey-diff/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-I03 · catalog 再分发许可结论

`phase-0` `track-i` `audit`

**Track** I · 兼容矩阵、catalog 与许可
**类型** audit
**排雷对象** Phase 2、7
**v2.1 依据** §35.5、R29
**依赖** 无

### 验证什么
**catalog 再分发许可结论**（CAT-001 / ADR-023）：每个 snapshot 三选一（可打包 / 用户运行时下载 / 本仓独立撰写）

### 方法
逐来源审查

### 任务
- [ ] 逐 upstream 来源（Redis commands JSON、文档站、Valkey）审查许可
- [ ] 每 snapshot 三选一结论：可打包 / 用户运行时下载 / 本仓独立撰写
- [ ] `prc` 自身许可决定 → ADR-023
- [ ] `pr-catalog/LICENSES.md` + notices；不可再分发部分的独立撰写计划

### 通过标准（验收）
无空结论；`prc` 自身许可已定

### 产出
`docs/adr/ADR-023.md`、`pr-catalog/LICENSES.md`

### 不通过时
不可再分发的部分 → 本仓独立撰写并以 fixture 对照真实服务器

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-I04 · `redis-cli` baseline 特殊模式 inventory

`phase-0` `track-i` `freeze`

**Track** I · 兼容矩阵、catalog 与许可
**类型** freeze
**排雷对象** Phase 5
**v2.1 依据** §28.4、R36
**依赖** V-I01

### 验证什么
**`redis-cli` baseline 特殊模式 inventory**：从 help/source 生成 §28.4 的命令/flag/退出码清单

### 方法
解析固定 tag 源码

### 任务
- [ ] 解析固定 `redis-cli` tag 的 help/source，生成特殊模式 inventory（命令 / flag / 退出码）
- [ ] 与 v2.1 §28.4 契约表逐项对应
- [ ] 入 `compatibility/redis-cli-modes.toml`

### 通过标准（验收）
清单与 §28.4 契约表逐项对应

### 产出
`compatibility/redis-cli-modes.toml`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Track J · 契约冻结与 ADR

### V-J01 · ADR-001~030 全部写入并评审

`phase-0` `track-j` `freeze`

**Track** J · 契约冻结与 ADR
**类型** freeze
**排雷对象** Phase 全部
**v2.1 依据** §37.1
**依赖** 无

### 验证什么
**ADR-001~030 全部写入并评审**

### 方法
逐条评审

### 任务
- [ ] ADR-001~030 逐条撰写（context / decision / consequences / 关联 V 项）
- [ ] 评审并标 `accepted`
- [ ] 有争议者在 Phase 0 内决出

### 通过标准（验收）
30 条状态 `accepted`

### 产出
`docs/adr/`

### 不通过时
有争议 → 在 Phase 0 内决出，不带入 Phase 1

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-J02 · 冻结接口骨架

`phase-0` `track-j` `freeze`

**Track** J · 契约冻结与 ADR
**类型** freeze
**排雷对象** Phase 全部
**v2.1 依据** §11.11、§19.2、§21.3、§23.2、§3.5、§8.3、§12.9、§31.4
**依赖** V-J01

### 验证什么
**冻结接口骨架**：`CommandRequest`、`ExecutionOutcome`、`ResultRecord`、`CompletionRequest`、`CompletionCandidate`、`AssistanceSnapshot`、`TrustIdentity`、`ApprovalToken`、`LocalCommandSpec`、`JsonNode`、`SafeText`、`TaskScope`

### 方法
Rust 类型 + doc test + schema 版本号

### 任务
- [ ] `CommandRequest`、`ExecutionOutcome`、`ResultRecord`、`CompletionRequest`、`CompletionCandidate`、`AssistanceSnapshot`、`TrustIdentity`、`ApprovalToken`、`LocalCommandSpec`、`JsonNode`、`SafeText`、`TaskScope` 的 Rust 类型骨架
- [ ] 每类型 doc test + schema 版本号
- [ ] `docs/contracts/` 与代码同步

### 通过标准（验收）
编译通过；每个类型有 doc test

### 产出
`crates/*/src/contract.rs`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-J03 · 威胁模型

`phase-0` `track-j` `freeze`

**Track** J · 契约冻结与 ADR
**类型** freeze
**排雷对象** Phase 3、7
**v2.1 依据** §23、§32.1 Security
**依赖** V-J01

### 验证什么
**威胁模型**：每条信任边界对应至少一个 SEC 用例

### 方法
边界 → 用例映射表

### 任务
- [ ] 列出全部信任边界（输入 / parser / 配置 / 连接 / 错误 / trace / history / clipboard / export / crash / suggestion / metadata / plugin / MCP）
- [ ] 每条边界 → 至少一个 SEC 用例映射表
- [ ] 无未覆盖边界断言

### 通过标准（验收）
无未覆盖边界

### 产出
`docs/threat-model/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

### V-J04 · Phase 0 新增 ADR

`phase-0` `track-j` `freeze`

**Track** J · 契约冻结与 ADR
**类型** freeze
**排雷对象** Phase 全部
**v2.1 依据** §37.3
**依赖** V-B01、V-C01、V-C07、V-E01、V-C03、V-C04、V-C05

### 验证什么
**Phase 0 新增 ADR**：每个 spike 一条结论（SPIKE-001~004 + V-C03/C04/C05 的平台结论）

### 方法
—

### 任务
- [ ] SPIKE-001~004 各一条 ADR（采用 / 拒绝 / 回退）
- [ ] V-C03 / C04 / C05 平台结论各一条 ADR
- [ ] 全部标 `accepted`

### 通过标准（验收）
每个 spike 有 ADR

### 产出
`docs/adr/`

### 不通过时
无回退

### 状态
`IN-PROGRESS` → 结束时必须为 `PASS` 或 `FALLBACK-ADOPTED(ADR-xxx)`；不存在 DEFERRED / SKIPPED。

### 完成定义
- [ ] 产出已进仓库并在 CI 运行（无法自动化的部分有人工记录编号）
- [ ] 通过标准逐条有证据链接
- [ ] 若 FALLBACK：对应 ADR 已 `accepted`，回退方案跑同一套测试
- [ ] 在 Phase 0 报告中登记一行

---

## Meta

### P0-META-01 · Phase 0 追踪看板与报告

`phase-0` `meta`

**类型** meta

### 目标
维护 Phase 0 全部 63 个 V 项的状态与证据链接；产出 Phase 0 报告。

### 任务
- [ ] 建立看板：列 = `IN-PROGRESS` / `PASS` / `FALLBACK-ADOPTED`；不设 DEFERRED 列
- [ ] 每个 V 项一张卡，链接 issue、产出路径、ADR
- [ ] `docs/phase-0-report.md`：每 V 项一行 = 状态 + 证据链接
- [ ] 每周同步依赖阻塞（按 issue 中的「依赖」字段）

### 通过标准
63 行全部为 PASS / FALLBACK-ADOPTED 且每行有证据链接。

---

### P0-META-02 · Phase 0 出口门禁 checklist

`phase-0` `meta` `gate`

**类型** meta / gate

### 目标
执行 phase-plan §2.4 的 8 项出口门禁；全部勾选后才允许 Phase 1 合入 `main`。

### Checklist（来自 phase-plan §2.4）
- [ ] §2.3 全部 V 项状态 ∈ {PASS, FALLBACK-ADOPTED(ADR-xxx)}；无 IN-PROGRESS / DEFERRED / SKIPPED
- [ ] V-A05 CI 矩阵在 macOS / Linux / Windows 三 runner 全绿，含 Cluster、Sentinel、TLS
- [ ] 每个 harness（PTY、differential、synthetic server、fault）至少跑通 v2.1 §33 中属于自己类别的一条验收案例
- [ ] `benches/baseline/` 基线报告存档，含硬件 / OS / commit / 依赖锁
- [ ] 人工记录模板建立，V-C03 / C04 / C05 有首批真实终端记录
- [ ] `compatibility/manifest.toml` 无 `latest`
- [ ] ADR-001~030 + spike ADR 全部 `accepted`
- [ ] Phase 0 报告完成（P0-META-01）

### 通过标准
8 项全部勾选；由至少一名非执行者复核签字。

---

### P0-META-03 · 人工验证记录模板

`phase-0` `meta` `manual-verification`

**类型** meta

### 目标
为无法自动化的验证（系统 IME、屏幕阅读器、真实终端字体宽度、bracketed paste 实测）建立正式记录模板，作为发布门禁的一部分（v2.1 §32.6）。

### 任务
- [ ] `docs/manual-verification/TEMPLATE.md`：平台 / 终端 / 版本 / 字体 / IME、操作步骤、预期、实际、录屏链接、结论、记录人、日期
- [ ] 为 V-C03 / V-C04 / V-C05 各建一个记录目录
- [ ] 记录编号规则（`MV-<V项>-<平台>-<序号>`），可被 ASSIST / UX 案例引用
- [ ] 复核流程：第二人复现或审阅录屏

### 通过标准
模板落地；三个 V 项各有 ≥ 1 条完整记录。

---

