#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

mode="${1:-local}"
case "$mode" in
  local|phone) ;;
  *)
    echo "Usage: $0 [local|phone]" >&2
    exit 1
    ;;
esac

for command_name in curl docker jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

standalone_containers="$(docker ps -a \
  --filter label=com.docker.compose.project=oxid-standalone \
  --format '{{.ID}}')"
standalone_container_count="$(awk 'NF { count++ } END { print count + 0 }' <<<"$standalone_containers")"
if [ "$standalone_container_count" -ne 3 ]; then
  echo "Oxid standalone has $standalone_container_count containers; expected exactly three." >&2
  exit 1
fi

require_container_state() {
  local service expected container_ids container_id count actual
  service="$1"
  expected="$2"
  container_ids="$(docker ps -a \
    --filter label=com.docker.compose.project=oxid-standalone \
    --filter "label=com.docker.compose.service=$service" \
    --format '{{.ID}}')"
  count="$(awk 'NF { count++ } END { print count + 0 }' <<<"$container_ids")"
  if [ "$count" -ne 1 ]; then
    echo "Oxid standalone service '$service' has $count containers; expected exactly one." >&2
    exit 1
  fi
  container_id="$(awk 'NF { print; exit }' <<<"$container_ids")"
  actual="$(docker inspect --format '{{.State.Status}} {{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}} {{.State.ExitCode}}' "$container_id")"
  if [ "$actual" != "$expected" ]; then
    echo "Oxid standalone service '$service' is not ready: expected '$expected', found '$actual'." >&2
    exit 1
  fi
}

require_container_state node "running healthy 0"
require_container_state indexer "running healthy 0"
require_container_state proof-server "running none 0"

curl --fail --silent --show-error --max-time 5 \
  -o /dev/null http://127.0.0.1:6300/
node_height="$(curl --fail --silent --show-error --max-time 5 \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' \
  http://127.0.0.1:9944 | jq -r '.result.number // empty')"
indexer_height="$(curl --fail --silent --show-error --max-time 5 \
  -H 'content-type: application/json' \
  --data '{"query":"query StandaloneReadiness { block { height } }"}' \
  http://127.0.0.1:8088/api/v4/graphql | jq -r '.data.block.height // empty')"
if ! [[ "$node_height" =~ ^0x[0-9a-fA-F]+$ ]] || ! [[ "$indexer_height" =~ ^[0-9]+$ ]]; then
  echo "Oxid standalone node or indexer did not return a valid height." >&2
  exit 1
fi
node_height_decimal=$((16#${node_height#0x}))
if (( indexer_height + 4 < node_height_decimal )); then
  echo "Oxid standalone indexer is behind the allowed readiness window." >&2
  exit 1
fi

if [ "$mode" = "phone" ]; then
  if ! command -v tailscale >/dev/null 2>&1; then
    echo "Required command 'tailscale' is missing." >&2
    exit 1
  fi
  tailscale_status="$(tailscale status --json)"
  if [ "$(jq -r '.BackendState' <<<"$tailscale_status")" != "Running" ]; then
    echo "Tailscale is not connected." >&2
    exit 1
  fi
  tailnet_dns_name="$(jq -r '.Self.DNSName | rtrimstr(".")' <<<"$tailscale_status")"
  if [ -z "$tailnet_dns_name" ] || [ "$tailnet_dns_name" = "null" ]; then
    echo "Tailscale did not report a MagicDNS name." >&2
    exit 1
  fi
  tailscale serve status --json | jq -e '
    .TCP["443"].HTTPS == true
    and .TCP["8443"].HTTPS == true
    and .TCP["10000"].HTTPS == true
  ' >/dev/null
  curl --fail --silent --show-error --max-time 10 \
    -o /dev/null "https://$tailnet_dns_name/"
  curl --fail --silent --show-error --max-time 10 \
    -o /dev/null "https://$tailnet_dns_name:10000/health"
  curl --fail --silent --show-error --max-time 10 \
    -H 'content-type: application/json' \
    --data '{"query":"query StandaloneReadiness { block { height } }"}' \
    -o /dev/null "https://$tailnet_dns_name:8443/api/v4/graphql"
fi

echo "oxid standalone ($mode): READY"
