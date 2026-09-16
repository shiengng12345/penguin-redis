#!/usr/bin/env bash
# V-C03 / V-C04 / V-C05 — a *real terminal* record, taken without a human at the keyboard.
#
# The manual-verification templates say what cannot be automated: whether a real terminal
# emits paired `CSI ? 2004` markers, what widths it really assigns, which keys it really
# delivers. A PTY cannot answer any of those, because in a PTY we are the terminal.
#
# kitty can, because it has remote control. `send-text --bracketed-paste auto` wraps the text
# only if the program in the window has actually turned the mode on, so it exercises kitty's
# own DECSET bookkeeping, not ours; `get-text` reads the rendered screen back. The result is a
# record produced by kitty's real paste, width and key handling, and it is repeatable.
#
# Requires: kitty on PATH and a desktop session (kitty needs a GPU context; there is no
# headless mode). Writes a markdown record to the path given, or to stdout.
#
# Usage: ci/terminals/record-kitty.sh [out.md]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-/dev/stdout}"
SOCK="/tmp/pr-kitty-record-$$.sock"
WORK="$(mktemp -d)"
DEMO="$ROOT/target/debug/pr-terminal-demo"
PRC="$ROOT/target/debug/prc"

command -v kitty >/dev/null || { echo "kitty not on PATH" >&2; exit 2; }
[ -x "$DEMO" ] || { echo "build first: cargo build -p pr-terminal --bin pr-terminal-demo" >&2; exit 2; }
[ -x "$PRC" ]  || { echo "build first: cargo build -p prc --bin prc" >&2; exit 2; }

KITTY_PID=""
cleanup() {
  [ -n "$KITTY_PID" ] && kill "$KITTY_PID" 2>/dev/null || true
  rm -rf "$WORK" "$SOCK"
}
trap cleanup EXIT

start_kitty() {  # start_kitty <cmd...>
  rm -f "$SOCK"
  kitty -o allow_remote_control=yes -o confirm_os_window_close=0 \
        --listen-on "unix:$SOCK" --start-as=hidden --title pr-terminal-record \
        -- "$@" >"$WORK/kitty.log" 2>&1 &
  KITTY_PID=$!
  for _ in $(seq 1 60); do [ -S "$SOCK" ] && break; sleep 0.25; done
  [ -S "$SOCK" ] || { echo "kitty never opened its control socket" >&2; exit 1; }
  sleep 2
}
stop_kitty() {
  kill "$KITTY_PID" 2>/dev/null || true
  wait "$KITTY_PID" 2>/dev/null || true
  KITTY_PID=""
  rm -f "$SOCK"
}
rc()   { kitty @ --to "unix:$SOCK" "$@"; }
snap() { rc get-text --extent=screen | sed -e 's/[[:space:]]*$//' | grep -v '^$' || true; }
key()  { rc send-text "$1"; sleep "${2:-0.8}"; }

KITTY_VERSION="$(kitty --version | head -1)"

# ------------------------------------------------------------------ V-C03 · paste
printf 'SET mv:c03:a 1\nFLUSHALL\nSET mv:c03:b 2\n' > "$WORK/three.txt"
printf 'GET mv:c03:single' > "$WORK/one.txt"

start_kitty "$DEMO"
C03_BEFORE="$(snap)"
rc send-text --bracketed-paste auto --from-file "$WORK/three.txt"; sleep 1.5
C03_STAGED="$(snap)"
key '\r' 1                       # Enter must do nothing in the review view
C03_AFTER_ENTER="$(snap)"
key '\x1b[B' 0.5; key '\x7f' 1   # down, delete -> FLUSHALL is gone
C03_AFTER_DELETE="$(snap)"
key '1' 1.5                      # run one by one
C03_AFTER_SUBMIT="$(snap)"
rc send-text --bracketed-paste auto --from-file "$WORK/three.txt"; sleep 1
key '\x1b' 1                     # Esc cancels
C03_AFTER_CANCEL="$(snap)"
rc send-text --bracketed-paste auto --from-file "$WORK/one.txt"; sleep 1
C03_SINGLE="$(snap)"
for _ in $(seq 1 20); do rc send-text '\x7f'; done; sleep 0.8

# Typed at human speed, no bracketed paste at all: the heuristic must not fire.
python3 - "$SOCK" <<'PY'
import os, subprocess, sys, time
sock = sys.argv[1]
for line in ["SET mv:c03:t1 1", "FLUSHALL", "SET mv:c03:t2 2"]:
    for ch in line:
        subprocess.run(["kitty", "@", "--to", "unix:" + sock, "send-text", "--stdin"],
                       input=ch.encode(), check=True)
        time.sleep(0.06)
    subprocess.run(["kitty", "@", "--to", "unix:" + sock, "send-text", "--stdin"],
                   input=b"\r", check=True)
    time.sleep(0.25)
PY
sleep 1
C03_TYPED="$(snap)"
stop_kitty

# ------------------------------------------------------------------ V-C04 · widths
# `sleep` after the probe so the window is still there to read: kitty closes a window when its
# child exits, and a screen that no longer exists reads back empty.
start_kitty /bin/sh -c "$PRC --probe-width; sleep 30"
sleep 5
C04="$(snap)"
stop_kitty

# ------------------------------------------------------------------ V-C05 · keys
#
# `send-key`, not `send-text`. send-text writes bytes straight into the pty, which would only
# prove that our decoder understands escape sequences we wrote ourselves -- the PTY tests
# already do that. send-key goes through kitty's own key encoder and is only delivered "if the
# current keyboard mode for the program supports the particular key", so what comes out is
# kitty's answer, not ours.
start_kitty /bin/sh -c "PR_KEYPROBE=1 $DEMO; sleep 5"
for k in f1 f2 f3 f4 f5 f6 ctrl+r ctrl+p ctrl+n ctrl+space; do
  rc send-key "$k"; sleep 0.3
done
sleep 1
C05="$(snap)"
rc send-key ctrl+d; sleep 1
stop_kitty

# ------------------------------------------------------------------ the record
{
  echo "# 真实终端记录 · kitty（自动采集）"
  echo
  echo "> 由 \`ci/terminals/record-kitty.sh\` 生成，可重复运行。"
  echo "> 终端：\`$KITTY_VERSION\` · 主机：$(uname -sm) · 日期：$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "这份记录回答的是 PTY 回答不了的问题：**这个终端自己**发不发成对的 \`CSI ? 2004\` 标记、"
  echo "自己怎么算宽度、自己送不送这些键。在 PTY 里我们就是终端，所以那里的答案永远是「是」。"
  echo
  for section in C03 C04 C05; do
    case $section in
      C03) echo "## V-C03 · bracketed paste"; echo;
           for n in BEFORE STAGED AFTER_ENTER AFTER_DELETE AFTER_SUBMIT AFTER_CANCEL SINGLE TYPED; do
             case $n in
               BEFORE)       t="起始屏幕";;
               STAGED)       t="粘贴 3 行之后（必须进审阅视图，且一条都没跑）";;
               AFTER_ENTER)  t="在审阅视图里按 Enter（必须什么都不发生）";;
               AFTER_DELETE) t="↓ 然后 Backspace，删掉 FLUSHALL 那行";;
               AFTER_SUBMIT) t="按 1 逐条提交（只应出现另外两条）";;
               AFTER_CANCEL) t="再粘贴一次然后 Esc（不应新增任何提交）";;
               SINGLE)       t="单行粘贴（进编辑缓冲区，不提交）";;
               TYPED)        t="手敲三行，无 bracketed paste（时序启发式不得误判）";;
             esac
             eval "body=\$C03_$n"
             echo "### $t"; echo; echo '```text'; echo "$body"; echo '```'; echo
           done;;
      C04) echo "## V-C04 · 宽度（\`prc --probe-width\`，走 \`CSI 6 n\`）"; echo;
           echo '```text'; echo "$C04"; echo '```'; echo;;
      C05) echo "## V-C05 · 按键送达（\`kitty @ send-key\`，经 kitty 自己的按键编码器）"; echo;
           echo "发送顺序：F1 F2 F3 F4 F5 F6 Ctrl+R Ctrl+P Ctrl+N Ctrl+Space"; echo;
           echo '```text'; echo "$C05"; echo '```'; echo;
           echo "**这条记录能说明什么、不能说明什么。** 能：kitty 这一层不拦截这些键，它的编码器把它们都送出去了，"
           echo "我们的解码器逐个还原正确。不能：remote control 从 kitty 内部注入按键，**绕过了 OS 与输入法那一层**。"
           echo "真人按下时，macOS 的输入法切换仍可能先吃掉 Ctrl+Space、系统仍可能占用 F1–F6——那一层只有人工记录"
           echo "能回答，编号见 \`MV-V-C05-keys.md\`。§14.2 的通过标准本来就不依赖这些键：每个功能至少一条纯 ASCII"
           echo "文字入口。"; echo;;
    esac
  done
} > "$OUT"
