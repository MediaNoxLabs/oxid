#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for required_command in docker openssl jq curl; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "Required command '$required_command' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
state_directory="${OXID_STANDALONE_STATE_DIR:-${TMPDIR:-/tmp}/oxid-standalone}"
environment_file="$state_directory/canonical-indexer.env"
serve_marker="$state_directory/tailscale-serve-owned"
source_compose_file="$repository_root/scripts/standalone-stack.yml"
compose_file="$state_directory/canonical-compose.yml"
lease_directory="$state_directory/startup-lease"
lease_record="$lease_directory/owner.json"
owner_receipt="$state_directory/owner-receipt.json"
session_id="$(printf '%s' "$repository_root" | shasum -a 256 | awk '{print $1}')"
lease_id="$(openssl rand -hex 16)"
mode="${1:-local}"

case "$mode" in
  local|phone)
    ;;
  *)
    echo "Usage: $0 [local|phone]" >&2
    exit 1
    ;;
esac

if ! docker info >/dev/null 2>&1; then
  echo "Docker is not running." >&2
  exit 1
fi

umask 077
mkdir -p "$state_directory"
chmod 700 "$state_directory"
if mkdir "$lease_directory" 2>/dev/null; then
  chmod 700 "$lease_directory"
  jq -cn --arg session "$session_id" --arg lease "$lease_id" \
    '{schema:"oxid-standalone-lease-v1",session:$session,lease:$lease}' >"$lease_record"
  chmod 600 "$lease_record"
elif [ -f "$lease_record" ] && jq -e \
  '.schema == "oxid-standalone-lease-v1" and (.session | type == "string") and (.lease | type == "string")' \
  "$lease_record" >/dev/null 2>&1; then
  owner_prefix="$(jq -r '.session[0:12]' "$lease_record")"
  jq -cn --arg owner "$owner_prefix" \
    '{schema:"oxid-standalone-lease-v1",state:"contention",ownerPrefix:$owner}' >&2
  exit 2
else
  echo "Standalone startup lease ownership is ambiguous; refusing mutation." >&2
  exit 1
fi
release_startup_lease() {
  if [ -f "$lease_record" ] && jq -e --arg session "$session_id" --arg lease "$lease_id" \
    '.schema == "oxid-standalone-lease-v1" and .session == $session and .lease == $lease' \
    "$lease_record" >/dev/null 2>&1; then
    rm -f -- "$lease_record"
    rmdir -- "$lease_directory" 2>/dev/null || true
  fi
}
trap release_startup_lease EXIT

current_ids="$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --format '{{.ID}}' | sort | jq -Rsc 'split("\n") | map(select(length > 0))')"
current_count="$(jq -r 'length' <<<"$current_ids")"
if [ "$current_count" -eq 3 ]; then
  if [ ! -f "$compose_file" ] || [ ! -f "$owner_receipt" ] || ! jq -e \
    --arg compose "$(shasum -a 256 "$compose_file" | awk '{print $1}')" \
    --argjson containers "$current_ids" \
    '.schema == "oxid-standalone-owner-v1"
      and (.session | type == "string")
      and .composeSha256 == $compose
      and .containerIds == $containers' \
    "$owner_receipt" >/dev/null 2>&1; then
    echo "Standalone resources exist without a matching canonical owner receipt; preserving them." >&2
    exit 1
  fi
  echo "Reusing the healthy candidate standalone stack without Compose mutation."
elif [ "$current_count" -eq 0 ]; then
  candidate="$(mktemp "$state_directory/.canonical-compose.XXXXXX")"
  cp "$source_compose_file" "$candidate"
  chmod 600 "$candidate"
  mv "$candidate" "$compose_file"
  if [ ! -f "$environment_file" ]; then
    storage_password="$(openssl rand -hex 24)"
    pub_sub_password="$(openssl rand -hex 24)"
    ledger_password="$(openssl rand -hex 24)"
    indexer_secret="$(openssl rand -hex 32)"
    {
      # The node transport is private to Compose; Tailnet ingress terminates TLS separately.
      node_transport="ws"
      printf 'APP__INFRA__NODE__URL=%s://node:9944\n' "$node_transport"
      printf 'APP__INFRA__STORAGE__PASSWORD=%s\n' "$storage_password"
      printf 'APP__INFRA__PUB_SUB__PASSWORD=%s\n' "$pub_sub_password"
      printf 'APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD=%s\n' "$ledger_password"
      printf 'APP__INFRA__SECRET=%s\n' "$indexer_secret"
    } >"$environment_file"
  fi
  chmod 600 "$environment_file"

  export OXID_STANDALONE_ENV_FILE="$environment_file"
  docker compose -p oxid-standalone -f "$compose_file" up -d --wait
  container_ids="$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --format '{{.ID}}' | sort | jq -Rsc 'split("\n") | map(select(length > 0))')"
  [ "$(jq -r 'length' <<<"$container_ids")" -eq 3 ] || {
    echo "Standalone startup did not produce exactly three containers; preserving state for diagnosis." >&2
    exit 1
  }
  jq -cn --arg session "$session_id" \
    --arg compose "$(shasum -a 256 "$compose_file" | awk '{print $1}')" \
    --argjson containers "$container_ids" \
    '{schema:"oxid-standalone-owner-v1",session:$session,composeSha256:$compose,containerIds:$containers}' \
    >"$owner_receipt"
  chmod 600 "$owner_receipt"
else
  echo "Standalone has $current_count containers; preserving ambiguous resources without mutation." >&2
  exit 1
fi

proof_server_ready=0
for attempt in {1..60}; do
  if curl --fail --silent --max-time 2 \
    -o /dev/null http://127.0.0.1:6300/ 2>/dev/null; then
    proof_server_ready=1
    break
  fi
  sleep 2
done
if [ "$proof_server_ready" != "1" ]; then
  echo "The standalone proof server did not become ready on loopback." >&2
  docker compose -p oxid-standalone -f "$compose_file" logs --tail 80 proof-server >&2
  exit 1
fi

# Container health means the GraphQL service can start, not that it has
# replayed the node. Transaction proving must not combine a stale indexer tip
# with current node finality, so wait for the bounded standalone lag first.
indexer_caught_up=0
for attempt in {1..600}; do
  node_height_hex="$(
    curl --fail --silent --max-time 2 \
      -H 'content-type: application/json' \
      --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' \
      http://127.0.0.1:9944 \
      | jq -r '.result.number // empty' 2>/dev/null \
      || true
  )"
  indexer_height="$(
    curl --fail --silent --max-time 2 \
      -H 'content-type: application/json' \
      --data '{"query":"query StandaloneReadiness { block { height } }"}' \
      http://127.0.0.1:8088/api/v4/graphql \
      | jq -r '.data.block.height // empty' 2>/dev/null \
      || true
  )"
  if [[ "$node_height_hex" =~ ^0x[0-9a-fA-F]+$ ]] \
    && [[ "$indexer_height" =~ ^[0-9]+$ ]]; then
    node_height=$((16#${node_height_hex#0x}))
    if (( indexer_height + 4 >= node_height )); then
      indexer_caught_up=1
      break
    fi
    if (( attempt % 30 == 0 )); then
      echo "Waiting for standalone indexer replay: indexer=$indexer_height node=$node_height"
    fi
  fi
  sleep 2
done
if [ "$indexer_caught_up" != "1" ]; then
  echo "The standalone indexer did not catch the node tip within 20 minutes." >&2
  exit 1
fi

echo "Oxid standalone node, indexer, and proof server are healthy on loopback."

if [ "$mode" = "local" ]; then
  echo "Indexer: http://127.0.0.1:8088/api/v4/graphql"
  echo "Node: ws://127.0.0.1:9944"
  echo "Proof server: http://127.0.0.1:6300"
  exit 0
fi

if ! command -v tailscale >/dev/null 2>&1; then
  echo "The Tailscale CLI is required for phone mode." >&2
  exit 1
fi
if [ "$(tailscale status --json | jq -r '.BackendState')" != "Running" ]; then
  echo "Tailscale is not connected." >&2
  exit 1
fi

if [ ! -f "$serve_marker" ]; then
  serve_status="$(tailscale serve status 2>&1 || true)"
  if [ "$serve_status" != "No serve config" ]; then
    echo "Tailscale Serve already has unrelated configuration; refusing to replace it." >&2
    exit 1
  fi
  : >"$serve_marker"
  cleanup_partial_serve_configuration() {
    tailscale serve reset >/dev/null 2>&1 || true
    rm -f "$serve_marker"
  }
  run_tailscale_serve() {
    tailscale serve --yes --bg "$@" &
    local serve_process=$!
    for _attempt in $(seq 1 15); do
      if ! kill -0 "$serve_process" 2>/dev/null; then
        wait "$serve_process"
        return
      fi
      sleep 1
    done
    if kill -0 "$serve_process" 2>/dev/null; then
      kill -TERM "$serve_process" 2>/dev/null || true
      wait "$serve_process" 2>/dev/null || true
    fi
    echo "Timed out waiting for Tailscale Serve enablement; enable it in the tailnet admin page and retry." >&2
    return 1
  }
  trap cleanup_partial_serve_configuration ERR
  trap 'cleanup_partial_serve_configuration; exit 1' INT TERM
  run_tailscale_serve --https=8443 http://127.0.0.1:8088
  run_tailscale_serve --https=10000 http://127.0.0.1:9944
  run_tailscale_serve --https=443 http://127.0.0.1:6300
  trap - ERR
  trap - INT TERM
fi

tailnet_dns_name="$(tailscale status --json | jq -r '.Self.DNSName | rtrimstr(".")')"
if [ -z "$tailnet_dns_name" ] || [ "$tailnet_dns_name" = "null" ]; then
  echo "Tailscale did not report a MagicDNS name." >&2
  exit 1
fi

curl --silent --show-error --max-time 10 -o /dev/null \
  "https://$tailnet_dns_name:8443/api/v4/graphql"
curl --silent --show-error --max-time 10 -o /dev/null \
  "https://$tailnet_dns_name:10000/health"
curl --silent --show-error --max-time 10 -o /dev/null \
  "https://$tailnet_dns_name/"

echo "Tailnet TLS routes are ready for the compile-time phone profile."
echo "Run: just android-phone"
