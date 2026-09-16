#!/usr/bin/env bash
# V-G01 — 6-node Redis Cluster (3 primaries + 3 replicas) pinned by digest.
set -euo pipefail
IMAGE="redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7"  # 7.4.11
NET=penguin-cluster
docker network create "$NET" >/dev/null 2>&1 || true
NODES=""
for i in 0 1 2 3 4 5; do
  port=$((7000 + i))
  docker run -d --name "pr-cl-$i" --network "$NET" -p "$port:$port" "$IMAGE" \
    redis-server --port "$port" --cluster-enabled yes \
      --cluster-config-file nodes.conf --cluster-node-timeout 5000 \
      --appendonly no --save '' >/dev/null
  NODES="$NODES 127.0.0.1:$port"
done
for i in 0 1 2 3 4 5; do
  port=$((7000 + i))
  for _ in $(seq 1 30); do
    if docker exec "pr-cl-$i" redis-cli -p "$port" PING 2>/dev/null | grep -q PONG; then break; fi
    sleep 1
  done
done
# shellcheck disable=SC2086
docker exec pr-cl-0 redis-cli -p 7000 --cluster create $NODES --cluster-replicas 1 --cluster-yes
for _ in $(seq 1 60); do
  if docker exec pr-cl-0 redis-cli -p 7000 CLUSTER INFO | grep -q 'cluster_state:ok'; then
    echo "cluster ready"; exit 0
  fi
  sleep 1
done
echo "::error::cluster did not reach cluster_state:ok" >&2
docker exec pr-cl-0 redis-cli -p 7000 CLUSTER INFO >&2 || true
exit 1
