#!/usr/bin/env bash
# Every acceptance case the phase plan names must appear somewhere in the repository.
#
# §2.3 names 45 of them — NET-01, ASSIST-082, WIN-03, LIFE-03, PERF-04 and the rest — in the
# 依据 and 通过标准 columns. A case that appears in the plan and nowhere else is a criterion
# nobody wrote a test for, and it fails silently: the V item still gets a green tick from the
# tests that *were* written.
#
# This is the same failure the threat model's `no_sec_case_is_unclaimed` guards one level down,
# and the same one that let V-D06 sit at `PASS（history 路径）` while nine of its ten paths had
# no test at all.
#
# The check is deliberately a text search rather than an attempt to prove a case is *tested*.
# It cannot tell a test from a mention in a note, and it says so; what it can do is notice that
# a criterion has never been written down anywhere but the plan.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import os, re, sys

plan = "docs/penguin-redis-phase-plan.md"
cases = {}
for line in open(plan, encoding="utf-8"):
    m = re.match(r"^\| (V-[A-J]\d\d) \|", line)
    if not m:
        continue
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    if len(cells) != 8:
        continue
    # Column 3 is 依据 (the spec sections and cases), column 5 is 通过标准.
    for prefix, num in re.findall(r"\b([A-Z]{2,6})-(\d{2,3})\b", cells[3] + " " + cells[5]):
        if prefix in ("ADR", "V"):
            continue
        cases.setdefault(f"{prefix}-{num}", set()).add(cells[0])

hay = []
for root in ("crates", "xtask", "docs", "tests", "fixtures", "ci", "compatibility", "benches"):
    for dirpath, _, files in os.walk(root):
        for f in files:
            if f.endswith((".rs", ".md", ".toml", ".sh", ".yml")):
                p = os.path.join(dirpath, f)
                if os.path.abspath(p) == os.path.abspath(plan):
                    continue
                try:
                    hay.append(open(p, encoding="utf-8", errors="ignore").read())
                except OSError:
                    pass
hay = "\n".join(hay)

missing = sorted(c for c in cases if c not in hay)
for c in missing:
    print(f"::error::{c} is named by {sorted(cases[c])} in the plan and appears nowhere else")
print(f"{len(cases)} acceptance cases named in the plan, {len(missing)} unclaimed")
sys.exit(1 if missing else 0)
PY
