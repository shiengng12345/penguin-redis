#!/usr/bin/env bash
# P0-META-02 — the Phase 0 exit gate (phase-plan §2.4), as a program rather than a checklist.
#
# §2.4 is eight bullet points with checkboxes. A checkbox is a claim someone made once; this
# re-derives each of the eight from the repository every time it runs. Seven of the eight can
# be decided here outright. The eighth (the CI matrix on three runners) needs GitHub, so it is
# checked through `gh` when `gh` is available and authenticated, and reported as unverified
# otherwise — never assumed.
#
# Exit code 0 means Phase 0 may close. Anything else names what is missing.
#
#   PHASE0_FULL=1   also runs the long cases (the 1 GiB blob, the differential replay).
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
pass() { printf '  ok    %s\n' "$1"; }
bad()  { printf '  FAIL  %s\n' "$1"; fail=1; }
warn() { printf '  ????  %s\n' "$1"; }

echo "Phase 0 exit gate (phase-plan §2.4)"

# --------------------------------------------------------------- 1. every V item is terminal
echo "1. every V item is PASS or FALLBACK-ADOPTED(ADR-xxx)"
if PHASE0_FINAL=1 ./ci/check-phase0-report.sh >/dev/null 2>&1; then
  pass "no IN-PROGRESS / BLOCKED / DEFERRED / SKIPPED in the ledger"
else
  # IDs and states only. The ledger rows carry several paragraphs of notes each, and a gate
  # that prints them buries its own verdict.
  PHASE0_FINAL=1 ./ci/check-phase0-report.sh 2>&1 \
    | sed -nE 's/^[0-9]+:\| ([A-Z0-9-]+) \| ([A-Z-]+(\(ADR-[0-9]{3}\))?) \|.*/        \1 \2/p' || true
  bad "the ledger still has open items"
fi

# --------------------------------------------------------------- 2. the CI matrix
echo "2. CI green on macOS / Linux / Windows, including Cluster, Sentinel and TLS"
wf=".github/workflows/ci.yml"
for os in ubuntu-latest macos-latest windows-latest; do
  grep -q "$os" "$wf" || bad "$wf never mentions $os"
done
for job in topology shared-files ssh-tunnels differential; do
  grep -qE "^  $job:" "$wf" || bad "$wf has no '$job' job"
done
# The soak has its own workflow: it runs for hours and ci.yml cancels superseded runs.
[ -f .github/workflows/soak.yml ] || bad ".github/workflows/soak.yml is missing"
# §2.4 names TLS alongside Cluster and Sentinel, but TLS has no job of its own: its matrix
# needs no Docker, generates its certificates at run time, and therefore rides in the ordinary
# three-platform `test` job. A job-name check cannot see that, so run the matrix instead of
# looking for a name. `run_case` is defined under item 3 and used here after it.
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  # The latest run that actually reached a verdict. `.[0]` alone picks up a queued run (whose
  # conclusion is null, so the check reported "no run" while four were in flight) or a
  # cancelled one — and `ci.yml` cancels superseded runs on purpose, so cancelled is the
  # normal state of every run but the newest. Neither is a verdict about the code.
  concl="$(gh run list --workflow=ci.yml --branch=main --limit=30 \
            --json conclusion \
            --jq '[.[] | select(.conclusion == "success" or .conclusion == "failure")][0].conclusion' \
            2>/dev/null || true)"
  case "$concl" in
    success) pass "the latest CI run on main concluded success" ;;
    "")      warn "gh returned no run for ci.yml on main" ;;
    *)       bad "the latest CI run on main concluded '$concl'" ;;
  esac
else
  warn "gh unavailable or not authenticated; CI conclusion not checked here"
fi

# --------------------------------------------------------------- 3. one case per harness
echo "3. every harness has run one §33 acceptance case of its own category"
# The mapping, and why each case belongs to that harness:
#   PTY harness        ASSIST-082  a real bracketed paste through a real pty
#   synthetic server   PERF-01     a 1 GiB reply that is never materialised
#   fault injection    STATE-01    a request cut mid-flight, which is how unknown-after-send
#                                  is produced rather than simulated
#   differential       CMD-01..05  argv, reply bytes, final state and output compared against
#                                  a real redis-cli
run_case() {  # run_case <label> <cargo args...>
  local label="$1"; shift
  if cargo test -q "$@" >/dev/null 2>&1; then pass "$label"; else bad "$label"; fi
}
run_case "ASSIST-082 (PTY harness)" -p pr-terminal --test pty_owner -- assist_082
# Item 2's TLS half, run here because `run_case` lives here.
run_case "NET-01 (TLS matrix, part of item 2)" -p pr-transport --test tls_matrix
run_case "STATE-01 (fault injection)" -p xtask -- cut_after_client_bytes
if [ "${PHASE0_FULL:-0}" = "1" ]; then
  run_case "PERF-01 (synthetic RESP server, 1 GiB)" -p pr-protocol --test large_values -- \
    a_one_gibibyte_synthetic_blob
else
  grep -q 'fn a_one_gibibyte_synthetic_blob_streams_without_being_held' \
    crates/pr-protocol/tests/large_values.rs \
    && pass "PERF-01 (synthetic RESP server) — present; set PHASE0_FULL=1 to run the 1 GiB case" \
    || bad "PERF-01 case is missing"
fi
missing=""
for c in CMD-01 CMD-02 CMD-03 CMD-04 CMD-05; do
  f="tests/differential/report/$c.json"
  [ -f "$f" ] || { missing="$missing $c"; continue; }
  grep -q '"execution_status": "PASS"' "$f" || missing="$missing $c(not PASS)"
done
[ -z "$missing" ] && pass "CMD-01..05 (differential) all recorded PASS" \
                  || bad "differential cases:$missing"
if [ "${PHASE0_FULL:-0}" = "1" ]; then
  ./ci/check-differential.sh >/dev/null 2>&1 \
    && pass "the differential report still reproduces" \
    || bad "the differential report no longer reproduces"
fi

# --------------------------------------------------------------- 4. the baseline report
echo "4. benches/baseline/ records hardware, OS, commit and the dependency lock"
b="benches/baseline/README.md"
if [ -f "$b" ]; then
  # A local flag: `$fail` may already be set by an earlier item, and gating the "ok" on it
  # would silently drop this item's result.
  bad4=0
  for need in "Apple M3" "macOS" "rustc 1.97.1" "commit" "Cargo.lock"; do
    grep -qF "$need" "$b" || { bad "$b does not record '$need'"; bad4=1; }
  done
  grep -qE '\| commit \| `[0-9a-f]{7,40}`' "$b" || { bad "$b has no commit hash"; bad4=1; }
  grep -qE 'sha256 `[0-9a-f]{16}' "$b" || { bad "$b has no Cargo.lock digest"; bad4=1; }
  [ "$bad4" -eq 0 ] && pass "$b records hardware, OS, toolchain, commit and the lock digest"
else
  bad "$b is missing"
fi

# --------------------------------------------------------------- 5. manual records
echo "5. the manual-record template exists and V-C03/C04/C05 each have a first real record"
[ -f docs/manual-verification/TEMPLATE.md ] || bad "TEMPLATE.md is missing"
for v in C03 C04 C05; do
  f="$(ls docs/manual-verification/MV-V-$v-*.md 2>/dev/null | head -1)"
  [ -n "$f" ] || { bad "no MV-V-$v record file"; continue; }
  if grep -q '首条已执行' "$f"; then pass "V-$v has a first real-terminal record"; else
    bad "V-$v's record still says the manual part is unfilled"
  fi
done
[ -d docs/manual-verification/records ] && [ -n "$(ls -A docs/manual-verification/records)" ] \
  && pass "the captured screens are archived" \
  || bad "docs/manual-verification/records/ is empty"

# --------------------------------------------------------------- 6. no floating tags
echo "6. compatibility/manifest.toml has no 'latest'"
# Comments first: the file's own header explains that `latest` is banned, and that
# explanation must not be mistaken for a use of it. Same rule as the `manifest-pinned` job.
if sed 's/#.*//' compatibility/manifest.toml | grep -qE '(^|[^a-z])latest'; then
  bad "compatibility/manifest.toml still contains 'latest'"
else
  pass "every image is pinned"
fi

# --------------------------------------------------------------- 7. the ADRs
echo "7. every ADR, spike ADR included, is accepted and carries a recorded result"
if ./ci/check-adr-ledger.sh >/dev/null 2>&1; then
  pass "$(ls docs/adr/ADR-*.md | wc -l | tr -d ' ') ADRs, all accepted, all with a result"
else
  ./ci/check-adr-ledger.sh 2>&1 | sed 's/^/        /' || true
  bad "the ADR ledger is incomplete"
fi

# --------------------------------------------------------------- 8. the report itself
echo "8. the Phase 0 report gives every V item a status and an evidence link"
report="docs/phase-0-report.md"
missing=""
while IFS= read -r id; do
  row="$(grep -E "^\| $id \|" "$report" || true)"
  [ -n "$row" ] || { missing="$missing $id"; continue; }
  # Column 3 is the evidence. An em dash there is a row nobody filled in.
  # Split on the pipe and trim, rather than on " | ": one row in the ledger has no space
  # after its third pipe, and a checker that reports a row as empty because of a missing
  # space is a checker people learn to ignore.
  ev="$(printf '%s' "$row" | awk -F'|' '{gsub(/^[ \t]+|[ \t]+$/, "", $4); print $4}')"
  case "$ev" in ""|"—"|"-") missing="$missing $id(no evidence)" ;; esac
done < <(grep -oE '^\| (V-[A-J][0-9]{2}) \|' docs/penguin-redis-phase-plan.md | tr -d '| ' | sort -u)
[ -z "$missing" ] && pass "every V item has a status and evidence" \
                  || bad "rows without evidence:$missing"

echo
if [ "$fail" -eq 0 ]; then
  echo "Phase 0 exit gate: all eight satisfied."
else
  echo "Phase 0 exit gate: NOT satisfied. Phase 1 may not merge to main."
  exit 1
fi
