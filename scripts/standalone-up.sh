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
state_directory="$repository_root/target/standalone"
environment_file="$state_directory/indexer.env"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$repository_root/scripts/standalone-stack.yml"
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
if [ ! -f "$environment_file" ]; then
  storage_password="$(openssl rand -hex 24)"
  pub_sub_password="$(openssl rand -hex 24)"
  ledger_password="$(openssl rand -hex 24)"
  indexer_secret="$(openssl rand -hex 32)"
  {
    printf 'APP__INFRA__NODE__URL=ws://node:9944\n'
    printf 'APP__INFRA__STORAGE__PASSWORD=%s\n' "$storage_password"
    printf 'APP__INFRA__PUB_SUB__PASSWORD=%s\n' "$pub_sub_password"
    printf 'APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD=%s\n' "$ledger_password"
    printf 'APP__INFRA__SECRET=%s\n' "$indexer_secret"
  } >"$environment_file"
fi
chmod 600 "$environment_file"

export OXID_STANDALONE_ENV_FILE="$environment_file"
docker compose -p oxid-standalone -f "$compose_file" up -d --wait

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
