#!/usr/bin/env bash
# v2.1 §20.4 / §34.2 Compatibility gate: a target may claim `verified` only for what this
# repository actually runs against it. Claiming support the tests do not cover is exactly the
# degradation §38.2 forbids, and ADR-011 says the claim is per *version*, not in general.
#
# "Actually runs" is three questions, and the first version of this asked only the first:
#
#   1. Is the digest in the CI matrix at all?
#   2. **Which topologies** is it run in? `topologies = [...]` says what the server supports;
#      `verified_topologies = [...]` says what we drove. valkey lists cluster and only ever
#      runs standalone here, so a bare `verified` would have claimed a topology no job touches.
#   3. Is the run recorded? `verified_ci_run` names the run the claim rests on, so the evidence
#      is a link rather than a memory.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import re, sys

MANIFEST = "compatibility/manifest.toml"
WORKFLOW = ".github/workflows/ci.yml"

workflow = open(WORKFLOW, encoding="utf-8").read()
ci_digests = set(re.findall(r"sha256:[0-9a-f]{64}", workflow))

# Which digests the topology job drives. It brings the cluster and sentinel fixtures up from
# ci/topology/*.sh, so that is where the digest lives.
topo_digests = set()
import glob, os
for f in glob.glob("ci/topology/*.sh") + glob.glob("ci/topology/*/*.sh"):
    topo_digests |= set(re.findall(r"sha256:[0-9a-f]{64}", open(f, encoding="utf-8").read()))

fail = 0
blocks = re.split(r"^\[\[server\]\]$", open(MANIFEST, encoding="utf-8").read(), flags=re.M)[1:]
verified = 0
for b in blocks:
    def field(name):
        m = re.search(rf'^{name} = "([^"]*)"', b, re.M)
        return m.group(1) if m else None

    def list_field(name):
        m = re.search(rf"^{name} = \[(.*?)\]", b, re.M | re.S)
        return re.findall(r'"([^"]+)"', m.group(1)) if m else []

    status, digest = field("status"), field("digest")
    line, family = field("line"), field("family")
    if status != "verified":
        continue
    verified += 1
    who = f"{family} {line}"
    if not digest or digest not in ci_digests:
        print(f"::error::{who} claims 'verified' but no CI job runs its digest")
        fail = 1
        continue
    vt = list_field("verified_topologies")
    if not vt:
        print(f"::error::{who} claims 'verified' without verified_topologies; say what was run")
        fail = 1
        continue
    for t in vt:
        if t == "standalone":
            continue  # the server-matrix job runs every pinned digest standalone
        if digest not in topo_digests:
            print(f"::error::{who} claims verified topology '{t}', but no fixture under "
                  f"ci/topology/ starts that digest")
            fail = 1
    if not field("verified_ci_run"):
        print(f"::error::{who} claims 'verified' without verified_ci_run; name the run")
        fail = 1

# Platform capability table: no capability may be 'verified' while its V-item is open.
manifest = open(MANIFEST, encoding="utf-8").read()
if re.search(r'"(macos|linux|windows)[^"]*" = \{[^}]*verified', manifest):
    report = open("docs/phase-0-report.md", encoding="utf-8").read()
    if re.search(r"\| V-(C07|G05|D01) *\| *(IN-PROGRESS|BLOCKED)", report):
        print("::error::a platform capability claims 'verified' while V-C07/V-G05/V-D01 are open")
        fail = 1

print(f"{verified} target(s) claim verified; no unsupported claims." if not fail
      else "unsupported 'verified' claims above")
sys.exit(fail)
PY
