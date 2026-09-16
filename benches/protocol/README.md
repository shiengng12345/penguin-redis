# 大值解码基线（V-H02 / PERF-01，v2.1 §24.7）

由 `crates/pr-protocol/tests/large_values.rs` 测出。这里记的是人读过的数字，以便以后的回归是一次 **diff** 而不是一场争论。

## 为什么是两个来源，而且必须分开报

§24.7 写得很清楚：

> 官方字符串文档列出默认最大字符串大小为 512 MB。[R52] 因此 1 GiB 单 blob 的测试默认使用 synthetic RESP fixture server，验证解码/输出的增量行为；真 Redis 测试按固定版本和实际配置的数据上限生成，**不把调大服务器限制或 mock 结果冒充默认真实部署**。

所以 1 GiB 那一栏不可能来自一台原装 Redis。把 `proto-max-bulk-len` 调大去造一个，测的就是没人在跑的部署。两个来源回答的是两个问题：

- **synthetic**：解码器过了 1 GiB 还是不是增量的？
- **真 Redis**：面对一台真服务器、在它自己的文档上限上，还是不是？

哪一个都替代不了另一个，把两个数合并只会让人看不出问到的是哪个问题。

## 2026-09-16 — Apple M3, macOS 26.5.2 (aarch64), rustc 1.97.1, debug build

| 来源 | 声明长度 | 交付字节 | socket 读取块 | **峰值持有** | 进程 RSS 变化 |
|---|---:|---:|---:|---:|---:|
| synthetic RESP server | 1 073 741 824 B (1 GiB) | 1 073 741 824 B | 64 KiB | **65 536 B (0.06 MiB)** | +0.22 MiB |
| 真 Redis 7.4.11（原装配置上限） | 536 870 912 B (512 MB) | 536 870 912 B | 64 KiB | **65 536 B (0.06 MiB)** | +0.30 MiB |

真 Redis 镜像：`redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7`（与 `compatibility/manifest.toml` 的 `baseline_cli` 同一 digest）。512 MB 字符串用 `SETRANGE vh02:big 536870911 x` 造出来——服务器保持原装配置，没有调高任何上限。

## 被测量的是「峰值持有」，不是耗时

§24.3 的预算讲的是内存。一个「很快」的解码器如果是因为把整个 GiB 缓了下来才快，正是这个测试要抓的失败。所以断言写成：**持有量必须跟 socket 读取块走，而不是跟值的大小走**（上限取两个读取块，因为一块在排队时另一块可能已经到达）。

两行都是 65 536 B = 恰好一个读取块。值大了 16 倍（512 MB → 1 GiB），持有量一个字节都没动——这才是「增量」的可检验含义。

## 16 MiB 的持有上限没有被绕过

同一份字节，两个 API 会给出两种正确而不同的答案：

| API | 一个 17 MiB 的 bulk | 依据 |
|---|---|---|
| `Decoder`（值） | `Err(Budget("bulk"))` | §24.3：单结果完整内存保留 16 MiB |
| `Streamer`（流式） | 开始交付 | §24.4：能流式输出就继续，不无限扩容 |

这不是两套语义，是一套语义的两半。`crates/pr-protocol/tests/stream_equivalence.rs` 把 V-A03 的 **316 个 fixture 全部**同时喂给两个 API，要求值一致、**错误也一致**，并且允许的差异**只有一种**：值 API 因 `Budget("bulk")` 拒绝、而声明长度确实超过 16 MiB 时，流式 API 可以继续。语料里正好 4 条属于这种情况，数字被冻结；其余 312 条完全一致。

而且这个等价测试还跑第二遍，把 `stream_above` 设成 0，强迫**每一个** `$` / `!` / `=` 都走流式路径——包括畸形的和截断的。一个谁也没跨过的阈值证明不了任何事。

### 这个等价测试抓到了一个真 bug

`~?`（streamed set）在值解码器里被解成了 `Value::Array`：streamed 分支只区分了 map 与非 map，把 tag 丢了。后果不止是枚举变体错了——一个 streamed **push**（`>?`）同样会变成 `Array`，而 `is_push()` 正是 §19.3 用来阻止 push 占用下一条命令回复槽的那个方法。

修复前先加了复现用例（§32.5），见 `crates/pr-protocol/src/lib.rs` 的 `a_streamed_aggregate_keeps_the_type_its_tag_declared` 与 `a_streamed_push_never_answers_a_command`。

## 还没测的

- **巨大 aggregate、大 key 名、巨大单 field、超长 command input**——§24.7 明说它们的内存路径不同，要另外测。归属 Phase 1。
- **流式输出到终端/文件的背压**（§24.5）。本文件测的是解码侧到 `ScalarChunk` 为止；消费侧的预算是 `pr-results` 的事。
