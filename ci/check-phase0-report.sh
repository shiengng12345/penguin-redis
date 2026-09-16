#!/usr/bin/env bash
# Phase 0 gate (phase-plan §2.4 / P0-META-02).
#
# The report may only contain states the plan allows. DEFERRED and SKIPPED do not exist:
# an item is either PASS, FALLBACK-ADOPTED with an accepted ADR, or still open.
#
# While Phase 0 is in progress, IN-PROGRESS and BLOCKED(...) are tolerated but reported.
# Set PHASE0_FINAL=1 to enforce the actual exit gate (every item terminal).
set -euo pipefail

REPORT="docs/phase-0-report.md"
[ -f "$REPORT" ] || { echo "::error::$REPORT missing"; exit 1; }

fail=0

# Only the status column of the item tables is a "state". Prose and the summary table are
# documentation about the rules, not claims about an item.
rows() { grep -E '^\| (V-[A-J][0-9]{2}|P0-META-[0-9]{2}) \|' "$REPORT"; }

# 1. Forbidden states, at any time.
if rows | grep -nE '\b(DEFERRED|SKIPPED|TODO|WONTFIX|N/?A)\b'; then
  echo "::error::$REPORT contains a forbidden state. Phase 0 items are PASS or FALLBACK-ADOPTED(ADR-xxx)."
  fail=1
fi

# 2. Every FALLBACK-ADOPTED must name an ADR that exists and is accepted.
while IFS= read -r line; do
  adr=$(printf '%s' "$line" | sed -nE 's/.*FALLBACK-ADOPTED\((ADR-[0-9]{3})\).*/\1/p')
  if [ -z "$adr" ]; then
    echo "::error::FALLBACK-ADOPTED without an ADR reference: $line"
    fail=1
    continue
  fi
  f="docs/adr/${adr}.md"
  if [ ! -f "$f" ]; then
    echo "::error::$adr referenced by the report but $f does not exist"
    fail=1
  elif ! grep -q '^- \*\*状态\*\*：accepted' "$f"; then
    echo "::error::$adr is referenced as an adopted fallback but is not accepted"
    fail=1
  fi
done < <(rows | grep -F 'FALLBACK-ADOPTED' || true)

# 3. Every V-item in the plan appears in the report exactly once.
plan="docs/penguin-redis-phase-plan.md"
if [ -f "$plan" ]; then
  # V-ids as defined in the plan's §2.3 tables (first column).
  while IFS= read -r id; do
    n=$(grep -cE "^\| $id \|" "$REPORT" || true)
    if [ "$n" -ne 1 ]; then
      echo "::error::$id appears $n times in $REPORT (expected exactly 1)"
      fail=1
    fi
  done < <(grep -oE '^\| (V-[A-J][0-9]{2}) \|' "$plan" | tr -d '| ' | sort -u)
fi

# 4. Terminal-state enforcement, only for the real gate.
if [ "${PHASE0_FINAL:-0}" = "1" ]; then
  if rows | grep -nE '\| (IN-PROGRESS|BLOCKED)'; then
    echo "::error::PHASE0_FINAL is set but items are still open. Phase 1 may not merge to main."
    fail=1
  fi
else
  open=$(rows | grep -cE '\| (IN-PROGRESS|BLOCKED)' || true)
  echo "Phase 0 in progress: $open item(s) still open (set PHASE0_FINAL=1 to enforce the exit gate)."
fi

exit "$fail"
