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

# 0. The status column is a state, not a sentence.
#
# `PASS（history 路径）` counted as PASS for weeks: the summary regex stops at the ASCII-only
# `[A-Z-]+`, so a full-width parenthetical after it was invisible to every check while being
# perfectly visible to a reader — and what it said was that one of ten required paths had been
# audited. A qualified pass is not one of §2.4's two terminal states. If the scope needs
# saying, it goes in the note column, where it does not look like a verdict.
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -nE 's/^\| ([A-Z0-9-]+) \|.*/\1/p')
  state=$(printf '%s' "$line" | sed -nE 's/^\| [A-Z0-9-]+ \| ([^|]*) \|.*/\1/p' | sed 's/[[:space:]]*$//')
  case "$state" in
    PASS|IN-PROGRESS) ;;
    FALLBACK-ADOPTED\(ADR-[0-9][0-9][0-9]\)|BLOCKED\(ADR-[0-9][0-9][0-9]\)) ;;
    *)
      echo "::error::$id has status '$state'. The only states are PASS, IN-PROGRESS, \
FALLBACK-ADOPTED(ADR-xxx) and BLOCKED(ADR-xxx); anything else belongs in the note column."
      fail=1
      ;;
  esac
done < <(rows)

# 1. Forbidden states, at any time.
if rows | grep -nE '\b(DEFERRED|SKIPPED|TODO|WONTFIX|N/?A)\b'; then
  echo "::error::$REPORT contains a forbidden state. Phase 0 items are PASS or FALLBACK-ADOPTED(ADR-xxx)."
  fail=1
fi

# 2. Every FALLBACK-ADOPTED and every BLOCKED must name an ADR that exists and is accepted.
#
# Matched on the *status column*, not anywhere in the row: a note that mentions the word
# FALLBACK-ADOPTED while explaining why an item is not one used to trip this check, which is
# the sort of false positive that gets a rule deleted.
#
# BLOCKED is held to the same bar as FALLBACK-ADOPTED. An item blocked without a recorded
# decision is indistinguishable from an item nobody finished, and the difference is the whole
# point of having the state.
check_adr_state() {
  state="$1"
  while IFS= read -r line; do
    adr=$(printf '%s' "$line" | sed -nE "s/^\\| [A-Z0-9-]+ \\| ${state}\\((ADR-[0-9]{3})\\) \\|.*/\\1/p")
    if [ -z "$adr" ]; then
      echo "::error::${state} without an ADR reference in its status column: $line"
      fail=1
      continue
    fi
    f="docs/adr/${adr}.md"
    if [ ! -f "$f" ]; then
      echo "::error::$adr referenced by the report but $f does not exist"
      fail=1
    elif ! grep -q '^- \*\*状态\*\*：accepted' "$f"; then
      echo "::error::$adr is referenced by a ${state} item but is not accepted"
      fail=1
    fi
  done < <(rows | grep -E "^\\| [A-Z0-9-]+ \\| ${state}" || true)
}
check_adr_state 'FALLBACK-ADOPTED'
check_adr_state 'BLOCKED'

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
