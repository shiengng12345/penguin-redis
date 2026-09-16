# SPIKE-002 · `redis-rs` 的低层 API 能不能当 kernel 的协议层

- **V 项**：V-B01
- **依据**：v2.1 §31.2、§37.3
- **被测版本**：`redis` 1.7.0（`default-features = false`, `features = ["tokio-comp"]`）
- **结论**：**不能**。采用本 ADR 预先命名的回退——`pr-protocol` 自研 codec 为唯一 kernel 边界，`redis-rs` 降为互操作测试对象。见 [ADR-031](../adr/ADR-031.md)。
- **可复现证据**：`crates/pr-protocol/tests/spike_002_redis_rs.rs`（19 个测试，全绿）

## 这份 spike 怎么做的

§31.2 列了六条门槛。本 spike 不读文档下结论，而是把 **V-A03 的 316 个 fixture 全部**同时喂给两个 decoder，逐条比对，并把结果**冻结成断言**。

冻结是关键。一个写在 md 里的结论会随版本升级悄悄过期；一个写成 `assert_eq!` 的结论在 `redis-rs` 变化的那一刻就会让 CI 红，逼人回来重读 ADR-031。

## 全语料比对（316 个 fixture）

| 类别 | 数量 | 含义 |
|---|---:|---|
| **一致** | 271 | 两边同样接受且无信息丢失，或两边同样拒绝 |
| **有损接受** | 10 | 两边都接受，但 `redis-rs` 的值已经不再承载线上说过的话 |
| **未实现** | 28 | 我们能解，`redis-rs` 解不了。**28 条全部是 RESP3 streamed 类型**（测试里单独断言了这一点） |
| **过度接受** | 5 | 我们判协议错误，`redis-rs` 当成正常值交给上层 |
| **超出我方预算** | 2 | 我们按 budget 拒绝，`redis-rs` 用它自己的固定上限接受。不是缺陷，是所有权问题 |

`beyond_our_budget` 单独一列，是为了不把「限额不同」混进「过度接受」里充数。

## 逐条对照 §31.2

### 1. 是否完整保留需要的 RESP frame/type —— **否**

| 线上的东西 | `redis-rs` 1.7.0 得到什么 | 后果 |
|---|---|---|
| `$-1` / `*-1` / `~-1` / `_` | 全部 → `Value::Nil` | 哪一种 null 到达的信息消失。ADR-005 要求 view 不改变类型；`--output resp` 无法忠实回写 |
| `,1.2300` | `Value::Double(1.23_f64)` | **词法丢失**。§18.3 明确要求保留服务器选的那串字符 |
| `,1e999` | `Value::Double(inf)`，且返回 `Ok` | 不是报错，是换了个数给用户 |
| `, 1.23 ` | `Ok(1.23)` | `line.trim()` 让一个非法帧变正常 |
| `=N txt:<非 UTF-8>` | `String::from_utf8_lossy` → `U+FFFD` | **静默损坏**，没有任何告知。ADR-005 的存在就是为了防这个 |
| `!N ERR <非 UTF-8>` | 同上，错误文本被改写 | §23.5 要求 `SafeText` 承载服务器原字节 |
| `+<非 UTF-8>` | 整帧硬错误 | 在连接层等于断连 |
| `+OK` | `Value::Okay`（独立 variant） | 本身无害；但说明帧层已经在做解释，重新编码必须特判 |
| `>0` / `>-1` | `Push { kind: Other(""), data: [] }` | **凭空造出一个 push**。push 恰恰是最不能被误认成回复的那种帧 |
| `>1 +x` | 唯一元素被吃成 `PushKind`，`data` 空 | 元素若非 UTF-8 字符串则整帧硬错误 |
| `$?` / `*?` / `%?` / `~?` / `;N` / `.` | dispatch 表里**根本没有** | 见下 |

保留得好的也记下来，否则这不叫比较：map 的顺序与重复 key 保留（是 `Vec<(Value, Value)>` 不是 HashMap）、big number 保留为数字串、attribute 在此层保留、bulk string 二进制安全（含 NUL）、`-0` 因为 Rust `f64` 的 Display 恰好写回 `-0`。

### 2. 增量解析 —— **否（最要命的一条）**

`redis-rs` 1.7.0 的 parser 里 `$?`、`;`、`.`、`streamed` 的出现次数都是 **0**。RESP3 streamed 类型不在 `combine::dispatch!` 的分支里，`$?` 会走进整数解析得到 "Expected integer, got garbage"。

这正是 §31.2 最在意的一条：**streamed 类型是协议自己用来说「这东西装不进内存」的方式**。解不了它，就只能等整帧到齐，1 GiB blob 也一样——而 §24.7 的预算是按块算的。

官方文档说的「用了 iterator 就是流式」在这里被 fixture 直接证伪：普通 bulk 的 iterator 仍然要完整的 `Vec`（R27 已经写明），而 `SCAN` iterator 是在发后续查询，两者都不是帧级流式。

### 3. 大小与复制行为 —— **否**

- `Value::BulkString(Vec<u8>)` 是从读缓冲 `bs.to_vec()` 出来的**独立副本**，每次 `clone` 再复制一份。512 MB 的值要付读缓冲 + 值 + 每个 clone。`pr-protocol` 交出 `Bytes`，是已存在缓冲上的引用计数窗口——测试里用指针相等直接验了这一点。
- **没有 budget，也没有地方放 budget。** `$1073741824` 与「1 GiB 还没收完」在 `redis-rs` 看来是同一件事，唯一可做的就是继续读。§24.4 区分「协议错误（断连）」与「预算超限（干净拒绝并告诉用户）」，这里两者不可分。
- 嵌套深度是 `MAX_RECURSE_DEPTH = 100`，硬编码、不可配置。不是缺陷，但 §24.3 要求这个预算归我们。

### 4. push channel 预算 —— **否**

push 就是 `parse_redis_value` 返回的那个值。两个回复之间能来多少个 push 没有上限，也没有公开 API 去加一个：能让调用方驱动解码并计数的 `ValueCodec` 位于私有模块（`redis-1.7.0/src/parser.rs` 的 `mod aio_support`），而 `lib.rs` 只 re-export 了 `{Parser, parse_redis_value}`。§24.3 的 push 预算无处安放。

（RESP2 的 pub/sub 是普通 `*3` 数组，两边在帧层都无法分流——这一条是平的，测试里也记了。）

### 5. 取消后的读取行为与连接状态 —— **否**

公开的低层入口只有两个，都不行：

- **`parse_redis_value(&[u8]) -> RedisResult<Value>`** 不报告消耗了多少字节。缓冲里有两帧时它返回第一帧，第二帧从哪开始无从得知——除非把它刚做过的分帧再做一遍。pipelining、`--pipe`、MULTI/EXEC、push 与回复同时到达，全都是「缓冲里不止一帧」，这是常态不是特例。
- **`Parser::parse_value<T: Read>`** 收的是**阻塞** `std::io::Read`。喂一个「还没准备好」的 reader（返回 `WouldBlock`，也就是非阻塞 socket 的常态）它不会说「未完成」，它返回错误，而已经吃掉的字节留在 parser 的私有 partial state 里。**无法得知从 socket 上取走了几个字节，因此连接既不能续读也不能安全复用**——下一次读会从半个帧中间开始。这恰好就是 ADR-007 所说的 `UnknownAfterSend`，而在这里它会由一次普通的慢网络产生。

半个好消息，如实记下：`redis-rs` **确实**区分了截断（IO 类错误）与畸形（parse 类错误），所以「继续读」和「断连」是可分的。这一条原本预期它做不到，它做到了。缺的是字节计数。

### 6. 重定向与内部重试 —— **不归我们控制**

`err_parser` 在帧层就把 `-MOVED` / `-ASK` / `-TRYAGAIN` / `-CLUSTERDOWN` / `-READONLY` 等 14 个前缀识别成 `ServerErrorKind`，并附带 `retry_method()`：`MovedRedirect`、`AskRedirect`、`WaitAndRetry`、`RefreshSlotsAndRetry`。

`retry_method` 是 `pub(crate)`，`ServerErrorKind` 是 `#[non_exhaustive]`——**这套重试策略既不可观测也不可覆盖**。ADR-007 要求写命令不盲目重试、不确定即 `UnknownAfterSend`；把重试决定交给一个我们看不见也改不动的表，与之直接冲突。

另外错误码本身被规范化了：`ServerErrorKind::code()` 会重新生成标准拼写，原始大小写不保留（`-moved 1 x` 落到 `Repr::Extension`），错误文本是 `ArcStr`（UTF-8），`!` blob error 还要过一遍 lossy 转换。

## 过度接受的 5 个（安全相关）

| fixture | 线上字节 | `redis-rs` | 为什么要紧 |
|---|---|---|---|
| `malformed-length/t24-1-0` | `$-5\r\n` | `Nil` | RESP 只定义了 `-1` 一个负长度。`if size < 0 { Nil }` 让任意负数变成一个正常的 null |
| `malformed-length/t2a-1-1` | `*-5\r\n` | `Nil` | 同上 |
| `malformed-length/t7e-1-3` | `~-5\r\n` | `Nil` | `~-1` 本身在 RESP3 里就不存在，`~-5` 更不存在 |
| `overlong-number/int-bad-6` | `:+1\r\n` | `Int(1)` | RESP 整数没有符号前缀；`line.trim().parse()` 照收 |
| `overlong-number/bignum-bad` | `(12a\r\n` | `BigNumber(b"12a")` | 不带 `num-bigint` 时完全不校验，畸形 big number 由第一个试图使用它的调用方在离线上任意远的地方发现 |

**一个看起来像值的协议违规，比一个看起来像错误的协议违规更糟**，因为上层永远不会知道。

## 决定

§31.2 六条门槛里通过 0 条完整、1 条部分（截断/畸形可分）。触发 §31.2 写明的分支：

> 如库不能满足增量解析和状态控制，选择受测扩展/补丁或自建最小协议层，仍通过同一 kernel；`redis-rs` 可作为互操作测试对象，不是永远保留两套难以对齐的产品路径。

自建（`pr-protocol`）已经存在且通过全部 316 个 fixture。`redis-rs` 转为**互操作测试对象**：本文件的 19 个测试常驻 CI，`ci/check-redis-rs-is-dev-only.sh` 保证它停留在 `[dev-dependencies]`（该脚本已用一次真实违规验证过会红）。

见 [ADR-031](../adr/ADR-031.md)。

## 这个结论什么时候需要重看

`crates/pr-protocol/tests/spike_002_redis_rs.rs` 里每一条断言都写成「若此处变了，ADR-031 需要重读」。具体触发点：

- `redis-rs` 实现 RESP3 streamed 类型（`not_implemented` 从 28 掉下来）
- `ValueCodec` 被公开导出（增量解析与 push 预算就有地方放了）
- 出现可配置的 decode budget
- `retry_method` 变成可覆盖的
