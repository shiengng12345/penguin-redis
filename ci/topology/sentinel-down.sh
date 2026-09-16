#!/usr/bin/env bash
set -uo pipefail
for n in pr-sn-primary pr-sn-replica pr-sn-0 pr-sn-1 pr-sn-2; do docker rm -f "$n" >/dev/null 2>&1 || true; done
docker network rm penguin-sentinel >/dev/null 2>&1 || true
echo "sentinel torn down"
