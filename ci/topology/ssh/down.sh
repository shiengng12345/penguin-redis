#!/usr/bin/env bash
# V-G03 teardown. Only what this fixture created.
set -uo pipefail
for c in pr-ssh-bastion pr-ssh-r0 pr-ssh-r1 pr-ssh-r2; do
  docker rm -f "$c" >/dev/null 2>&1 || true
done
for n in pr-ssh-net pr-ssh-internal; do
  docker network rm "$n" >/dev/null 2>&1 || true
done
