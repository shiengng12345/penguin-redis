# 显式发现在百万 key 上的实测（V-F07，v2.1 §12.4、§12.5、R08、R09、R32）

状态机在 `crates/pr-intelligence/src/discovery.rs`；纯逻辑 19 个测试在 `crates/pr-intelligence/tests/discovery.rs`（无需服务器，常驻）；实测 5 个测试在 `crates/pr-intelligence/tests/discovery_live.rs`（Docker + 10⁶ key，CI 以 `--ignored` 跑）。预算调整见 [ADR-033](../../../docs/adr/ADR-033.md)。

## fixture

`DEBUG POPULATE 1000000` 生成 `key:0` … `key:999999`，再放 10 个 `player:{i*7919}` 作为 needle——按名字散开而不是聚成一簇，因为聚在一起的集合要么整块被找到要么整块被漏掉，说明不了什么。`--enable-debug-command yes` 只在这个一次性容器里开。

## 实测数字

| 预算 | 停在哪 | 用时 | SCAN 次数 | 命中 needle |
|---|---|---:|---:|---:|
| §12.5 原始（50 次 SCAN） | **SCAN 次数** | 2.58 s | 50 | 2 / 10 |
| ADR-033 调整后（200 次） | **时间** | 10.0 s | 189～190 | 1～2 / 10 |
| 走到底（无上限） | cursor 归零 | 0.39 s | 1000 | **10 / 10** |

原始默认**不是空的**，V-F07 的通过标准（稀疏前缀有结果并报告完成度）满足。但它在 10 秒预算里只用掉 2.58 秒——卡住它的是 SCAN 次数而不是用户实际感受到的那个限制。50 × 20 req/s = 2.5 s，而 10 s × 20 req/s = 200。见 ADR-033。

## needle 命中数不是断言对象

走到 19% 时，10 个 needle 的期望命中约 2 个；一次跑出 1 个不是回归。断言的是**确定量**：SCAN 次数 > 50、用满时间预算、每个命中确实以 `player:` 开头、时间预算真的封住了运行。§32.5 说 flaky 测试要定位而不是靠重跑掩盖——一条断言在硬币上的测试正是那种。

## 两种匹配模式，对着真服务器验

§12.4 给了一张表，`player:[` 是那个会暴露问题的输入：

| 模式 | 发送的 MATCH | 对 `player:[1]` |
|---|---|---|
| 字面前缀 | `player:\[*` | **命中** |
| 原始 glob | `player:[` | 不命中（未终结的字符类，由服务器判定） |

两边结果不同，这正是「不存在无声变成另一个匹配条件的路径」的可验证含义。转义是**逐字节**的，不是逐 char——Redis 的 key 是字节，未必是 UTF-8。

## 空结果永远不说「key 不存在」

§12.4 明文禁止。客户端的预算说明不了服务器上有什么，而一个被告知「不存在」的用户会停止寻找。这条由类型表达：只有 `Completeness::Exhausted`（cursor 归零）的 `is_conclusive()` 为真。其余四种——SCAN 次数、时间、响应大小、取消、被拒——的空结果都说 `discovery incomplete`，并且附带停在哪个预算上。

有命中时同样要说覆盖了多少：看到三条就以为是全部、因为没人说可能还有更多的用户，是被遗漏误导的。

## SCAN 的真实保证，逐条兑现

- **空页 + 非零 cursor 不结束扫描**（[R08]）。在这里停下，就会对一个存在的 key 报「找不到」。
- **同一个元素可能返回多次**。重复的显示一次、计数一次，但 `keys_seen` 仍如实记录收到过几个。
- **`COUNT` 是提示**，不是页大小，也不是进度。没有任何地方从 cursor 值推算百分比。

## NOPERM 冷却（ASSIST-084）

对着真实 ACL 验的，不是模拟的错误——要测的是「服务器真发过来时我们认得出」。`ACL SETUSER noscan ... -scan` 之后 SCAN 返回 `NOPERM User noscan has no permissions to run the 'scan' command`，发现据此退避 60 秒，作用域是 (profile, database)——ACL 规则的作用域就是这个，全局冷却会连累一个本来允许扫描的 profile。

**人可以手动清掉冷却**：它是用来阻止自动重试撞 ACL 墙的，不是用来跟一个刚改好权限的人争论。

## 还没做的

- **自动发现预算的实测**（每 profile ≤ 2 请求/秒、≤ 2 次 SCAN、≤ 1 秒）：数量级上刻意比显式小得多，有不变量测试守住这个差距，但没有在 10⁶ key 上单独跑过。归 Phase 2。
- **Cluster 上的发现**：§21.2 要求维护 `{node_id, cursor, topology_epoch}` 且不输出伪全局 cursor。归 Phase 3。
- **field/member 级发现**（`HSCAN NOVALUES` 与其降级路径）：归 V-F07 的兄弟项与 ADR-019，Phase 2。
