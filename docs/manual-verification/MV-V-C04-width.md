# MV-V-C04 · 真实终端显示宽度实测

> V 项：**V-C04**（Unicode 宽度矩阵）· 验收案例：**UX-08**、**ASSIST-018**、**ASSIST-067**  
> v2.1 依据：§14.6、§32.6、R33

## 这份记录为什么必须存在

§14.6 已经说明「显示宽度正确」**不可能**对所有终端成立：终端与字体对 East Asian Ambiguous、ZWJ 序列、变体选择符的处理各不相同。因此自动化测试能证明的只有两件事，而且已经证明了：

| 已自动化 | 证据 |
|---|---|
| 固定策略 + 固定仿真终端下，UX-08 的表格框线对齐 | `crates/pr-terminal/tests/width_matrix.rs` · 12 tests |
| 策略与终端**不一致**时，`DrawMode::CursorReset` 仍保证框线对齐 | 同上，`cursor_reset_keeps_the_frame_aligned_when_the_width_judgement_is_wrong` |
| `CSI 6 n` 探测在非 TTY 上拒绝执行、在无应答时放弃而不是挂起 | `crates/pr-terminal/src/probe.rs` · 12 tests |
| 探测结论能驱动绘制模式切换 | `the_probe_selects_the_fallback_for_a_terminal_that_disagrees` |

**不能**自动化的是：每个真实终端 + 字体组合实际画出来是几列。仿真终端是我们写的，它只会同意我们自己的模型。所以下面这张表要人去跑。

## 怎么跑

在每个终端里执行：

```
prc --probe-width
```

它会逐个打印代表字符、用 `CSI 6 n` 问光标位置、把「策略认为几列」和「终端实际几列」并排列出，最后给出结论（`Padded` 或 `CursorReset`）。若终端不应答，命令会在 500 ms 后放弃并说明，不会挂住。

再执行一次目视检查（UX-08）：

```
prc @dev --output pretty HGETALL <一个含中文/emoji/组合字符的 hash>
```

看框线是否对齐。若不齐，记录下来，并确认 `--probe-width` 是否已经把该终端判为 `CursorReset`——**探测判对但绘制仍不齐**是一个真 bug，必须开 issue，不能记成「已知差异」。

## 代表字符

| 字符 | 码位 | 类别 | narrow 策略 | wide 策略 |
|---|---|---|---|---|
| 中 | U+4E2D | East Asian Wide | 2 | 2 |
| → | U+2192 | Ambiguous | 1 | 2 |
| ± | U+00B1 | Ambiguous | 1 | 2 |
| § | U+00A7 | Ambiguous | 1 | 2 |
| ① | U+2460 | Ambiguous | 1 | 2 |
| 🐧 | U+1F427 | Emoji Presentation | 2 | 2 |
| ◌́ | U+0301 | 组合字符 | 0 | 0 |

## 记录表

每格填终端实测列数；`—` 表示终端不应答 `CSI 6 n`。最后一列填目视框线是否对齐。

| 编号 | 终端 | 版本 | 字体 | LANG | 中 | → | ± | § | ① | 🐧 | ◌́ | 探测结论 | 框线对齐 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| MV-V-C04-macos-terminal-001 | macOS Terminal.app | | | | | | | | | | | | |
| MV-V-C04-macos-iterm2-001 | iTerm2 | | | | | | | | | | | | |
| MV-V-C04-macos-kitty-001 | kitty | | | | | | | | | | | | |
| MV-V-C04-macos-alacritty-001 | Alacritty | | | | | | | | | | | | |
| MV-V-C04-macos-wezterm-001 | WezTerm | | | | | | | | | | | | |
| MV-V-C04-win-wt-001 | Windows Terminal | | | | | | | | | | | | |
| MV-V-C04-win-conhost-001 | conhost | | | | | | | | | | | | |
| MV-V-C04-linux-tmux-001 | tmux（内层终端另记） | | | | | | | | | | | | |
| MV-V-C04-linux-ssh-001 | SSH 到 Linux（客户端另记） | | | | | | | | | | | | |

CJK locale 要单独跑一遍：同一终端设 `LANG=zh_CN.UTF-8` 与 `LANG=en_US.UTF-8` 各记一行，序号递增（`-002`）。默认策略由 locale 推导（§14.6），所以两者的「探测结论」本来就可能不同，这不是 bug。

## 结论怎么写回去

- 某终端**不应答 `CSI 6 n`**：策略保持不变，这是已定义行为。在支持矩阵里标注该终端无法探测。
- 某终端**探测出分歧**：默认切 `CursorReset`，框线应当仍对齐。若不对齐 → 真 bug。
- 某终端**探测一致但目视不齐**：说明代表字符集没覆盖到该终端实际用到的字符类别，扩充 `representative_chars()` 并补自动化测试。

## 状态

| 字段 | 值 |
|---|---|
| 自动化部分 | **PASS**（见上表证据） |
| 人工部分 | 待填 —— 记录编号已分配，未执行 |
| 阻塞 Phase 1？ | 否。§14.6 的契约是「在声明的策略下一致 + 有降级路径」，两者均已自动化验证 |
