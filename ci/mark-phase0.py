#!/usr/bin/env python3
"""Mark a Phase 0 V-item in the ledger, safely.

Editing `docs/phase-0-report.md` by hand-written regex cost three follow-up commits: a
replacement that dropped its trailing newline silently glued the next row onto it, and the
CI gate only caught it because it counts rows. This does the edit structurally instead —
find the row by ID, replace the whole line, and recount the summary from the rows that are
actually there rather than from a number somebody remembered to bump.

Evidence and notes are read from files, not from the command line. Backticks in a shell
argument are command substitution, and a note full of `code spans` silently loses every one of
them — which is how a ledger row ended up describing flags it never named.

Usage:
    ci/mark-phase0.py V-D03 PASS --evidence-file e.md --note-file n.md [--log-file l.md]
"""

import re
import sys
from pathlib import Path

REPORT = Path(__file__).resolve().parent.parent / "docs" / "phase-0-report.md"


def recount() -> int:
    text = REPORT.read_text(encoding="utf-8")
    lines = text.split("\n")
    apply_summary(lines)
    REPORT.write_text("\n".join(lines), encoding="utf-8")
    return 0


def apply_summary(lines: list[str]) -> None:
    # `[^|]+` for the state, not `[A-Z-]+(\(...\))?`: a row recorded as `PASS（history 路径）`
    # matched neither branch and was silently left out of the summary, so the totals read 62
    # when there are 63 items. The shape of a state is now enforced by
    # `ci/check-phase0-report.sh`; this only has to count rows, and it must count all of them.
    row = re.compile(r"^\| (V-[A-J]\d\d) \| ([^|]+?) \|")
    counts = {"PASS": 0, "FALLBACK-ADOPTED": 0, "IN-PROGRESS": 0, "BLOCKED": 0}
    for l in lines:
        m = row.match(l)
        if not m:
            continue
        s = m.group(2)
        key = (
            "FALLBACK-ADOPTED"
            if s.startswith("FALLBACK")
            else "BLOCKED"
            if s.startswith("BLOCKED")
            else s
        )
        if key in counts:
            counts[key] += 1
    for name, n in counts.items():
        summary = re.compile(rf"^\| {re.escape(name)} \| \d+ \|$")
        hits = [i for i, l in enumerate(lines) if summary.match(l)]
        if len(hits) == 1:
            lines[hits[0]] = f"| {name} | {n} |"
    print(
        f"PASS {counts['PASS']}, FALLBACK-ADOPTED {counts['FALLBACK-ADOPTED']}, "
        f"IN-PROGRESS {counts['IN-PROGRESS']}, BLOCKED {counts['BLOCKED']}, "
        f"total {sum(counts.values())}"
    )


def main() -> int:
    args = sys.argv[1:]
    if args == ["--recount"]:
        # Just fix the summary from the rows. The counts were hand-maintained for a while and
        # drifted: `PASS` was being incremented for an item recorded as FALLBACK-ADOPTED too,
        # so the three categories summed to one more than there are items.
        return recount()
    def take(flag: str) -> str | None:
        nonlocal args
        if flag not in args:
            return None
        i = args.index(flag)
        value = args[i + 1]
        args = args[:i] + args[i + 2 :]
        return value

    evidence_file = take("--evidence-file")
    note_file = take("--note-file")
    log_file = take("--log-file")
    log_line = take("--log")

    if evidence_file:
        evidence = Path(evidence_file).read_text(encoding="utf-8").strip()
        note = Path(note_file).read_text(encoding="utf-8").strip() if note_file else ""
        if log_file:
            log_line = Path(log_file).read_text(encoding="utf-8").strip()
        if len(args) != 2:
            print(__doc__, file=sys.stderr)
            return 2
        item, status = args
    else:
        if len(args) != 4:
            print(__doc__, file=sys.stderr)
            return 2
        item, status, evidence, note = args

    # A row is one line. A newline in the evidence or the note would split the table.
    evidence = " ".join(evidence.split())
    note = " ".join(note.split())
    if log_line:
        log_line = " ".join(log_line.split())

    text = REPORT.read_text(encoding="utf-8")
    lines = text.split("\n")

    # 1. Replace the row, keeping the line structure intact.
    pattern = re.compile(rf"^\| {re.escape(item)} \| ")
    matches = [i for i, l in enumerate(lines) if pattern.match(l)]
    if len(matches) != 1:
        print(f"error: {item} matched {len(matches)} rows, expected 1", file=sys.stderr)
        return 1
    lines[matches[0]] = f"| {item} | {status} | {evidence} | {note} |"

    # 2. Recount the summary from the rows themselves.
    apply_summary(lines)

    # 3. Prepend a changelog line, if one was given.
    if log_line:
        heads = [i for i, l in enumerate(lines) if l.startswith("| 日期 | 变更 |")]
        if not heads:
            print("error: no changelog table found", file=sys.stderr)
            return 1
        # The header is followed by the separator row; insert after it.
        lines.insert(heads[0] + 2, f"| 2026-09-16 | {log_line} |")

    REPORT.write_text("\n".join(lines), encoding="utf-8")
    print(f"{item} -> {status}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
