#!/usr/bin/env bash
# V-F01 gate: the committed catalog snapshots must be reproducible from their pinned images.
#
# A snapshot is the authority for what every command does (ADR-030). A hand-edited one would
# be indistinguishable from a captured one, so this re-captures from the digest recorded
# *inside each snapshot* and requires the command list to come back byte-identical. Only
# `provenance.captured_utc` is allowed to differ, because that is the capture time.
set -euo pipefail

cd "$(dirname "$0")/.."
FAIL=0

# The fingerprints must match the files first; everything else is moot otherwise.
cargo run -q -p xtask -- catalog-verify crates/pr-catalog/snapshots/*.json

for f in crates/pr-catalog/snapshots/*.json; do
  family=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['provenance']['family'])" "$f")
  line=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['provenance']['line'])" "$f")
  image=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['provenance']['image'])" "$f")

  case "$image" in
    *@sha256:*) ;;
    *) echo "FAIL $f: provenance.image is not digest-pinned ($image)" >&2; FAIL=1; continue ;;
  esac

  tmp=$(mktemp -d)
  echo "re-capturing $family $line from $image"
  ci/catalog-snapshot.sh "$family" "$line" "$image" "$tmp/fresh.json" >/dev/null

  if python3 - "$f" "$tmp/fresh.json" <<'PY'
import json, sys
a = json.load(open(sys.argv[1], encoding="utf-8"))
b = json.load(open(sys.argv[2], encoding="utf-8"))
for d in (a, b):
    d["provenance"].pop("captured_utc", None)
sys.exit(0 if a == b else 1)
PY
  then
    echo "OK   $f reproduces from its pinned image"
  else
    echo "FAIL $f does not reproduce from $image — it has been edited by hand" >&2
    diff <(python3 -m json.tool "$f") <(python3 -m json.tool "$tmp/fresh.json") | head -40 >&2 || true
    FAIL=1
  fi
  rm -rf "$tmp"
done

# V-I02: the divergence list is generated, not written by hand.
echo "regenerating the Redis/Valkey difference list"
tmpdiff="$(mktemp -d)"
cargo run -q -p xtask -- catalog-diff "$tmpdiff" >/dev/null
if diff -r fixtures/catalog/valkey-diff "$tmpdiff" >/dev/null; then
  echo "OK   fixtures/catalog/valkey-diff is current"
else
  echo "FAIL fixtures/catalog/valkey-diff is stale; re-run: cargo run -p xtask -- catalog-diff" >&2
  diff -r fixtures/catalog/valkey-diff "$tmpdiff" | head -40 >&2 || true
  FAIL=1
fi
rm -rf "$tmpdiff"

exit "$FAIL"
