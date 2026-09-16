#!/usr/bin/env bash
# v2.1 §20.4 / §34.2 Compatibility gate: a target may claim `verified` only if this
# repository actually runs a matrix job for it. Claiming support the tests do not cover is
# exactly the degradation §38.2 forbids.
set -euo pipefail

MANIFEST="compatibility/manifest.toml"
WORKFLOW=".github/workflows/ci.yml"
[ -f "$MANIFEST" ] || { echo "::error::$MANIFEST missing"; exit 1; }

fail=0

# Collect digests that appear in the CI matrix.
ci_digests=$(grep -oE 'sha256:[0-9a-f]{64}' "$WORKFLOW" | sort -u)

# Walk the manifest: for each [[server]] block capture status + digest.
status=""
digest=""
line_no=0
while IFS= read -r line; do
  line_no=$((line_no + 1))
  case "$line" in
    '[[server]]'*)
      status=""
      digest=""
      ;;
    'status = '*)
      status=$(printf '%s' "$line" | sed -E 's/status = "(.*)".*/\1/')
      if [ "$status" = "verified" ] && [ -n "$digest" ]; then
        if ! printf '%s\n' "$ci_digests" | grep -qF "$digest"; then
          echo "::error file=$MANIFEST,line=$line_no::target with digest $digest claims 'verified' but no CI matrix job runs it"
          fail=1
        fi
      fi
      ;;
    'digest = '*)
      digest=$(printf '%s' "$line" | sed -E 's/digest = "(.*)".*/\1/')
      ;;
  esac
done < "$MANIFEST"

# Platform capability table: no capability may be 'verified' while its V-item is open.
if grep -qE '"(macos|linux|windows)[^"]*" = \{[^}]*verified' "$MANIFEST"; then
  if grep -qE '\| V-(C07|G05|D01) *\| *(IN-PROGRESS|BLOCKED)' docs/phase-0-report.md 2>/dev/null; then
    echo "::error::a platform capability claims 'verified' while V-C07/V-G05/V-D01 are still open"
    fail=1
  fi
fi

if [ "$fail" -eq 0 ]; then
  echo "No unsupported 'verified' claims."
fi
exit "$fail"
