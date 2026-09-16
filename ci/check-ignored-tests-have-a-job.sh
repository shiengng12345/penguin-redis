#!/usr/bin/env bash
# Every `#[ignore]`d test must name a CI job, and that job must exist.
#
# An ignored test is a test that does not run. That is fine when something else runs it — these
# need Docker, a real keychain, or half a gigabyte — and it is indistinguishable from dead code
# when nothing does. Phase 0 had three CI jobs that had never passed; the same blind spot one
# level down is a test nobody has ever executed.
#
# The check is deliberately shallow: it asserts the reason string names a job that exists in a
# workflow. It cannot prove the job runs that particular test, which is why the reason strings
# say `--ignored` and the jobs are short enough to read.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
jobs="$(grep -hoE '^  [a-z0-9-]+:' .github/workflows/*.yml | tr -d ' :' | sort -u)"

while IFS= read -r line; do
  file="${line%%:*}"
  reason="${line#*#\[ignore}"
  case "$reason" in
    *=*) : ;;
    *)
      echo "::error::$file has a bare #[ignore] with no reason; say which CI job runs it"
      fail=1
      continue
      ;;
  esac
  named=""
  for j in $jobs; do
    case "$reason" in *"$j"*) named="$j" ;; esac
  done
  if [ -z "$named" ]; then
    echo "::error::$file: #[ignore] reason names no CI job that exists: ${reason}"
    fail=1
  fi
# Doc comments explaining the convention mention `#[ignore]` too, and a checker that trips over
# its own documentation is a checker people delete. Only real attributes.
done < <(grep -rn '^\s*#\[ignore' --include='*.rs' crates xtask)

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "every #[ignore]d test names a CI job that exists."
