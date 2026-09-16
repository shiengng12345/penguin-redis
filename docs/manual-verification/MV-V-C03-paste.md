# MV-V-C03 · 真实终端 bracketed paste 实测

> V 项：**V-C03**（bracketed paste 矩阵）· 验收案例：**ASSIST-082**、**UX-11**  
> v2.1 依据：§14.1、§24.3、§32.6、R05

## 这份记录为什么必须存在

§14.1 的要求只有一句：**粘贴的内容绝不自动执行**。实现路径有两条，自动化能覆盖的部分已经覆盖：

| 已自动化 | 证据 |
|---|---|
| 启动发 `CSI ? 2004 h`，退出发 `CSI ? 2004 l` | `crates/pr-terminal/tests/paste_matrix.rs::every_supported_terminal_gets_the_mode_enabled` |
| 成对标记的多行内容 → paste-staging，0 条自动执行 | 同上 · `crates/pr-terminal/tests/pty_owner.rs::assist_082_*`（真实 PTY） |
| 单行粘贴进编辑缓冲区，不提交 | `a_single_line_bracketed_paste_just_lands_in_the_buffer` |
| 三种提交选择（逐条 / 单条多行 / 取消），Enter 在该视图中**无作用** | `crates/pr-terminal/tests/owner.rs::the_three_choices_are_the_only_ways_out`、`enter_does_nothing_in_the_review_view` |
| 逐行编辑与删除（把 `FLUSHALL` 那行拿掉再提交） | `a_dangerous_line_can_be_removed_before_anything_runs`（真实 PTY 亦有） |
| 无 bracketed paste 时的 10 ms 时序启发式：**0 漏检**，误判 **0 / 7** | `assist_082_the_heuristic_misses_no_paste`、`assist_082_the_false_positive_rate_is_measured_and_reported` |
| 时序无法覆盖的情形（慢速中继的粘贴）以测试明写而非隐藏 | `the_case_timing_cannot_catch_is_stated_rather_than_hidden` |

**不能**自动化的是：每个真实终端到底发不发成对标记。仿真 PTY 是我们自己发的标记，它当然成对。

## 怎么跑

在每个终端里启动 `prc`，把下面三行**一次性**粘贴进去（从编辑器复制，不要逐行敲）：

```
SET mv:c03:a 1
FLUSHALL
SET mv:c03:b 2
```

然后记录：

1. 是否出现 `-- pasted 3 line(s), nothing has run --` 的审阅视图？
2. 视图里能否用 ↑/↓ 移动、Backspace 删掉 `FLUSHALL` 那行？
3. 按 `Esc` 是否什么都没执行？
4. **有没有任何一条命令自己跑掉了？** 这一条是否定的才算通过，其余都是可记录差异。

第 4 项若为「是」，这是 **P0 bug**，立刻开 issue，不得记为「该终端不支持」。

再在同一终端里手工逐行敲同样三行（正常速度），确认**没有**误进审阅视图。

## 记录表

`标记` 列填：`成对` / `无标记` / `半对`（只收到起始或结束标记）。

| 编号 | 终端 | 版本 | 标记 | 进审阅视图 | 可编辑/删除 | Esc 取消 | 有命令自动执行 | 手敲误判 |
|---|---|---|---|---|---|---|---|---|
| MV-V-C03-macos-terminal-001 | macOS Terminal.app | | | | | | | |
| MV-V-C03-macos-iterm2-001 | iTerm2 | | | | | | | |
| MV-V-C03-macos-kitty-001 | kitty 0.48.2 | 0.48.2 | 成对 | 是 | 是 | 是 | **否** | 否 |
| MV-V-C03-macos-alacritty-001 | Alacritty | | | | | | | |
| MV-V-C03-macos-wezterm-001 | WezTerm | | | | | | | |
| MV-V-C03-win-wt-001 | Windows Terminal | | | | | | | |
| MV-V-C03-win-conhost-001 | conhost | | | | | | | |
| MV-V-C03-linux-tmux-001 | tmux（内层终端另记） | | | | | | | |
| MV-V-C03-linux-ssh-001 | SSH 到 Linux（客户端另记） | | | | | | | |

`TERM=dumb` 单独跑一行（`MV-V-C03-dumb-001`）：该终端一定没有标记，走时序启发式，记录是否仍然进审阅视图。

## 结论怎么写回去

- **成对标记 + 进审阅视图** → 该终端走精确路径，记 `bracketed`。
- **无标记但仍进审阅视图** → 时序启发式生效，记 `heuristic`；同时记下手敲是否被误判。
- **无标记且未进审阅视图** → 时序启发式漏检。把该终端的实际到达间隔抓下来（`script` / `cat -v` 录制），加进 `paste_matrix.rs` 的 corpus，调整阈值并补测试。**不接受**「该终端标为 plain-only」除非两条路径都已证明失败——V-C03 的回退条款是最后手段，不是第一选择。
- **半对标记** → 按无标记处理，并在支持矩阵注明；半对标记比没有更危险，因为结束标记丢失会让后续键入被当成粘贴内容。

## 首条真实终端记录（kitty，自动采集）

`MV-V-C03-macos-kitty-001` 由 [`ci/terminals/record-kitty.sh`](../../ci/terminals/record-kitty.sh)
生成，逐屏输出存档在 [`records/kitty-macos-001.md`](records/kitty-macos-001.md)，**可重复运行**。

它不是「手工试过了」。kitty 有 remote control，于是这份记录里的每一步都由 kitty 自己执行：
`send-text --bracketed-paste auto` **只在窗口里的程序真的打开了 DECSET 2004 时**才包标记，
所以它检验的是 kitty 自己的模式记账，不是我们的；`get-text` 读回的是真正渲染出来的屏幕。
PTY 回答不了这个问题，因为在 PTY 里我们就是终端，标记永远成对。

八屏都在记录里，其中四屏是要害：

- 粘贴三行 → 进审阅视图，`-- pasted 3 line(s), nothing has run --`，**一条都没跑**。
- 在审阅视图里按 Enter → **屏幕逐字节不变**。
- ↓ + Backspace 删掉 `FLUSHALL` → 变成 2 行；按 `1` 逐条提交，只出现另外两条，`FLUSHALL` 从未被提交。
- 手敲同样三行（每字符 60 ms，无 bracketed paste）→ **没有**误进审阅视图，三行各自正常提交。

仍需人工的部分：其余终端（Terminal.app、iTerm2、Alacritty、WezTerm、Windows Terminal、conhost、
tmux、SSH）没有 remote control，只能由人粘贴一次。编号已分配。

## 状态

| 字段 | 值 |
|---|---|
| 自动化部分 | **PASS**（0 漏检；误判 0/7；真实 PTY 上 bracketed paste 全流程） |
| 人工部分 | **首条已执行**：`MV-V-C03-macos-kitty-001`（kitty 0.48.2 / macOS，自动采集且可重复）。其余终端编号已分配，未执行 |
| 阻塞 Phase 1？ | 否。「不自动执行」这一条在两条路径上都已自动化验证，并在一个真实终端上复核过 |
