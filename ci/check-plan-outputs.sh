#!/usr/bin/env bash
# Every path the phase plan promises as a 产出 must exist, or be mapped to where it really is.
#
# §2.3's 产出 column is what a reviewer follows to find the evidence. Thirty-nine of its
# seventy-four paths point at directories that were never created: the plan said
# `tests/pty/harness/`, the harness landed in `xtask/pty-harness/`, and nothing reconciled them.
# The work exists in every case — the ledger cites it — but a reviewer following the plan finds
# an empty directory and has no way to tell that from missing work.
#
# So: a path either exists and is non-empty, or `docs/phase-0-outputs.md` says where it went.
# The map is checked too — a mapping that points at nothing is worse than no mapping.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import os, re, sys

plan = "docs/penguin-redis-phase-plan.md"
mapfile = "docs/phase-0-outputs.md"

def real(p):
    p = p.rstrip("/")
    if "*" in p or "{" in p:
        # A glob in the plan (`crates/*/src/contract.rs`) is a shape, not a path. The map has
        # to name it explicitly, and the map's target is what gets checked.
        return False
    return os.path.exists(p) and (os.path.isfile(p) or bool(os.listdir(p)))

mapped = {}
if os.path.exists(mapfile):
    for line in open(mapfile, encoding="utf-8"):
        m = re.match(r"^\| `([^`]+)` \| `([^`]+)` \|", line.strip())
        if m:
            mapped[m.group(1).rstrip("/")] = m.group(2).rstrip("/")

promised = {}
for line in open(plan, encoding="utf-8"):
    m = re.match(r"^\| (V-[A-J]\d\d) \|", line)
    if not m:
        continue
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    if len(cells) != 8:
        continue
    for p in re.findall(r"`([^`]+)`", cells[6]):
        if p.startswith(("tests/", "fixtures/", "crates/", "xtask/", "benches/", "docs/", "ci/",
                         "compatibility/")):
            promised.setdefault(p.rstrip("/"), set()).add(cells[0])

unmapped, broken = [], []
for p, items in sorted(promised.items()):
    if real(p):
        continue
    target = mapped.get(p)
    if target is None:
        unmapped.append((p, sorted(items)))
    elif not real(target):
        broken.append((p, target))

for p, items in unmapped:
    print(f"::error::the plan promises {p} for {items} and it does not exist; "
          f"add a row to {mapfile} saying where it went")
for p, target in broken:
    print(f"::error::{mapfile} maps {p} -> {target}, which does not exist either")

print(f"{len(promised)} promised paths: {len(promised) - len(unmapped) - len(broken)} resolved, "
      f"{len(unmapped)} unmapped, {len(broken)} mapped to nothing")
sys.exit(1 if unmapped or broken else 0)
PY
