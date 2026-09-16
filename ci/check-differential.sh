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
cargo run -q -p differential -- "$out"

if ! diff -ru tests/differential/report "$out"; then
  echo "error: the differential report changed. If the change is intended, run" >&2
  echo "       'cargo run -p differential' and commit the result." >&2
  exit 1
fi
echo "differential report reproduces byte for byte."
