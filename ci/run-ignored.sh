#!/usr/bin/env bash
# Run a suite's `#[ignore]`d tests and fail if none ran.
#
# `cargo test -- --ignored` exits 0 when the suite has no ignored tests left, so a job that
# invokes it stays green after the tests it was built for are renamed, moved or deleted. That
# is the same shape as the two failures this repository has already had: a `live_` filter that
# matched nothing with `--no-tests=pass`, and a `security_|secret_|...` filter that matched one
# parser test while the job called itself "safety".
#
# Usage: ci/run-ignored.sh -p pr-transport --test ssh_live -- --test-threads=1 --nocapture
#
# `--ignored` is added here so a caller cannot forget it and get the ordinary suite instead.
set -euo pipefail
cd "$(dirname "$0")/.."

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

# `--ignored` is a *test binary* argument, so it has to go after `--`. Appending it to the end
# of the caller's arguments only works when they already wrote one, and silently produces
# `cargo test --ignored` -- a usage error, exit 0 through the pipe -- when they did not.
args=()
saw_separator=0
for a in "$@"; do
  args+=("$a")
  if [ "$a" = "--" ]; then
    args+=("--ignored")
    saw_separator=1
  fi
done
if [ "$saw_separator" -eq 0 ]; then
  args+=("--" "--ignored")
fi

status=0
cargo test "${args[@]}" 2>&1 | tee "$log" || status=$?
if [ "$status" -ne 0 ]; then
  exit "$status"
fi

if ! grep -qE 'test result: ok\. [1-9][0-9]* passed' "$log"; then
  echo "::error::no ignored tests ran for: cargo test $* --ignored" >&2
  echo "::error::the suite passed vacuously, which is indistinguishable from it testing nothing" >&2
  exit 1
fi
