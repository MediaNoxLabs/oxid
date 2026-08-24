#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Exact-owner lifecycle for the Oxid-owned shared Midnight project.

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=scripts/e2e/stack-env-v1.sh
source "$repository_root/scripts/e2e/stack-env-v1.sh"
readonly compose_file="$repository_root/scripts/standalone-stack.yml"
readonly receipt_name="oxid-standalone.owner.receipt"
readonly env_name="oxid-standalone.indexer.env"
operation=""
ownership=attach
container_ids=""
container_count=0
chain_height=""

fail_input() { printf 'standalone-lifecycle: error=%s\n' "$1" >&2; exit 2; }
fail_lifecycle() { printf 'standalone-lifecycle: error=%s\n' "$1" >&2; exit 1; }

[ "$#" -eq 2 ] || fail_input usage
operation="$1"
case "$operation" in ensure|attach|status|down) ;; *) fail_input usage ;; esac
if ! stack_env_load "$2"; then fail_input "$STACK_ENV_ERROR"; fi
for command_name in curl docker git jq mktemp openssl shasum sort stat; do
  command -v "$command_name" >/dev/null 2>&1 || fail_input missing_tool
done
[ -f "$compose_file" ] || fail_input invalid_compose

readonly state_directory="$LOCAL_STACK_STATE_DIR"
readonly environment_file="$state_directory/$env_name"
readonly receipt_file="$state_directory/$receipt_name"

count_lines() {
  local input="$1" count=0 line
  while IFS= read -r line; do [ -z "$line" ] || count=$((count + 1)); done <<<"$input"
  printf '%d\n' "$count"
}

query_containers() {
  local label all running
  label="label=com.docker.compose.project=$SHARED_MIDNIGHT_PROJECT"
  all="$(docker ps -a --filter "$label" --quiet 2>/dev/null | sort)" || return 1
  running="$(docker ps --filter "$label" --quiet 2>/dev/null | sort)" || return 1
  container_count="$(count_lines "$all")"
  container_ids="$all"
  [ "$container_count" -eq 0 ] || [ "$all" = "$running" ]
}

probe_indexer() {
  curl --fail --silent --output /dev/null --connect-timeout 2 --max-time 5 \
    -H 'content-type: application/json' \
    --data '{"query":"query OxidSharedReadiness { block { height } }"}' "$1" 2>/dev/null
}

probe_readiness_once() {
  local response hex
  query_containers || return 1
  [ "$container_count" -eq 3 ] || return 1
  curl --fail --silent --output /dev/null --connect-timeout 2 --max-time 5 \
    "$SHARED_MIDNIGHT_NODE_HOST_URL/health" 2>/dev/null || return 1
  probe_indexer "$SHARED_MIDNIGHT_INDEXER_V3_HOST_URL" || return 1
  probe_indexer "$SHARED_MIDNIGHT_INDEXER_V4_HOST_URL" || return 1
  curl --fail --silent --output /dev/null --connect-timeout 2 --max-time 5 \
    "$SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL/ready" 2>/dev/null || return 1
  response="$(curl --fail --silent --connect-timeout 2 --max-time 5 \
    -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' \
    "$SHARED_MIDNIGHT_NODE_HOST_URL" 2>/dev/null)" || return 1
  hex="$(printf '%s' "$response" | jq -r '.result.number // empty' 2>/dev/null)"
  [[ "$hex" =~ ^0x[0-9a-fA-F]+$ ]] || return 1
  chain_height="$((16#${hex#0x}))"
}

wait_readiness() {
  local attempt
  for attempt in $(seq 1 600); do
    if probe_readiness_once; then return 0; fi
    sleep 2
  done
  return 1
}

receipt_metadata_valid() {
  local owner mode size extra current_user
  [ -f "$receipt_file" ] && [ ! -L "$receipt_file" ] || return 1
  if read -r owner mode size extra < <(stat -c '%u %a %s' -- "$receipt_file" 2>/dev/null); then :
  elif read -r owner mode size extra < <(stat -f '%u %Lp %z' -- "$receipt_file" 2>/dev/null); then :
  else return 1; fi
  current_user="$(id -u)" || return 1
  [ "$owner" = "$current_user" ] && [ "$mode" = 600 ] && [ -z "${extra:-}" ] && ((10#$size <= 8192))
}

read_receipt() {
  local schema profile project digest expected_digest receipt_ids
  receipt_metadata_valid || return 1
  schema="$(sed -n '1p' "$receipt_file")"
  profile="$(sed -n '2p' "$receipt_file")"
  project="$(sed -n '3p' "$receipt_file")"
  digest="$(sed -n '4p' "$receipt_file")"
  receipt_ids="$(sed -n '5,$p' "$receipt_file")"
  expected_digest="$(shasum -a 256 "$compose_file" | awk '{print $1}')"
  [ "$schema" = oxid-standalone-owner-v1 ] &&
    [ "$profile" = "$STACK_ENV_PATH" ] &&
    [ "$project" = "$SHARED_MIDNIGHT_PROJECT" ] &&
    [ "$digest" = "$expected_digest" ] &&
    [ "$(count_lines "$receipt_ids")" -eq 3 ] || return 1
  [ "$receipt_ids" = "$container_ids" ]
}

write_receipt() {
  local candidate digest
  digest="$(shasum -a 256 "$compose_file" | awk '{print $1}')" || return 1
  candidate="$(umask 077 && mktemp "$state_directory/.oxid-owner.XXXXXX")" || return 1
  {
    printf 'oxid-standalone-owner-v1\n%s\n%s\n%s\n' \
      "$STACK_ENV_PATH" "$SHARED_MIDNIGHT_PROJECT" "$digest"
    printf '%s\n' "$container_ids"
  } >"$candidate" || { rm -f -- "$candidate"; return 1; }
  chmod 600 "$candidate" && mv -f -- "$candidate" "$receipt_file" || {
    rm -f -- "$candidate"
    return 1
  }
}

ensure_environment_file() {
  local candidate storage_password pub_sub_password ledger_password indexer_secret
  if [ -e "$environment_file" ] || [ -L "$environment_file" ]; then
    [ -f "$environment_file" ] && [ ! -L "$environment_file" ] || return 1
    chmod 600 "$environment_file" || return 1
    return 0
  fi
  storage_password="$(openssl rand -hex 24)" || return 1
  pub_sub_password="$(openssl rand -hex 24)" || return 1
  ledger_password="$(openssl rand -hex 24)" || return 1
  indexer_secret="$(openssl rand -hex 32)" || return 1
  candidate="$(umask 077 && mktemp "$state_directory/.indexer-env.XXXXXX")" || return 1
  {
    printf 'APP__INFRA__NODE__URL=%s\n' "ws:"'//node:9944'
    printf 'APP__INFRA__STORAGE__PASSWORD=%s\n' "$storage_password"
    printf 'APP__INFRA__PUB_SUB__PASSWORD=%s\n' "$pub_sub_password"
    printf 'APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD=%s\n' "$ledger_password"
    printf 'APP__INFRA__SECRET=%s\n' "$indexer_secret"
  } >"$candidate" || { rm -f -- "$candidate"; return 1; }
  chmod 600 "$candidate" && mv "$candidate" "$environment_file" || {
    rm -f -- "$candidate"
    return 1
  }
}

compose() {
  OXID_STANDALONE_ENV_FILE="$environment_file" \
    docker compose -p "$SHARED_MIDNIGHT_PROJECT" -f "$compose_file" "$@"
}

emit_result() {
  local state="$1"
  printf '{"schema":"oxid-standalone-lifecycle-v2","operation":"%s","profile":"headless","project":"oxid-standalone","state":"%s","ownership":"%s","containers":%d}\n' \
    "$operation" "$state" "$ownership" "$container_count"
}

run_status() {
  if ! query_containers; then fail_lifecycle status_query_failed; fi
  if [ "$container_count" -eq 0 ]; then
    if [ -e "$receipt_file" ] || [ -L "$receipt_file" ]; then fail_lifecycle stale_owner_receipt; fi
    ownership=attach
    emit_result stopped
    return
  fi
  probe_readiness_once || fail_lifecycle shared_midnight_unavailable
  if [ -e "$receipt_file" ] || [ -L "$receipt_file" ]; then
    read_receipt || fail_lifecycle ownership_conflict
    ownership=owner
  else
    ownership=attach
  fi
  emit_result ready
}

run_ensure() {
  query_containers || fail_lifecycle status_query_failed
  if [ "$container_count" -eq 3 ]; then
    probe_readiness_once || fail_lifecycle shared_midnight_unavailable
    if [ -e "$receipt_file" ] || [ -L "$receipt_file" ]; then
      read_receipt || fail_lifecycle ownership_conflict
      ownership=owner
    else ownership=attach; fi
    emit_result ready
    return
  fi
  [ "$container_count" -eq 0 ] || fail_lifecycle partial_shared_midnight
  [ ! -e "$receipt_file" ] && [ ! -L "$receipt_file" ] || fail_lifecycle stale_owner_receipt
  ensure_environment_file || fail_lifecycle private_state_failed
  compose up -d --wait || fail_lifecycle compose_up_failed
  if ! wait_readiness; then fail_lifecycle shared_midnight_unavailable; fi
  write_receipt || fail_lifecycle owner_receipt_failed
  ownership=owner
  emit_result ready
}

run_down() {
  query_containers || fail_lifecycle status_query_failed
  if [ ! -e "$receipt_file" ] && [ ! -L "$receipt_file" ]; then
    ownership=attach
    if [ "$container_count" -eq 0 ]; then emit_result stopped
    else probe_readiness_once || fail_lifecycle shared_midnight_unavailable; emit_result ready; fi
    return
  fi
  if [ "$container_count" -eq 0 ]; then
    receipt_metadata_valid || fail_lifecycle ownership_conflict
    rm -f -- "$receipt_file"
    ownership=owner
    emit_result stopped
    return
  fi
  [ "$container_count" -eq 3 ] || fail_lifecycle ownership_conflict
  read_receipt || fail_lifecycle ownership_conflict
  ownership=owner
  [ -f "$environment_file" ] && [ ! -L "$environment_file" ] || fail_lifecycle private_state_failed
  compose down --timeout 30 || fail_lifecycle compose_down_failed
  query_containers || fail_lifecycle down_query_failed
  [ "$container_count" -eq 0 ] || fail_lifecycle down_incomplete
  rm -f -- "$receipt_file"
  emit_result stopped
}

case "$operation" in
  ensure) run_ensure ;;
  attach|status) run_status ;;
  down) run_down ;;
esac
