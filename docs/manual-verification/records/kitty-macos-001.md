# 真实终端记录 · kitty（自动采集）

> 由 `ci/terminals/record-kitty.sh` 生成，可重复运行。
> 终端：`kitty 0.48.2 created by Kovid Goyal` · 主机：Darwin arm64 · 日期：2026-09-16T12:56:33Z

这份记录回答的是 PTY 回答不了的问题：**这个终端自己**发不发成对的 `CSI ? 2004` 标记、
自己怎么算宽度、自己送不送这些键。在 PTY 里我们就是终端，所以那里的答案永远是「是」。

## V-C03 · bracketed paste

### 起始屏幕

```text
demo starting
penguin@r2/0>
```

### 粘贴 3 行之后（必须进审阅视图，且一条都没跑）

```text
demo starting
penguin@r2/0>
-- pasted 3 line(s), nothing has run --
> SET mv:c03:a 1
  FLUSHALL
  SET mv:c03:b 2
[1] run one by one  [2] run as one command  [Esc] cancel
```

### 在审阅视图里按 Enter（必须什么都不发生）

```text
demo starting
penguin@r2/0>
-- pasted 3 line(s), nothing has run --
> SET mv:c03:a 1
  FLUSHALL
  SET mv:c03:b 2
[1] run one by one  [2] run as one command  [Esc] cancel
```

### ↓ 然后 Backspace，删掉 FLUSHALL 那行

```text
demo starting
penguin@r2/0>
-- pasted 2 line(s), nothing has run --
  SET mv:c03:a 1
> SET mv:c03:b 2
[1] run one by one  [2] run as one command  [Esc] cancel
```

### 按 1 逐条提交（只应出现另外两条）

```text
demo starting
[info] submitted: SET mv:c03:a 1
[info] submitted: SET mv:c03:b 2
penguin@r2/0>
```

### 再粘贴一次然后 Esc（不应新增任何提交）

```text
demo starting
[info] submitted: SET mv:c03:a 1
[info] submitted: SET mv:c03:b 2
penguin@r2/0>
```

### 单行粘贴（进编辑缓冲区，不提交）

```text
demo starting
[info] submitted: SET mv:c03:a 1
[info] submitted: SET mv:c03:b 2
penguin@r2/0> GET mv:c03:single
```

### 手敲三行，无 bracketed paste（时序启发式不得误判）

```text
demo starting
[info] submitted: SET mv:c03:a 1
[info] submitted: SET mv:c03:b 2
penguin@r2/0> SET mv:c03:t1 1
[info] submitted: SET mv:c03:t1 1
penguin@r2/0> FLUSHALL
[info] submitted: FLUSHALL
penguin@r2/0> SET mv:c03:t2 2
[info] submitted: SET mv:c03:t2 2
penguin@r2/0>
```

## V-C04 · 宽度（`prc --probe-width`，走 `CSI 6 n`）

```text
char      assumed  measured
  U+4E2D        2         2
  U+2192        1         1
  U+00B1        1         1
  U+00A7        1         1
  U+2460        1         1
  U+1F427        2         2
  U+0301        0         0
0 disagreement(s); tables will draw Padded
```

## V-C05 · 按键送达（`kitty @ send-key`，经 kitty 自己的按键编码器）

发送顺序：F1 F2 F3 F4 F5 F6 Ctrl+R Ctrl+P Ctrl+N Ctrl+Space

```text
keyprobe ready
key=Function(1)
key=Function(2)
key=Function(3)
key=Function(4)
key=Function(5)
key=Function(6)
key=Ctrl('r')
key=Ctrl('p')
key=Ctrl('n')
key=Ctrl(' ')
```

**这条记录能说明什么、不能说明什么。** 能：kitty 这一层不拦截这些键，它的编码器把它们都送出去了，
我们的解码器逐个还原正确。不能：remote control 从 kitty 内部注入按键，**绕过了 OS 与输入法那一层**。
真人按下时，macOS 的输入法切换仍可能先吃掉 Ctrl+Space、系统仍可能占用 F1–F6——那一层只有人工记录
能回答，编号见 `MV-V-C05-keys.md`。§14.2 的通过标准本来就不依赖这些键：每个功能至少一条纯 ASCII
文字入口。

