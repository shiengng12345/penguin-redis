#!/usr/bin/env bash
# V-A02: the committed differential report must still reproduce (v2.1 §32.2).
#
# Same discipline as ci/check-catalog-snapshots.sh: an artifact nobody regenerates is an
# artifact that describes a version of the code nobody is running any more.
set -euo pipefail
cd "$(dirname "$0")/.."

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

cargo build -q -p prc -p differential
# Not under `set -e`: when the run itself finds an undeclared difference, the generated report
# is the only description of it, and it lives in a directory this script is about to delete.
if ! cargo run -q -p differential -- "$out"; then
  echo "error: the differential run found undeclared differences. The generated report:" >&2
  for f in "$out"/*.json; do
    grep -q '"execution_status": "FAIL"' "$f" || continue
    echo "----- $f" >&2
    cat "$f" >&2
  done
  exit 1
fi

if ! diff -ru tests/differential/report "$out"; then
  echo "error: the differential report changed. If the change is intended, run" >&2
  echo "       'cargo run -p differential' and commit the result." >&2
  exit 1
fi
echo "differential report reproduces byte for byte."
