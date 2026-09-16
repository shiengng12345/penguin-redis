#!/usr/bin/env bash
# V-G02 — 1 primary + 1 replica + 3 sentinels, pinned by digest.
set -euo pipefail
IMAGE="redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7"  # 7.4.11
NET=penguin-sentinel
docker network create "$NET" >/dev/null 2>&1 || true
docker run -d --name pr-sn-primary --network "$NET" -p 6390:6379 "$IMAGE" \
  redis-server --port 6379 --appendonly no --save '' >/dev/null
docker run -d --name pr-sn-replica --network "$NET" -p 6391:6379 "$IMAGE" \
  redis-server --port 6379 --replicaof pr-sn-primary 6379 --appendonly no --save '' >/dev/null
for _ in $(seq 1 30); do
  docker exec pr-sn-primary redis-cli PING 2>/dev/null | grep -q PONG && break
  sleep 1
done
for i in 0 1 2; do
  port=$((26379 + i))
  docker run -d --name "pr-sn-$i" --network "$NET" -p "$port:26379" --entrypoint sh "$IMAGE" -c \
    "printf 'port 26379\nsentinel monitor mymaster pr-sn-primary 6379 2\nsentinel down-after-milliseconds mymaster 2000\nsentinel failover-timeout mymaster 10000\n' > /tmp/s.conf && redis-sentinel /tmp/s.conf" >/dev/null
done
for _ in $(seq 1 60); do
  if docker exec pr-sn-0 redis-cli -p 26379 SENTINEL master mymaster 2>/dev/null | grep -q mymaster; then
    echo "sentinel ready"; exit 0
  fi
  sleep 1
done
echo "::error::sentinel did not come up" >&2
exit 1
