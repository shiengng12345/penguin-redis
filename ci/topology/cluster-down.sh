#!/usr/bin/env bash
set -uo pipefail
for i in 0 1 2 3 4 5; do docker rm -f "pr-cl-$i" >/dev/null 2>&1 || true; done
docker network rm penguin-cluster >/dev/null 2>&1 || true
echo "cluster torn down"
