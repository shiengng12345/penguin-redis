# 进度总览

> 最后更新：2026-09-16
> 这份文件回答三个问题：**现在在哪、还差什么、为什么这么慢**。
> 逐项的状态与证据在 [`phase-0-report.md`](phase-0-report.md)；这里是给人看的那一层。

---

## 一、整体位置

| Phase | 内容 | 进度 | 说明 |
|---|---|---:|---|
| **Phase 0** | 前置验证（63 V 项 + 3 meta） | **95.4%** | 见下 |
| Phase 1 | E1 Native kernel | 0% | Phase 0 未关闭前不得合入 `main`（规则 G2） |
| Phase 2 | E2 Daily CLI + Assistance | 0% | — |
| Phase 2A | E2A Deep intelligence | 0% | — |
| Phase 3 | E3 Production topology | 0% | — |
| Phase 4 | E4 Deep TUI | 0% | — |
| Phase 5 | E5 Operational suite | 0% | — |
| Phase 6 | E6 Automation | 0% | — |
| Phase 7 | E7 Product hardening | 0% | — |
| Phase 8 | E8 Studio integration | 0% | — |

**Phase 0 不是「最难的一段」，是「最短的一段」。** 它不写功能，它把 Phase 1–8 会撞上的墙先撞一遍。
今天 `prc` 能做的事只有：`--help` / `--version` / `--diagnostics` / `--probe-width` / `--mcp-stdio`，
以及一条给 differential 比对用的透传。`--help` 里列出的 `--json`、`--output`、`--trace-wire`
大多**还没有实现** —— 它们是 Phase 0 冻结下来的**契约**，Phase 1–3 才去兑现。

---

## 二、Phase 0 明细

| 状态 | 数量 |
|---|---:|
| PASS | **60** |
| FALLBACK-ADOPTED（各有 accepted ADR） | 2 |
| IN-PROGRESS | **1** |
| BLOCKED | 0 |
| **V 项合计** | **63**（终态 62 / 63 = **98.4%**） |
| P0-META | 1 / 3 |
| **总计** | **61 / 66 = 92.4%** |

### 唯一未完成的 V 项

| 项 | 状态 | 卡在哪 |
|---|---|---|
| **V-H05** 8 小时 soak | IN-PROGRESS | **纯时间**。进程在本机跑，进度 2.6h / 8h，一切平稳（堆 85 KiB、RSS 3.5 MiB、活跃任务 0、命令数 338 万且在涨）。跑完后填 `benches/soak/README.md`、登记、关闭。 |

### 两个 meta 项

| 项 | 状态 | 卡在哪 |
|---|---|---|
| P0-META-01 报告 | IN-PROGRESS | 等 V-H05 登记完 |
| P0-META-02 出口门禁 | IN-PROGRESS | 等前两条；**门禁脚本本身已全通**（见下） |

### §2.4 出口门禁八条

演练过一次（把剩余三行临时标成 PASS 再跑完整门禁）：**八条全绿**。所以收尾是机械的。

| # | 条目 | 现状 |
|---|---|---|
| 1 | 全部 V 项 ∈ {PASS, FALLBACK-ADOPTED(ADR-xxx)} | ⏳ 等 V-H05 |
| 2 | 三平台 CI 全绿，含 Cluster / Sentinel / TLS | ✅ |
| 3 | 四个 harness 各跑通一条 §33 验收案例 | ✅（含 1 GiB 与 differential 重放） |
| 4 | `benches/baseline/` 含硬件 / OS / commit / 依赖锁 | ✅ |
| 5 | 人工记录模板 + V-C03/C04/C05 首批真实终端记录 | ✅ |
| 6 | `compatibility/manifest.toml` 无 `latest` | ✅ |
| 7 | 全部 ADR `accepted` 且有结论小节 | ✅（39 条） |
| 8 | 报告每项 = 状态 + 证据链接 | ⏳ 等 V-H05 |
| +1 | （新增）计划点名的 45 条验收案例全部被认领 | ✅ |
| +2 | （新增）计划承诺的 70 条产出路径全部可达 | ✅ |

---

## 三、这一轮抓到的东西

Phase 0 的产出不是功能，是**本该在 Phase 2、3 才爆的返工**。按类别列：

### 「一直是绿的，但什么都没测」

| 发现 | 实际情况 |
|---|---|
| 7 个 server matrix job | 跑 `-E 'test(/live_/)' --no-tests=pass`，而 workspace 里一个 `live_*` 测试都没有 —— 七个容器起来、被丢掉、七个绿勾。这也是 manifest 里每一行 `status = "unknown"` 的原因。 |
| `safety` job | 过滤器只匹配到**一个**测试，还是个语法解析测试，而 job 名字叫 "safety (secrets, injection, supply chain)"。 |
| `differential` job | 录制代理绑 `127.0.0.1`，容器里的 `redis-cli` 根本连不上 —— **在 Linux runner 上从来没通过过**，四层比对全是「未登记差异」。 |
| 五处 `-- --ignored` | `cargo test -- --ignored` 在没有 ignored 测试时退出 0。现在没跑到任何测试就算失败。 |

### 「记成 PASS，其实只做了一部分」

| 发现 | 实际情况 |
|---|---|
| **V-D06** | 状态栏写的是 `PASS（history 路径）` —— 规格要十条秘密路径各注入 token 后全量 grep，实际只做了一条。全角括号后面的字**对机器隐形、对人明写**；门禁的正则停在 ASCII。现已补齐十条并加了状态栏校验。 |
| 台账总数 | 同一个正则漏掉了那一行，**汇总一直显示 62，而计划有 63 项**。 |
| V-C07 | `SetConsoleCtrlHandler` 记为「未接线」，理由是等 ADR-023 的 crate 决定 —— 那个决定两项之后就做了，理由早就过期。已接线。 |
| V-J03 | 威胁模型明写四条边界「还不存在所以先不列，落地时靠人回来补」。四条都落地了。已补。 |
| assistance 堆预算 | 报了 8672 KiB，其实**量的是空气** —— `prc` 没有任何路径调用 `Finder::new()`，链接器把整张表丢了。真实是 9296 KiB（预算 12288）。 |

### 平台与真实设备

| 发现 | 实际情况 |
|---|---|
| Windows DACL | `ConvertSidToStringSidW` 导入错模块 —— 三个 Windows job 一直编译不过。 |
| Windows 权限检查 | 拿 SID 字符串比 DACL，而 Windows 会把内置账户渲染成别名（`LA`），于是**把自己刚设好的权限判成不安全**。 |
| 「原子替换」测试 | 用创建时间证明文件被替换，而 NTFS 的 file tunneling 会还原创建时间 —— 测试对着它要证明的操作报「就地改写」。改成**握住一个打开的 reader 跨越更新**，直接证明协议承诺的那件事。 |
| `prc.exe` | 两处查找可执行文件不带 `EXE_SUFFIX`，MCP 的 9 个测试在 Windows 上全报「prc is not built」。 |
| `prc --probe-width` | crossterm 把 `CSI 6 n` 的回复当**内部事件**吃掉了 —— 宽度探测**在真实终端上从来没工作过**。用一个真实 kitty 窗口跑出来的。 |
| valkey 8.x | **有 `HSCAN NOVALUES`，没有 hash-field TTL**。两个特性在 Redis 7.4 同时到达，valkey 只拿了一个 —— 任何「7.2 之后都有」的规则在这里都是错的。ADR-019 的字段发现路径依赖这件事。 |

### 泛化能力

**V-F08 用途搜索**是这一轮最长的一条线，也是结论最硬的一条：

| 时刻 | 独立语料 top-3 |
|---|---:|
| 第一份独立语料，首次接触 | 67.3% |
| 用它的 miss 扩词表后，**在同一份语料上** | 86.5% |
| 用它的 miss 扩词表后，**在一份新语料上** | **69.6%** |
| 换机制之后（概念 + 语料双路，RRF 融合） | **83.2% / 84.7%** |

**一轮扩表的 +19.2 个百分点里只有 2.3 个是泛化**，其余是记住了那一份语料的措辞。这是两个数相减，
不是推断。ADR-034 原来把不收敛归咎于「四份语料同一个作者」——那个解释被自己的数据排除了，
剩下的只能是机制。**门槛一个字没改，改的是实现**（ADR-039）。

最终：**独立 held-out top-3 83.9% / top-5 90.8%**，790 句，来自**八位撰写人**，
每一位都被要求不打开仓库里任何文件。第三份语料在机制定稿后才写、只测一次、之后没改任何东西。

---

## 四、进行中的后台作业

| 作业 | 状态 | 怎么停 |
|---|---|---|
| 8 小时 soak（V-H05） | 本机进程 **PID 88950**，写 `/tmp/soak-8h.jsonl`，2.6h / 8h | `kill 88950`（会丢掉已跑的 2.6 小时，V-H05 得重跑） |
| soak 的 Redis | 容器 `prsoak-redis` | `docker rm -f prsoak-redis` |
| 夜间 soak workflow | 每天 03:00 UTC，5 小时 | GitHub → Actions → Soak → 关掉，或改 cron |

**关于邮件噪音**：今天推了约 20 次，每推一次就是一次 CI 运行。已经改成同 ref 取消旧运行，
并且从现在起按「一个完成的项目推一次」批量提交。如果仍然不想收：
GitHub → 本仓库 → Watch → Custom → 取消勾选 **Actions**。

---

## 五、接下来（按顺序）

1. **等 soak 跑完**（约 5.4 小时）→ 填 `benches/soak/README.md` 的结果小节 → 登记 V-H05。
2. **重测一次基线**（`benches/baseline/`），这次在空闲机器上，把被 soak 污染的那一列换掉。
3. **把 `compatibility/manifest.toml` 的 `status` 从 `unknown` 改成 `verified`**，附
   `verified_topologies` 与 `verified_ci_run` —— 门禁现在要求这两项，因为 valkey 声称支持
   cluster 而这里从来只跑 standalone。
4. **更新 V-A05 的证据**：它引的 CI run 是旧的，而那次「17/17 全绿」里有三个 job 是空的。
5. 关闭 **P0-META-01**（报告）与 **P0-META-02**（出口门禁），跑一次 `PHASE0_FINAL=1`。
6. **Phase 0 关闭，Phase 1 才可以开工。**

---

## 六、给复核者

```bash
./ci/check-phase0-exit-gate.sh              # 八条门禁，从仓库重新推导
PHASE0_FULL=1 ./ci/check-phase0-exit-gate.sh   # 连 1 GiB 与 differential 重放一起跑
```

门禁不替代 §2.4 要求的「至少一名非执行者复核签字」。它只是把可机器判定的部分变成可重放的。
