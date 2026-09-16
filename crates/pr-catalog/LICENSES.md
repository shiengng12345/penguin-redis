# Catalog sources and redistribution (V-I03, CAT-001, ADR-023)

> v2.1 §35.5、R29。每个 upstream snapshot 必须登记来源、许可与三选一结论（**可打包** / **用户运行时下载** / **本仓独立撰写**）。**结论为空不得发布。**

## 这份文件回答的问题

`prc` 的二进制里嵌了一份命令目录。它从哪来、能不能跟着二进制一起分发。

答案取决于**嵌的到底是什么**，所以先说这个。

## 嵌入的是什么：接口结构，不是文档

快照由 `ci/catalog-snapshot.sh` 从一个 **digest-pinned 的运行中服务器**抓取，读的是 `COMMAND DOCS` 与 `COMMAND` 两条内省命令的回复。**不是** clone 上游仓库、**不是**抓文档站、**不是**复制 `src/commands/*.json`。

回复里有两类东西，性质完全不同：

| 类别 | 例子 | 是否嵌入 |
|---|---|---|
| **接口结构** | 命令名、arity、flags、ACL 分类、key spec、参数树（name / type / token / optional / multiple）、`since`、`group` | **是** |
| **上游撰写的散文** | `summary`（"Sets the string value of a key, ignoring its type…"）、`complexity`（"O(1)"） | **否** |

散文占原始回复的 16%，`ci/catalog-normalise.py` 在归一化阶段直接丢弃，`crates/pr-catalog/src/snapshot.rs` 的 schema 里根本没有这两个字段。因为那些 struct 是 `deny_unknown_fields`，**将来某次抓取若重新带上它们，快照会加载失败**而不是悄悄把没人审过许可的文本分发出去。

守住这条的测试：`crates/pr-catalog/tests/grammar_table.rs::the_snapshot_carries_interface_structure_and_no_upstream_prose`。

## 逐来源结论

| 来源 | 实际用到的东西 | 许可 | 结论 |
|---|---|---|---|
| Redis 8.0.6 服务器（`redis@sha256:ae47…`）的 `COMMAND DOCS` / `COMMAND` 回复 | 接口结构 | 服务器本体为 RSALv2 / SSPL / AGPLv3（8.0 起）；**回复内容是运行时输出，不是被分发的源码** | **可打包**（仅结构） |
| Valkey 8.1.10 服务器（`valkey/valkey@sha256:3fbd…`）的同两条命令 | 接口结构 | BSD-3-Clause | **可打包**（仅结构） |
| Redis `src/commands/*.json` 源文件 | **未使用** | 随服务器许可 | 不适用——从未取用 |
| Redis 文档站 / `redis-doc` 仓库 | **未使用** | CC-BY-SA-4.0 | 不适用——从未取用。这是刻意的：CC-BY-SA 的 copyleft 会传染到我们自己写的说明 |
| Valkey 文档站 | **未使用** | — | 不适用 |
| 命令说明文字（`summary` / `complexity`） | **本仓独立撰写**（尚未写） | — | **本仓独立撰写** |

### 为什么「回复内容」与「源文件」要分开算

这不是文字游戏，是两件不同的事：

- 复制 `src/commands/set.json` 是在分发上游仓库里的一个文件。
- 记录「跑 `COMMAND DOCS` 会得到什么」是在记录一个已发布协议的**可观察行为**——就像记下 HTTP 状态码的含义。任何客户端要正确工作都必须知道这些，而且任何人跑一次服务器就能重新得到同一份数据（`ci/check-catalog-snapshots.sh` 每次 CI 都在重做这件事，并要求逐字节一致）。

**这是工程判断，不是法律意见。** 我不是律师。这里记录的是可核实的事实——数据取自哪里、包含什么、不包含什么、如何复现——以及据此做出的工程决定。发布前需要一次人工法务确认，见下方「待办」。

## 「本仓独立撰写」的部分：计划

命令说明文字要自己写。这不是负担，是本来就该做的事：

- 上游的 `summary` 是为参考手册写的（"Sets the string value of a key, ignoring its type."），不是为卡在提示符前的人写的。§13 要的是后者。
- 独立撰写的说明可以带上 Penguin 才有的信息：这条命令在当前 profile 的策略下会不会被拒、它的效果分类是什么、取消它意味着什么。

**核对方式**（V-I03 回退条款要求的「以 fixture 对照真实服务器」）：说明文字是我们的，但它描述的**结构**逐条对照真实服务器——`ci/check-catalog-snapshots.sh` 已经在做，`crates/pr-catalog/tests/grammar_table.rs` 的 §11.7 全表也在做。一条说明若与结构矛盾（比如说某命令接受一个它其实不接受的选项），语法表的测试会先失败。

落点：`crates/pr-catalog/src/descriptions.rs`（Phase 2）。在它存在之前，`CommandSpec::summary` 为空字符串——**明显缺失，而不是悄悄借用**。

## 二进制里还有什么第三方内容

`cargo deny check licenses` 在 CI 每次运行，只允许宽松许可（MIT / Apache-2.0 / BSD / ISC / Unicode-3.0 / CC0 / Zlib）。见 `deny.toml`。

唯一的 advisory 例外是 `RUSTSEC-2024-0436`（`paste` 不再维护），在 `deny.toml` 里带理由与退出条件单条列出，不是一句 `unmaintained = "allow"` 把以后的告警一起盖掉。

## 待办（发布前）

| 项 | 状态 |
|---|---|
| `prc` 自身许可证 | **已定：`MIT OR Apache-2.0`**（ADR-023）。可逆——未发布，改一处即可 |
| 人工法务确认上述「接口结构 vs 散文」的划分 | 未做。发布门禁项，不阻塞 Phase 1 |
| `NOTICE` 文件与 SBOM | Phase 7（§35.5） |
| `descriptions.rs` 独立撰写的说明 | Phase 2 |
