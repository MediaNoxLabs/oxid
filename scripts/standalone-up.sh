#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=
for required_command in docker openssl jq curl shasum; do
  command -v "$required_command" >/dev/null 2>&1 || { echo "Required command '$required_command' is missing." >&2; exit 1; }
done
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state_directory="${OXID_STANDALONE_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/oxid/standalone}"
environment_file="$state_directory/indexer.env"
owner_receipt="$state_directory/oxid-standalone.owner.receipt"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$repository_root/scripts/standalone-stack.yml"
mode="${1:-local}"
case "$mode" in local|phone) ;; *) echo "Usage: $0 [local|phone]" >&2; exit 1 ;; esac
case "$state_directory" in /*) ;; *) echo "Oxid standalone state directory must be absolute." >&2; exit 1 ;; esac
if ! docker info >/dev/null 2>&1; then echo "Docker is not running." >&2; exit 1; fi
umask 077
mkdir -p "$state_directory"
chmod 700 "$state_directory"

count_lines() { local count=0 line; while IFS= read -r line; do [ -z "$line" ] || count=$((count+1)); done <<<"$1"; printf '%d\n' "$count"; }
project_ids() { docker ps -a --filter 'label=com.docker.compose.project=oxid-standalone' --quiet 2>/dev/null | sort; }
running_ids() { docker ps --filter 'label=com.docker.compose.project=oxid-standalone' --quiet 2>/dev/null | sort; }
receipt_matches() {
  local ids="$1" schema owner project digest receipt_ids expected receipt_mode
  [ -f "$owner_receipt" ] && [ ! -L "$owner_receipt" ] || return 1
  if receipt_mode="$(stat -c '%a' -- "$owner_receipt" 2>/dev/null)"; then :; else receipt_mode="$(stat -f '%Lp' -- "$owner_receipt")"; fi
  [ "$receipt_mode" = 600 ] || return 1
  schema="$(sed -n '1p' "$owner_receipt")"; owner="$(sed -n '2p' "$owner_receipt")"
  project="$(sed -n '3p' "$owner_receipt")"; digest="$(sed -n '4p' "$owner_receipt")"
  receipt_ids="$(sed -n '5,$p' "$owner_receipt")"; expected="$(shasum -a 256 "$compose_file" | awk '{print $1}')"
  [ "$schema" = oxid-standalone-owner-v1 ] && [ "$owner" = ordinary ] &&
    [ "$project" = oxid-standalone ] && [ "$digest" = "$expected" ] && [ "$receipt_ids" = "$ids" ]
}
write_receipt() {
  local ids="$1" candidate digest
  candidate="$(mktemp "$state_directory/.owner.XXXXXX")"; digest="$(shasum -a 256 "$compose_file" | awk '{print $1}')"
  { printf 'oxid-standalone-owner-v1\nordinary\noxid-standalone\n%s\n' "$digest"; printf '%s\n' "$ids"; } >"$candidate"
  chmod 600 "$candidate"; mv -f "$candidate" "$owner_receipt"
}
ensure_environment() {
  local candidate storage_password pub_sub_password ledger_password indexer_secret
  if [ -f "$environment_file" ] && [ ! -L "$environment_file" ]; then chmod 600 "$environment_file"; return; fi
  [ ! -e "$environment_file" ] && [ ! -L "$environment_file" ] || return 1
  storage_password="$(openssl rand -hex 24)"; pub_sub_password="$(openssl rand -hex 24)"
  ledger_password="$(openssl rand -hex 24)"; indexer_secret="$(openssl rand -hex 32)"
  candidate="$(mktemp "$state_directory/.indexer.XXXXXX")"
  {
    printf 'APP__INFRA__NODE__URL=ws://node:9944\n'
    printf 'APP__INFRA__STORAGE__PASSWORD=%s\n' "$storage_password"
    printf 'APP__INFRA__PUB_SUB__PASSWORD=%s\n' "$pub_sub_password"
    printf 'APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD=%s\n' "$ledger_password"
    printf 'APP__INFRA__SECRET=%s\n' "$indexer_secret"
  } >"$candidate"
  chmod 600 "$candidate"; mv "$candidate" "$environment_file"
}

all_ids="$(project_ids)"; live_ids="$(running_ids)"; count="$(count_lines "$all_ids")"; started=0; ownership=attach
if [ "$count" -eq 0 ]; then
  [ ! -e "$owner_receipt" ] && [ ! -L "$owner_receipt" ] || { echo "Stale standalone ownership receipt; refusing mutation." >&2; exit 1; }
  ensure_environment
  OXID_STANDALONE_ENV_FILE="$environment_file" docker compose -p oxid-standalone -f "$compose_file" up -d --wait
  started=1; ownership=owner
elif [ "$count" -eq 3 ] && [ "$all_ids" = "$live_ids" ]; then
  if [ -e "$owner_receipt" ] || [ -L "$owner_receipt" ]; then
    receipt_matches "$all_ids" || { echo "Standalone ownership receipt does not match the running project." >&2; exit 1; }
    ownership=owner
  fi
else
  echo "Standalone project is partial; refusing mutation." >&2
  exit 1
fi

proof_server_ready=0
for _attempt in $(seq 1 60); do
  if curl --fail --silent --max-time 2 -o /dev/null http://127.0.0.1:6300/ready 2>/dev/null; then proof_server_ready=1; break; fi
  sleep 2
done
[ "$proof_server_ready" = 1 ] || { echo "The standalone proof server did not become ready on loopback." >&2; exit 1; }
indexer_caught_up=0
for attempt in $(seq 1 600); do
  node_height_hex="$(curl --fail --silent --max-time 2 -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' http://127.0.0.1:9944 | jq -r '.result.number // empty' 2>/dev/null || true)"
  v3_height="$(curl --fail --silent --max-time 2 -H 'content-type: application/json' --data '{"query":"query StandaloneReadiness { block { height } }"}' http://127.0.0.1:8088/api/v3/graphql | jq -r '.data.block.height // empty' 2>/dev/null || true)"
  v4_height="$(curl --fail --silent --max-time 2 -H 'content-type: application/json' --data '{"query":"query StandaloneReadiness { block { height } }"}' http://127.0.0.1:8088/api/v4/graphql | jq -r '.data.block.height // empty' 2>/dev/null || true)"
  if [[ "$node_height_hex" =~ ^0x[0-9a-fA-F]+$ && "$v3_height" =~ ^[0-9]+$ && "$v4_height" =~ ^[0-9]+$ ]]; then
    node_height=$((16#${node_height_hex#0x}))
    if ((v3_height + 4 >= node_height && v4_height + 4 >= node_height)); then indexer_caught_up=1; break; fi
    if ((attempt % 30 == 0)); then echo "Waiting for standalone indexer replay."; fi
  fi
  sleep 2
done
[ "$indexer_caught_up" = 1 ] || { echo "The standalone indexer did not catch the node tip within 20 minutes." >&2; exit 1; }
if [ "$started" = 1 ]; then all_ids="$(project_ids)"; [ "$(count_lines "$all_ids")" -eq 3 ] || exit 1; write_receipt "$all_ids"; fi

echo "Oxid standalone node, indexer v3/v4, and proof server are healthy on loopback ($ownership)."
if [ "$mode" = local ]; then exit 0; fi
[ "$ownership" = owner ] || { echo "Phone routes require the exact standalone owner receipt." >&2; exit 1; }
command -v tailscale >/dev/null 2>&1 || { echo "The Tailscale CLI is required for phone mode." >&2; exit 1; }
[ "$(tailscale status --json | jq -r '.BackendState')" = Running ] || { echo "Tailscale is not connected." >&2; exit 1; }
if [ ! -f "$serve_marker" ]; then
  [ "$(tailscale serve status 2>&1 || true)" = "No serve config" ] || { echo "Tailscale Serve already has unrelated configuration; refusing to replace it." >&2; exit 1; }
  : >"$serve_marker"; chmod 600 "$serve_marker"
  cleanup_partial() { tailscale serve reset >/dev/null 2>&1 || true; rm -f "$serve_marker"; }
  trap cleanup_partial ERR INT TERM
  tailscale serve --yes --bg --https=8443 http://127.0.0.1:8088
  tailscale serve --yes --bg --https=10000 http://127.0.0.1:9944
  tailscale serve --yes --bg --https=443 http://127.0.0.1:6300
  trap - ERR INT TERM
fi
echo "Tailnet TLS routes are ready for the compile-time phone profile."
