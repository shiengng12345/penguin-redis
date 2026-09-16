#!/usr/bin/env bash
# Capture a pinned command-catalog snapshot from one digest-pinned server (V-F01, §11.4, ADR-030).
#
# The snapshot is a *build input*, not a runtime source. It is captured once, from a known
# image digest, reviewed as a diff, and committed. At runtime the compiled catalog is the sole
# authority for effects; a live server may only report availability (precedence.rs).
#
# Usage: ci/catalog-snapshot.sh <family> <line> <image@sha256:...> <out.json>
set -euo pipefail

FAMILY="${1:?family (redis|valkey)}"
LINE="${2:?version line, e.g. 8.0}"
IMAGE="${3:?image@sha256:...}"
OUT="${4:?output path}"

case "$FAMILY" in
  redis) CLI=redis-cli ;;
  valkey) CLI=valkey-cli ;;
  *) echo "unknown family: $FAMILY" >&2; exit 2 ;;
esac

NAME="pr-catsnap-$FAMILY-${LINE//./}"
docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" "$IMAGE" >/dev/null
trap 'docker rm -f "$NAME" >/dev/null 2>&1 || true' EXIT

for _ in $(seq 1 30); do
  [ "$(docker exec "$NAME" "$CLI" ping 2>/dev/null || true)" = "PONG" ] && break
  sleep 1
done
[ "$(docker exec "$NAME" "$CLI" ping 2>/dev/null || true)" = "PONG" ] || { echo "server never came up" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'docker rm -f "$NAME" >/dev/null 2>&1 || true; rm -rf "$TMP"' EXIT

docker exec "$NAME" "$CLI" -3 --json COMMAND DOCS > "$TMP/docs.json"
docker exec "$NAME" "$CLI" -3 --json COMMAND          > "$TMP/info.json"
VERSION="$(docker exec "$NAME" "$CLI" info server | tr -d '\r' | awk -F: "/^${FAMILY}_version:/ {print \$2}")"
[ -n "$VERSION" ] || VERSION="$(docker exec "$NAME" "$CLI" info server | tr -d '\r' | awk -F: '/^redis_version:/ {print $2}')"

FAMILY="$FAMILY" LINE="$LINE" IMAGE="$IMAGE" VERSION="$VERSION" OUT="$OUT" TMP="$TMP" \
python3 "$(dirname "$0")/catalog-normalise.py"

echo "wrote $OUT ($(wc -c < "$OUT") bytes)"
