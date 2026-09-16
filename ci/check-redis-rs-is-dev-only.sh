#!/usr/bin/env bash
# ADR-031 / V-B01: `redis-rs` is an interop TEST SUBJECT, never a product dependency.
#
# The decision in ADR-031 is only real if the crate cannot drift into the shipped binary, and
# "we agreed not to" is not a mechanism. `cargo tree -e no-dev` resolves what actually gets
# linked, so this catches it however it arrives -- directly, or pulled in by something else.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# 1. Nothing that ships may depend on it, at any depth.
if cargo tree --workspace -e no-dev --prefix none 2>/dev/null | grep -qE '^redis v'; then
  echo "error: redis-rs is reachable without dev-dependencies -- it would ship in the binary." >&2
  cargo tree --workspace -e no-dev --invert redis 2>/dev/null >&2 || true
  fail=1
fi

# 2. And it must be declared in exactly one place, so the exception stays visible in review.
declared=$(grep -rln '^redis = ' --include=Cargo.toml crates xtask 2>/dev/null \
  | sed 's|^\./||' | sort -u || true)
count=$(printf '%s' "$declared" | grep -c . || true)
if [ "$count" != "1" ]; then
  echo "error: expected exactly one crate to declare redis-rs, found $count:" >&2
  printf '%s\n' "$declared" >&2
  fail=1
elif [ "$declared" != "crates/pr-protocol/Cargo.toml" ]; then
  echo "error: redis-rs moved to $declared; ADR-031 puts the interop test in pr-protocol." >&2
  fail=1
fi

# 3. And that one declaration must be under [dev-dependencies].
if ! awk '/^\[dev-dependencies\]/{d=1;next} /^\[/{d=0} d && /^redis = /{found=1} END{exit !found}' \
     crates/pr-protocol/Cargo.toml; then
  echo "error: redis-rs is not under [dev-dependencies] in crates/pr-protocol/Cargo.toml." >&2
  fail=1
fi

if [ "$fail" = 0 ]; then
  echo "redis-rs is dev-only (ADR-031)."
fi
exit "$fail"
