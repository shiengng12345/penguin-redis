#!/usr/bin/env bash
# ADR ledger gate (phase-plan V-J01 / V-J04).
#
# Four things, each of which was a real gap at some point in Phase 0:
#
#   1. Every ADR is `accepted`. A `proposed` ADR that kernel code already cites is a decision
#      nobody finished making.
#   2. Every ADR has a non-empty "结果 / 验证记录" section. The template ships with a
#      placeholder comment; an ADR that still carries it is a decision with no evidence that
#      it survived contact with the code.
#   3. The README index and docs/adr/ agree in both directions. An ADR nobody links to is an
#      ADR nobody reads.
#   4. Every spike names an ADR that exists. V-J04's acceptance criterion is literally
#      "每个 spike 有 ADR"; this is that sentence, executable.
set -euo pipefail

DIR="docs/adr"
INDEX="$DIR/README.md"
fail=0

[ -d "$DIR" ] || { echo "::error::$DIR missing"; exit 1; }
[ -f "$INDEX" ] || { echo "::error::$INDEX missing"; exit 1; }

PLACEHOLDER='V 项完成后在此登记'

for f in "$DIR"/ADR-*.md; do
  id=$(basename "$f" .md)

  if ! grep -q '^- \*\*状态\*\*：accepted' "$f"; then
    echo "::error::$id is not accepted"
    fail=1
  fi

  if ! grep -q '^## 结果 / 验证记录' "$f"; then
    echo "::error::$id has no 「结果 / 验证记录」 section"
    fail=1
    continue
  fi

  if grep -q "$PLACEHOLDER" "$f"; then
    echo "::error::$id still carries the empty result placeholder; record PASS / FALLBACK-ADOPTED with evidence"
    fail=1
    continue
  fi

  # The section must say something. Blank lines and the heading itself do not count.
  body=$(sed -n '/^## 结果 \/ 验证记录/,$p' "$f" | tail -n +2 | tr -d '[:space:]')
  if [ -z "$body" ]; then
    echo "::error::$id has an empty 「结果 / 验证记录」 section"
    fail=1
  fi

  if ! grep -q "($id.md)" "$INDEX"; then
    echo "::error::$id is not listed in $INDEX"
    fail=1
  fi
done

# The index may not point at ADRs that do not exist.
while IFS= read -r id; do
  [ -f "$DIR/$id.md" ] || { echo "::error::$INDEX lists $id but $DIR/$id.md does not exist"; fail=1; }
done < <(grep -oE 'ADR-[0-9]{3}\.md' "$INDEX" | sed 's/\.md$//' | sort -u)

# Every spike names an ADR, and that ADR exists.
for s in docs/spikes/SPIKE-[0-9][0-9][0-9].md; do
  [ -f "$s" ] || continue
  sid=$(basename "$s" .md)
  adrs=$(grep -oE 'ADR-[0-9]{3}' "$s" | sort -u || true)
  if [ -z "$adrs" ]; then
    echo "::error::$sid names no ADR (V-J04: 每个 spike 有 ADR)"
    fail=1
    continue
  fi
  found=0
  for a in $adrs; do
    if [ -f "$DIR/$a.md" ] && grep -q "$sid" "$DIR/$a.md"; then
      found=1
    fi
  done
  if [ "$found" -eq 0 ]; then
    echo "::error::$sid names $adrs but no such ADR records this spike's conclusion"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "ADR ledger: $(ls "$DIR"/ADR-*.md | wc -l | tr -d ' ') ADRs, all accepted, all with a recorded result, all indexed."
