#!/usr/bin/env bash
# V-G01 — 6-node Redis Cluster (3 primaries + 3 replicas), pinned by digest.
#
# Host networking is deliberate. A cluster client must be able to reach every node at the
# address the cluster *announces*, because that is what MOVED/ASK send it to (v2.1 §21.2).
# With a bridge network the nodes announce container addresses the host cannot reach, which
# is the §21.3 address-mapping problem — real, but not what this fixture is for.
#
# On a Linux CI runner this gives 127.0.0.1:7000-7005 directly on the host. On macOS/colima
# the ports live inside the VM, so a local client must run inside that namespace too; see
# cluster-probe.sh.
set -euo pipefail

IMAGE="redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7"  # 7.4.11
NODES=""

for i in 0 1 2 3 4 5; do
  port=$((7000 + i))
  docker run -d --name "pr-cl-$i" --network host "$IMAGE" \
    redis-server --port "$port" \
      --cluster-enabled yes \
      --cluster-config-file "nodes-$port.conf" \
      --cluster-node-timeout 5000 \
      --appendonly no --save '' >/dev/null
  NODES="$NODES 127.0.0.1:$port"
done

# Wait for every node to answer before forming the cluster.
for i in 0 1 2 3 4 5; do
  port=$((7000 + i))
  ready=0
  for _ in $(seq 1 30); do
    if docker exec "pr-cl-$i" redis-cli -p "$port" PING 2>/dev/null | grep -q PONG; then
      ready=1
      break
    fi
    sleep 1
  done
  if [ "$ready" -ne 1 ]; then
    echo "::error::node pr-cl-$i never answered on port $port" >&2
    docker logs "pr-cl-$i" 2>&1 | tail -20 >&2 || true
    exit 1
  fi
done

# shellcheck disable=SC2086
docker exec pr-cl-0 redis-cli -p 7000 --cluster create $NODES --cluster-replicas 1 --cluster-yes

for _ in $(seq 1 60); do
  if docker exec pr-cl-0 redis-cli -p 7000 CLUSTER INFO 2>/dev/null | grep -q 'cluster_state:ok'; then
    echo "cluster ready"
    docker exec pr-cl-0 redis-cli -p 7000 CLUSTER INFO | grep -E 'cluster_state|cluster_known_nodes|cluster_size'
    exit 0
  fi
  sleep 1
done

echo "::error::cluster did not reach cluster_state:ok" >&2
docker exec pr-cl-0 redis-cli -p 7000 CLUSTER INFO >&2 || true
for i in 0 1 2 3 4 5; do
  echo "--- pr-cl-$i ---" >&2
  docker logs "pr-cl-$i" 2>&1 | tail -15 >&2 || true
done
exit 1
