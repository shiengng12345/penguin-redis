# SPIKE-003 · Windows ConPTY 专项

- **V 项**：V-C07
- **验收案例**：WIN-01、WIN-02
- **v2.1 依据**：§35.1、§32.1、§22.3、R20
- **日期**：2026-09-16
- **结论**：**采用**。Windows Terminal / ConPTY 作为一级目标可行，但代价是三个必须先解决的真实缺陷——全部由 Windows CI 抓到，全部已修并有测试守住。

## 摘要

这个 spike 的价值不在于「跑通了」，而在于它证明了一件更重要的事：**V-A01 的 Windows 路径此前从未真正执行过**。pty-harness 自己的测试在 Windows 上全部 `#[cfg_attr(windows, ignore)]`，注释写着「V-C07 covers Windows」。V-C02 的 PTY 测试是第一次真跑 ConPTY，立刻连续暴露三个缺陷，其中一个是 P0。

如果按原计划把 V-C07 留到最后，这三个问题会在 Phase 2 的终端工作里一起爆出来，而那时终端代码已经写了几千行。

## 发现 1 · ConPTY 启动时阻塞等待光标位置报告（P0，harness）

**现象**：全部 7 个 PTY 测试在 Windows 上超时，收到的字节数是 **4**。

```
Timeout { waited: 10s, needle: "demo starting", received: 4, got: "\x1b[6n" }
```

**原因**：`ESC [ 6 n` 是 Device Status Report。ConPTY 在启动时发出它并**阻塞**，直到持有 pty master 的终端回答。我们的 harness 从不回答，于是 ConPTY 一个字节的子进程输出都不转发。

**修复**：reader 线程识别该查询并回 `ESC [ 1;1 R`。不追踪真实光标——提问的程序需要的是一个格式正确、及时的答案。查询可能跨两次 read 被切开，所以带 3 字节的 carry。回复记为 `Event::AutoReply` 而非 `Event::Input`，这样 golden 能分清哪些字节是测试发的、哪些是 harness 代终端发的。

**守护**：`a_cursor_position_query_is_answered`、`a_query_split_across_two_reads_is_still_answered`、`a_session_with_no_queries_sends_no_replies`，以及 PTY 侧的 `the_harness_answers_the_terminal_queries_the_child_makes`（Windows 上断言 `auto_replies() > 0`）。

**这个诊断是怎么来的**：`PtyError::Timeout` 原先只说「等什么超时了」。改成同时带上「实际收到了什么」之后，第一次 CI 就给出了答案。一个不说收到了什么的超时，在你无法 attach 的 runner 上近乎无用。

## 发现 2 · Windows 上每个字符被输入两次（P0，产品代码）

**现象**：DSR 修好后，`HGET player:10001 sta` 在屏幕上是 `HHGGEETT  ppllaayyeerr::1100000011  ssttaa`。

**原因**：crossterm 在 Windows 上对每次按键同时投递 `KeyEventKind::Press` **和** `KeyEventKind::Release`。忽略 `kind` 的翻译代码会把两者都当成按键。

**修复**：翻译逻辑从 demo 二进制移进 `pr-terminal::bridge` 作为产品代码，只接受 `Press` 与 `Repeat`。`Repeat` 必须接受——按住 Backspace 要持续删除。

**守护**：`bridge.rs` 8 个测试，含 `a_key_release_produces_nothing`、`typing_a_word_with_releases_interleaved_yields_the_word_once`。

## 发现 3 · ConPTY 下粘贴会自动执行命令（P0，产品代码）

**现象**：bracketed paste 的 PTY 测试在 Windows 上失败，而屏幕内容说明了一切：

```
penguin@r2/0> SET a 1      [info] submitted: SET a 1
penguin@r2/0> FLUSHALL     [info] submitted: FLUSHALL
penguin@r2/0> SET b 2      [info] submitted: SET b 2
```

**三条命令全部自动执行了，包括 `FLUSHALL`**——正是 §14.1 第一句禁止的事。

**原因**：两层。其一，crossterm 在 Windows 上**根本不投递** `Event::Paste`；ConPTY 把粘贴内容作为普通按键（含 Enter）送达，往 stdout 写 `ESC[?2004h` 不会在输入侧启用任何东西。其二——这一层更糟——§14.1 为这种终端准备的 10 ms 时序启发式，我写成了纯函数、配了 14 条 corpus、**从来没有接到实时输入路径上**。`classify()` 被测试了；coordinator 从不调用它。

**一个没有被调用的启发式，等于不存在。**

**修复**：`Coordinator::handle_at` 携带到达时间，`tick` 让事件循环声明「时间过去了」。burst 内完成的行被扣住不提交；输入安静下来后整个 burst 进审阅视图。单行快速输入退回编辑缓冲区**不提交**——极快的打字者多按一次 Enter，没有人跑到自己没读过的命令。

被扣住的那个 Enter 在判断「是否需要审阅」时不算作换行符，否则每条快速输入的命令都会弹出模态框；而保护强度不变：留在缓冲区里仍然需要一次明确的按键才会执行。

**顺带发现**：既有测试一直在以「0 ms/字符」打字——对这段代码而言那正是「粘贴」。它们之前能通过，恰恰说明它们从未走过要紧的那条路径。现在单元测试用单调时钟（50 ms/字符、200 ms/按键），PTY 测试逐字节限速。

**守护**：`paste_matrix.rs` 新增 5 个实时路径测试，含 `assist_082_a_burst_of_keystrokes_is_caught_without_bracketed_paste` 与 `ordinary_typing_still_submits_normally`（后者同样重要：兜底若吞掉真实 Enter，REPL 就不能用了，那比它要防的问题更糟）。

## 发现 4 · `tasklist` 的千位分隔符（非 P0，测量代码）

RSS 在分配 48 MiB 之后**下降**：`716800 -> 12288`。`tasklist /FO CSV` 的内存字段是 `"48,120 K"`，按逗号切会切进数字中间，把 48 MB 读成 120 KB。改为按引号字段边界切。

在此之前 `rss_bytes()` 在 Windows 上直接返回 `None`——三个受支持平台里有一个的预算根本没在检查。

## WIN-01 · PTY 场景在 ConPTY 下以同一套 fixture 运行

同一批测试文件、同一批断言，Windows runner 上与 macOS/Linux 一起跑：

| 测试文件 | 内容 |
|---|---|
| `crates/pr-terminal/tests/pty_owner.rs` | 11 tests：REPL 编辑、自绘下拉、push 涌入、alternate screen 往返、resize、bracketed paste 三例、录制回放 |
| `crates/pr-terminal/tests/key_matrix.rs` | 4 tests：F1–F6 / Ctrl+R/P/N / Ctrl+Space 实际送达情况 |
| `crates/pr-terminal/tests/width_matrix.rs` | 12 tests：宽度策略与降级绘制 |
| `xtask/pty-harness` | 36 tests：harness 自身，含 DSR 应答与屏幕模型 |

**一个关键前提**：断言必须针对**渲染后的屏幕**，不能针对字节流。ConPTY 转发的是它自己缓冲区的**差量**，不是子进程写出的字节——程序写 `\rpenguin> HGX`，Windows 上的字节流里这几个字母可能从不相邻，而屏幕上明明白白。`pty_harness::Screen` 因此存在：它把 VT 流应用到网格上，让测试问「屏幕上说了什么」，而这在三个平台上是同一个问题。

## WIN-02 · 二进制 stdio

`crates/prc/src/binout.rs` + `crates/prc/tests/budgets.rs::win_02_binary_output_reaches_a_pipe_byte_for_byte`。

canary 由「会被文本模式管线破坏的字节」组成，而不是一个友好的字符串——后者在一个会损坏数据的平台上照样能通过：

| 字节 | 为什么在里面 |
|---|---|
| `\r\n` | 文本模式写入会变成 `\r\r\n` |
| 裸 `\r`、裸 `\n` | 各自都是某些系统的行尾 |
| `\x1a` | Ctrl+Z，Windows 文本模式**读取**时的 EOF |
| `\0` | 任何把字节当 C 字符串的环节都会截断 |
| `\xff\xfe` | UTF-16 BOM，且是非法 UTF-8 |
| `中文` | 多字节 UTF-8 |

**Rust 的实际风险与 C 程序不同**，值得写明：Rust 的 `Stdout` 往管道/文件写字节是逐字节的，不做 CRLF 转换。Windows 的真实风险在另一边——当 stdout 是**控制台**时，Rust 走 `WriteConsoleW`（UTF-16），非法 UTF-8 无法通过，写入会失败。失败是对的，但默认的错误信息没用。`write_binary` 把它变成一句解释：二进制输出对着终端，重定向一下就好。

## Ctrl+C / Ctrl+Break

`crates/pr-core/src/signal.rs`，9 个测试。§22.3 给了 Ctrl+C 六种含义，理由是它「不能笼统当『撤销服务器操作』」——停止等待不是撤销写入。

§35.1 说 Ctrl+Break 等价于两次 Ctrl+C。实现方式是**把规则跑两遍**，而不是给结果开特例，于是两种写法不会随 §22.3 的演进而分叉；`ctrl_break_is_exactly_two_ctrl_c_and_not_a_special_case` 对全部七种活动状态逐一对照两条路径。

**尚未接线**：`SetConsoleCtrlHandler` 本身。含义层已经定义并测试，OS 投递层还没有——它需要一个封装 unsafe 的 crate（workspace 是 `unsafe_code = deny`），而选哪个 crate 是一个应当与 TLS 栈一起做的供应链决定（ADR-023）。在那之前，raw mode 下的 Ctrl+C 作为普通按键到达并已生效；**只有 Ctrl+Break 这一条路径尚未接通**。这条限制写在这里，而不是让它看起来已经完成。

## legacy conhost

按 §35.1，conhost 是 **plain 模式降级目标**，Windows Terminal 才是一级目标。CI 的 `windows-latest` runner 通过 ConPTY 运行，覆盖的是一级目标。conhost 的实测属于 `MV-V-C07-*` 人工记录，尚未执行；若不达标，按 §35.1 标 `plain-only`，Windows Terminal 必须过——现在过了。

## 结论

| 项 | 状态 |
|---|---|
| WIN-01（PTY 场景同一套 fixture 在 ConPTY 下通过） | **PASS** |
| WIN-02（二进制 stdio，无 CRLF 转换，bytes 往返一致） | **PASS** |
| Ctrl+C 语义（§22.3 六态 + 退出码区分） | **PASS** |
| Ctrl+Break ≡ 两次 Ctrl+C（含义层） | **PASS** |
| `SetConsoleCtrlHandler` OS 投递层 | **未接线**，依赖 ADR-023 的 crate 选择；限制已记录 |
| legacy conhost | 人工记录待执行；非一级目标 |
