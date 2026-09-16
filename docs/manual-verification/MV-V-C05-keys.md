# MV-V-C05 · 终端 × 输入法 按键可达性实测

> V 项：**V-C05**（按键可达性）· 验收案例：**ASSIST-068**  
> v2.1 依据：§14.2、§32.6

## 通过标准是什么，不是什么

V-C05 的验收标准是 **「每个功能至少一条文字入口在所有组合可用」**，不是「每个快捷键在所有终端都能用」。后者不可能成立，§14.2 也没有这样要求：

> 这些是设计默认值，可重映射。`Ctrl+Space` 可能与输入法/终端快捷键冲突，不作为唯一必需入口。Function keys 被系统拦截时仍可用文字入口；**不能要求用户先修改系统快捷键才能找帮助**。

所以下面这张表记录的是**差异**，不是通过条件。判为不通过的情形只有一种：某个功能在某个组合下既按不到键、又没有文字入口。

## 已自动化的部分

| 已自动化 | 证据 |
|---|---|
| §14.2 八行全部在代码里，F5 不被擅自赋予绑定 | `crates/pr-repl/src/entries.rs::the_table_covers_every_row_of_section_14_2` |
| 除「取消输入」外每个功能都有文字入口，且都能 parse | `every_function_except_cancelling_is_reachable_by_typing`、`every_text_entry_parses` |
| 文字入口全是可直接键入的 ASCII，不含控制字符/转义序列 | `no_text_entry_needs_a_key_a_terminal_might_eat`、`the_text_entries_are_typeable_under_any_input_method` |
| `Ctrl+Space` 不作为任何功能的默认键 | `ctrl_space_is_never_a_default` |
| 重映射按键**不能**连带移除文字入口 | `remapping_a_key_cannot_remove_the_text_entry` |
| 裸 PTY 上各键实际送达情况 | `crates/pr-terminal/tests/key_matrix.rs::what_this_terminal_delivers_is_recorded` |

裸 PTY（macOS，2026-09-16）实测：`F1(SS3)` `F1(CSI)` `F2(SS3)` `F2(CSI)` `F3(SS3)` `F4(SS3)` `F5` `F6` `Ctrl+R` `Ctrl+P` `Ctrl+N` **全部送达**；`Ctrl+Space`（NUL）**未送达**——与 §14.2 的预判一致，也正是它不能作为唯一入口的原因。

## 怎么跑

在每个「终端 × 输入法」组合里：

```
PR_KEYPROBE=1 <prc 所在目录>/pr-terminal-demo
```

逐个按 F1、F2、F3、F4、F5、F6、Ctrl+R、Ctrl+P、Ctrl+N、Ctrl+Space。每收到一个键会打印一行 `key=...`；**没打印就是被拦截了**。按 Ctrl+D 退出。

输入法要分两种状态各记一次：**英文/直接输入**态、**中文/组合**态。组合态下按 Ctrl+Space 通常就是切换输入法本身，这正是要记录的。

然后在 `prc` 里逐条验证文字入口仍然可用：`:help`、`:find`、`:guide`、`:complete`、`:history`、`exit`。

## 记录表

每格填 `✓`（送达）/ `✗`（被拦截）/ `→X`（被拦截且系统做了别的事，注明）。

| 编号 | 终端 | 输入法 | 状态 | F1 | F2 | F3 | F4 | F5 | F6 | ^R | ^P | ^N | ^Space | 文字入口全可用 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| MV-V-C05-macos-terminal-none-001 | Terminal.app | 无 | — | | | | | | | | | | | |
| MV-V-C05-macos-terminal-pinyin-001 | Terminal.app | 拼音 | 英文 | | | | | | | | | | | |
| MV-V-C05-macos-terminal-pinyin-002 | Terminal.app | 拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-macos-iterm2-pinyin-001 | iTerm2 | 拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-macos-kitty-encoder-001 | kitty 0.48.2 | 不经过（见下） | — | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| MV-V-C05-macos-kitty-pinyin-001 | kitty | 拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-macos-wezterm-pinyin-001 | WezTerm | 拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-win-wt-ime-001 | Windows Terminal | 微软拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-win-conhost-ime-001 | conhost | 微软拼音 | 中文 | | | | | | | | | | | |
| MV-V-C05-linux-fcitx-001 | 常用终端 | fcitx5 | 中文 | | | | | | | | | | | |
| MV-V-C05-linux-tmux-fcitx-001 | tmux + fcitx5 | fcitx5 | 中文 | | | | | | | | | | | |

macOS 的 F1–F6 默认是系统亮度/调度中心，除非勾选「将 F1、F2 等键用作标准功能键」。**两种设置都要记**：默认设置下 `✗` 是预期结果，不是 bug——它恰恰是文字入口存在的理由。

## 结论怎么写回去

- **键被拦截 + 文字入口可用** → 记录差异，在文档的平台章节写明该平台的默认重映射建议。不影响 V-C05 通过。
- **键被拦截 + 文字入口不可用** → **不通过**。按 V-C05 的回退条款，该键在该平台默认重映射并写入文档；同时补 `entries.rs` 的测试。
- **某功能既无键也无文字入口** → 这是 §14.2 表格与实现不一致，`entries.rs` 的测试本应拦住，先补测试再补实现。

## 首条真实终端记录（kitty 的按键编码器，自动采集）

`MV-V-C05-macos-kitty-encoder-001` 由 [`ci/terminals/record-kitty.sh`](../../ci/terminals/record-kitty.sh)
生成，输出存档在 [`records/kitty-macos-001.md`](records/kitty-macos-001.md)。用的是 `kitty @ send-key` 而**不是** `send-text`：
后者把字节直接写进 pty，只能证明我们的解码器认得我们自己写的转义序列（PTY 测试早就证明了）；
前者走 kitty 自己的按键编码器，且只在「当前键盘模式支持该键」时才投递，所以答案是 kitty 的。

结果：F1–F6、Ctrl+R/P/N、Ctrl+Space **十个全部送达**，解码逐个正确。

**这条记录不能说明什么，必须写清楚。** remote control 从 kitty 内部注入按键，**绕过了 OS 与输入法
那一层**。真人按下时，macOS 的输入法切换仍会先吃掉 Ctrl+Space，F1–F6 在未勾选「用作标准功能键」时
仍归系统。所以 `MV-V-C05-macos-kitty-pinyin-001` 这类带输入法的行**依然需要人**，编号保留。
「文字入口全可用」这一列对本行填 `—`：本次采集跑的是 keyprobe，不是 `prc` REPL，没有验证它，
不能借着同一行的其他 ✓ 顺手声称。

这正好把三层分开了：**编码器层**（kitty，已测，全过）、**OS / 输入法层**（只有人能测）、
**通过标准层**（§14.2 的文字入口，与前两层都无关，已由测试保证）。

## 状态

| 字段 | 值 |
|---|---|
| 自动化部分 | **PASS**（文字入口完备性 + 裸 PTY 送达矩阵） |
| 人工部分 | **首条已执行**：`MV-V-C05-macos-kitty-encoder-001`（终端编码器层，自动采集且可重复）。输入法层编号已分配，未执行 |
| 阻塞 Phase 1？ | 否。验收标准（文字入口在所有组合可用）已由类型与测试保证，与终端拦截与否无关 |
