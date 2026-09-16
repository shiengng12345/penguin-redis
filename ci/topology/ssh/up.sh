#!/usr/bin/env bash
# V-G03 — a jump host and a 3-node Redis cluster behind it (v2.1 §21.5, NET-07).
#
# Two networks, on purpose.
#
# The cluster lives on an `--internal` network, which Docker configures so that nothing
# outside it can reach in. The bastion is attached to both that and a published network, so
# it is the only way in -- which is the situation §21.5 is about, and the reason a single
# loopback forward is not enough: MOVED names an address on the internal side.
#
# The first version of this fixture used one ordinary bridge network, and the premise check
# caught it: on a host that routes bridge addresses, the cluster was reachable directly and
# the tunnel tests would have passed without the tunnel doing anything.
set -euo pipefail
cd "$(dirname "$0")"

NET=pr-ssh-net            # published: the bastion only
INNER=pr-ssh-internal     # --internal: the cluster, reachable only through the bastion
IMAGE="redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7"
WORK="${1:-$PWD/.work}"
mkdir -p "$WORK"

./down.sh >/dev/null 2>&1 || true
docker network create "$NET" >/dev/null
docker network create --internal "$INNER" >/dev/null

# A throwaway key pair for this fixture. Never reused, never committed.
rm -f "$WORK/id_ed25519" "$WORK/id_ed25519.pub"
ssh-keygen -t ed25519 -N '' -C penguin-vg03 -f "$WORK/id_ed25519" >/dev/null
cp "$WORK/id_ed25519.pub" ./authorized_keys

docker build -q -t pr-vg03-bastion . >/dev/null
rm -f ./authorized_keys

for i in 0 1 2; do
  docker run -d --name "pr-ssh-r$i" --network "$INNER" "$IMAGE" \
    redis-server --port 6379 --cluster-enabled yes \
      --cluster-config-file nodes.conf --cluster-node-timeout 5000 \
      --appendonly no --save '' >/dev/null
done

# Publish only the bastion. The cluster is reachable *through* it and nowhere else, which is
# the situation §21.5 is about.
docker run -d --name pr-ssh-bastion --network "$NET" -P pr-vg03-bastion >/dev/null
docker network connect "$INNER" pr-ssh-bastion

for i in 0 1 2; do
  for _ in $(seq 1 100); do
    if docker exec "pr-ssh-r$i" redis-cli PING 2>/dev/null | grep -q PONG; then break; fi
    sleep 0.2
  done
done

ADDRS=""
for i in 0 1 2; do
  ip=$(docker inspect -f "{{(index .NetworkSettings.Networks \"$INNER\").IPAddress}}" "pr-ssh-r$i")
  ADDRS="$ADDRS $ip:6379"
done
# shellcheck disable=SC2086
docker exec pr-ssh-r0 redis-cli --cluster create $ADDRS --cluster-yes >/dev/null

for _ in $(seq 1 100); do
  if docker exec pr-ssh-r0 redis-cli CLUSTER INFO | grep -q 'cluster_state:ok'; then break; fi
  sleep 0.2
done

SSH_PORT=$(docker port pr-ssh-bastion 22/tcp | head -1 | awk -F: '{print $NF}')

# The known-hosts entry, so StrictHostKeyChecking=yes has something to check against. Scanned
# once, here, at setup -- the tests must never scan, or they would be testing a client that
# trusts on first use.
ssh-keyscan -p "$SSH_PORT" -H 127.0.0.1 > "$WORK/known_hosts" 2>/dev/null

{
  echo "ssh_port=$SSH_PORT"
  echo "identity=$WORK/id_ed25519"
  echo "known_hosts=$WORK/known_hosts"
  i=0
  for a in $ADDRS; do echo "node$i=$a"; i=$((i+1)); done
} > "$WORK/fixture.env"

cat "$WORK/fixture.env"
