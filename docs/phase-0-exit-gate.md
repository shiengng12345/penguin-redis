# Phase 0 出口门禁（P0-META-02）

> 依据 `penguin-redis-phase-plan.md` §2.4。**八条全部满足才算通过；任一不满足，Phase 1 不得合入 `main`（G2）。**
>
> 这份文件不是打勾的地方。八条的判定在 [`ci/check-phase0-exit-gate.sh`](../ci/check-phase0-exit-gate.sh) 里，
> 每次运行都从仓库**重新推导**一遍。勾是某个人某一天做出的声称；脚本是今天的事实。

## 怎么跑

```bash
./ci/check-phase0-exit-gate.sh          # 快速：不跑 1 GiB 与 differential 重放
PHASE0_FULL=1 ./ci/check-phase0-exit-gate.sh   # 完整：连长用例一起跑
```

退出码 0 表示 Phase 0 可以关闭。非 0 会逐条说出缺什么。

## 八条，以及每条由什么判定

| # | §2.4 的条文 | 判定方式 | 能不能完全自动判 |
|---|---|---|---|
| 1 | 全部 V 项 ∈ {`PASS`, `FALLBACK-ADOPTED(ADR-xxx)`}，无 `IN-PROGRESS` / `DEFERRED` / `SKIPPED` | `PHASE0_FINAL=1 ci/check-phase0-report.sh` | 能 |
| 2 | CI 矩阵在 macOS / Linux / Windows 三 runner 全绿，含 Cluster、Sentinel、TLS | workflow 里三个 OS 与四个拓扑 job 必须存在；**结论**通过 `gh run list` 读 `main` 上最近一次运行 | 半：`gh` 不可用时报「未验证」，**绝不假定为绿** |
| 3 | 每个 harness 至少跑通 §33 中属于自己类别的一条验收案例 | 见下表，脚本**真的去跑** | 能 |
| 4 | `benches/baseline/` 基线报告存档，含硬件 / OS / commit / 依赖锁 | 逐字段检查，commit 与 `Cargo.lock` 摘要必须在表里 | 能 |
| 5 | 人工记录模板建立，V-C03 / C04 / C05 有首批真实终端记录 | 三份 MV 文档都要写明「首条已执行」，且 `records/` 非空 | 能 |
| 6 | `compatibility/manifest.toml` 无 `latest` | 先剥注释再查（文件自己解释了这条禁令） | 能 |
| 7 | ADR-001~030 + spike ADR 全部 `accepted` | `ci/check-adr-ledger.sh`：全部 accepted、**全部有非空结论小节**、索引双向一致、每个 spike 指向一条记录了它结论的 ADR | 能 |
| 8 | Phase 0 报告：每个 V 项一行 = 状态 + 证据链接 | 按 plan 的 V 项清单逐个查行，证据列不得为空或 `—` | 能 |

### 第 3 条的 harness × 案例对应

| harness | 案例 | 为什么是这条 |
|---|---|---|
| PTY | **ASSIST-082** | 真实 pty 上的一次 bracketed paste，进审阅视图且一条没跑 |
| synthetic RESP server | **PERF-01** | 1 GiB 的回复，**从不被materialise** —— 只有合成服务器能便宜地产生它 |
| fault injection | **STATE-01** | 请求在半途被切断，unknown-after-send 是**被制造出来**的，不是被模拟的 |
| differential | **CMD-01~05** | argv / 响应字节 / 最终状态 / 输出与退出码四层，对的是真的 `redis-cli` |

## 为什么第 2 条只能半自动

绿不绿是 GitHub 的事实，不在这个仓库里。脚本能做的是：确认 workflow **定义**了三个 runner 与
Cluster / Sentinel / TLS / SSH 的 job（少一个就是矩阵不完整），再通过 `gh` 去读 `main` 上最近一次
运行的结论。`gh` 不可用或未登录时，它打印 `????` 并**不计入通过**。

把「没查到」当成「绿」，是这份门禁唯一不能犯的错误。

## 复核

§2.4 要求「由至少一名非执行者复核签字」。脚本不替代这件事，它只是把可机器判定的部分变成可重放的：
复核者跑一次 `PHASE0_FULL=1 ./ci/check-phase0-exit-gate.sh`，读一次 `docs/phase-0-report.md`，
再对第 2 条的 CI 结论亲自看一眼。

| 字段 | 值 |
|---|---|
| 执行者 | （本仓库的 Phase 0 工作） |
| 复核者 | 待签 |
| 复核日期 | |
| 复核时的 commit | |
| 脚本输出 | 附在复核记录里 |
