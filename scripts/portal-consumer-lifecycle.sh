#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PROJECT="oxid-portal-consumer"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly COMPOSE_FILE="$REPOSITORY_ROOT/scripts/portal-consumer-stack.yml"
readonly OPERATION="${1:-}"
readonly SOURCE="${PORTAL_INTEGRATION_CHECKOUT:-}"
readonly STATE="${OXID_PORTAL_CONSUMER_STATE_DIR:-}"
readonly ENV_FILE="$STATE/runtime.env"
readonly RECEIPT="$STATE/owner-receipt.json"
readonly PRIVATE_LOG="$STATE/private.log"

fail() {
  printf 'portal-consumer-lifecycle: FAIL phase=%s\n' "$1" >&2
  exit 1
}

case "$OPERATION" in prerequisite|up|status|down) ;; *) fail usage ;; esac
for command_name in awk curl docker git jq nix openssl shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
[[ "$SOURCE" = /* && "$STATE" = /* ]] || fail paths
[ -d "$SOURCE" ] && [ ! -L "$SOURCE" ] || fail source
[ "$(git -C "$SOURCE" remote get-url origin 2>/dev/null)" = "$PORTAL_REMOTE" ] || fail source
[ "$(git -C "$SOURCE" rev-parse HEAD 2>/dev/null)" = "$PORTAL_COMMIT" ] || fail source
[ "$(git -C "$SOURCE" rev-parse 'HEAD^{tree}' 2>/dev/null)" = "$PORTAL_TREE" ] || fail source
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all 2>/dev/null)" ] || fail source
[ -f "$COMPOSE_FILE" ] || fail compose

umask 077
mkdir -p "$STATE"
chmod 700 "$STATE"
[ -d "$STATE" ] && [ ! -L "$STATE" ] || fail state

project_ids() {
  docker ps -a --filter "label=com.docker.compose.project=$PROJECT" --quiet 2>/dev/null | sort
}

running_ids() {
  docker ps --filter "label=com.docker.compose.project=$PROJECT" --quiet 2>/dev/null | sort
}

count_lines() {
  awk 'NF { count++ } END { print count + 0 }' <<<"$1"
}

shared_midnight_ready() {
  local all running labels
  all="$(docker ps -a --filter 'label=com.docker.compose.project=oxid-standalone' --quiet 2>/dev/null | sort)" || return 1
  running="$(docker ps --filter 'label=com.docker.compose.project=oxid-standalone' --quiet 2>/dev/null | sort)" || return 1
  [ "$(count_lines "$all")" -eq 3 ] && [ "$all" = "$running" ] || return 1
  labels="$(docker inspect --format '{{index .Config.Labels "com.docker.compose.service"}}' $all 2>/dev/null | sort)" || return 1
  [ "$labels" = $'indexer\nnode\nproof-server' ] || return 1
  curl --fail --silent --max-time 5 http://127.0.0.1:9944/health >/dev/null 2>&1 || return 1
  curl --fail --silent --max-time 5 -H 'content-type: application/json' \
    --data '{"query":"query PortalReadiness { block { height } }"}' \
    http://127.0.0.1:8088/api/v3/graphql | jq -e '.data.block.height >= 0' >/dev/null 2>&1 || return 1
  curl --fail --silent --max-time 5 -H 'content-type: application/json' \
    --data '{"query":"query PortalReadiness { block { height } }"}' \
    http://127.0.0.1:8088/api/v4/graphql | jq -e '.data.block.height >= 0' >/dev/null 2>&1 || return 1
  curl --fail --silent --max-time 5 http://127.0.0.1:6300/ready >/dev/null 2>&1
}

compose() {
  docker compose --env-file "$ENV_FILE" -p "$PROJECT" -f "$COMPOSE_FILE" "$@"
}

receipt_valid() {
  [ -f "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || return 1
  local mode
  if mode="$(stat -c '%a' -- "$RECEIPT" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' -- "$RECEIPT" 2>/dev/null)"; fi
  [ "$mode" = 600 ] || return 1
  jq -e \
    --arg commit "$PORTAL_COMMIT" \
    --arg tree "$PORTAL_TREE" \
    --arg compose "$(shasum -a 256 "$COMPOSE_FILE" | awk '{print $1}')" \
    --argjson ids "$(printf '%s\n' "$(project_ids)" | jq -Rsc 'split("\n") | map(select(length > 0)) | sort')" \
    '.schema == "oxid-portal-consumer-owner-v1"
      and .source == {commit:$commit,tree:$tree}
      and .composeSha256 == $compose
      and .project == "oxid-portal-consumer"
      and .containerIds == $ids
      and (.images | keys | sort == ["didManager","issuer","resolver"])' \
    "$RECEIPT" >/dev/null
}

emit_status() {
  local state="$1"
  if [ "$state" = running ] && receipt_valid; then
    jq -c '{schema:"oxid-portal-consumer-status-v1",state:"running",source:.source,images:.images}' "$RECEIPT"
  else
    jq -cn --arg state "$state" --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
      '{schema:"oxid-portal-consumer-status-v1",state:$state,source:{commit:$commit,tree:$tree}}'
  fi
}

build_image() {
  local attribute="$1" variable="$2" output image_id
  output="$(nix build --option access-tokens '' "$SOURCE#$attribute" --no-link --print-out-paths 2>>"$PRIVATE_LOG")" || return 1
  [ -f "$output" ] || return 1
  docker load <"$output" >>"$PRIVATE_LOG" 2>&1 || return 1
  case "$attribute" in
    midnight-did-resolver-image) image_id="$(docker image inspect --format '{{.Id}}' midnight-did-resolver:0.1.0 2>/dev/null)" ;;
    did-manager-image) image_id="$(docker image inspect --format '{{.Id}}' laceid-did-manager:0.1.0 2>/dev/null)" ;;
    issuer-image) image_id="$(docker image inspect --format '{{.Id}}' laceid-issuer:0.1.0 2>/dev/null)" ;;
    *) return 1 ;;
  esac
  [[ "$image_id" =~ ^sha256:[0-9a-f]{64}$ ]] || return 1
  printf -v "$variable" '%s' "$image_id"
}

run_prerequisite() {
  shared_midnight_ready || fail shared-midnight
  jq -cn '{schema:"oxid-portal-midnight-prerequisite-v1",state:"ready",project:"oxid-standalone"}'
}

run_up() {
  [ "$(count_lines "$(project_ids)")" -eq 0 ] || fail occupied-project
  [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
  shared_midnight_ready || fail shared-midnight
  : >"$PRIVATE_LOG"
  chmod 600 "$PRIVATE_LOG"
  local resolver_image did_manager_image issuer_image wallet_seed env_candidate receipt_candidate
  docker pull 'ghcr.io/smocker-dev/smocker@sha256:b4106c3aec1d58df09b6b94a89eba801298cbe5303f3c9236d105dbcaaaf4ab2' >>"$PRIVATE_LOG" 2>&1 || fail smocker
  build_image midnight-did-resolver-image resolver_image || fail resolver-image
  build_image did-manager-image did_manager_image || fail did-manager-image
  build_image issuer-image issuer_image || fail issuer-image
  wallet_seed="$(awk '$1 == "WALLET_SEED:" { gsub(/[\" ]/, "", $2); print $2 }' "$SOURCE/docker/docker-compose.yml")"
  [[ "$wallet_seed" =~ ^[0-9a-f]{64}$ ]] || fail wallet-input
  [[ "${PORTAL_ISSUER_URL:-}" =~ ^https?:// ]] || fail issuer-origin
  [[ "${PORTAL_HOLDER_RESOLVER_URL:-}" =~ ^http://host\.docker\.internal:[0-9]+$ ]] || fail holder-resolver
  env_candidate="$(mktemp "$STATE/.runtime-env.XXXXXX")"
  {
    printf 'PORTAL_RESOLVER_IMAGE=%s\n' "$resolver_image"
    printf 'PORTAL_DID_MANAGER_IMAGE=%s\n' "$did_manager_image"
    printf 'PORTAL_ISSUER_IMAGE=%s\n' "$issuer_image"
    printf 'PORTAL_WALLET_SEED=%s\n' "$wallet_seed"
    printf 'PORTAL_DID_MANAGER_API_KEY=%s\n' "$(openssl rand -hex 32)"
    printf 'PORTAL_DID_MANAGER_CONTROLLER_API_KEY=%s\n' "$(openssl rand -hex 32)"
    printf 'PORTAL_ISSUER_SESSION_TOKEN_SECRET=%s\n' "$(openssl rand -hex 32)"
    printf 'PORTAL_DIDIT_API_KEY=%s\n' "$(openssl rand -hex 32)"
    printf 'PORTAL_PRIVATE_INDEXER_WS_URL=%s%s\n' 'ws' '://host.docker.internal:8088/api/v3/graphql/ws'
    printf 'PORTAL_ISSUER_URL=%s\n' "$PORTAL_ISSUER_URL"
    printf 'PORTAL_ISSUER_REDIRECT_URL=%s/issue/pending.html\n' "${PORTAL_ISSUER_URL%/}"
    printf 'PORTAL_HOLDER_RESOLVER_URL=%s\n' "$PORTAL_HOLDER_RESOLVER_URL"
  } >"$env_candidate"
  chmod 600 "$env_candidate"
  mv "$env_candidate" "$ENV_FILE"
  cleanup_failed_up() {
    compose down --volumes --remove-orphans --timeout 30 >>"$PRIVATE_LOG" 2>&1 || true
    rm -f -- "$ENV_FILE" "$RECEIPT" "$PRIVATE_LOG"
  }
  trap cleanup_failed_up ERR INT TERM
  compose up -d --wait --wait-timeout 600 >>"$PRIVATE_LOG" 2>&1
  curl --fail --silent --show-error --max-time 30 -H 'Content-Type: application/x-yaml' \
    --data-binary "@$SOURCE/mock/didit.yml" 'http://127.0.0.1:8081/mocks?reset=true' \
    >>"$PRIVATE_LOG" 2>&1
  local ids running
  ids="$(project_ids)"; running="$(running_ids)"
  if [ "$(count_lines "$ids")" -ne 5 ] || [ "$(count_lines "$running")" -ne 4 ]; then
    cleanup_failed_up
    trap - ERR INT TERM
    fail project-shape
  fi
  receipt_candidate="$(mktemp "$STATE/.owner-receipt.XXXXXX")"
  jq -cn \
    --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
    --arg compose "$(shasum -a 256 "$COMPOSE_FILE" | awk '{print $1}')" \
    --arg resolver "$resolver_image" --arg didManager "$did_manager_image" --arg issuer "$issuer_image" \
    --argjson ids "$(printf '%s\n' "$ids" | jq -Rsc 'split("\n") | map(select(length > 0)) | sort')" \
    '{schema:"oxid-portal-consumer-owner-v1",source:{commit:$commit,tree:$tree},composeSha256:$compose,project:"oxid-portal-consumer",containerIds:$ids,images:{resolver:$resolver,didManager:$didManager,issuer:$issuer}}' \
    >"$receipt_candidate"
  chmod 600 "$receipt_candidate"
  mv "$receipt_candidate" "$RECEIPT"
  trap - ERR INT TERM
  emit_status running
}

run_status() {
  local ids running
  ids="$(project_ids)"; running="$(running_ids)"
  if [ -z "$ids" ]; then
    [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
    emit_status stopped
    return
  fi
  [ "$(count_lines "$ids")" -eq 5 ] && [ "$(count_lines "$running")" -eq 4 ] || fail project-shape
  receipt_valid || fail ownership
  emit_status running
}

run_down() {
  local ids
  ids="$(project_ids)"
  if [ -z "$ids" ]; then
    [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
    rm -f -- "$ENV_FILE" "$PRIVATE_LOG"
    emit_status stopped
    return
  fi
  receipt_valid || fail ownership
  [ -f "$ENV_FILE" ] && [ ! -L "$ENV_FILE" ] || fail private-state
  compose down --volumes --remove-orphans --timeout 30 >>"$PRIVATE_LOG" 2>&1 || fail cleanup
  [ -z "$(project_ids)" ] || fail cleanup-incomplete
  rm -f -- "$ENV_FILE" "$RECEIPT" "$PRIVATE_LOG"
  emit_status stopped
}

case "$OPERATION" in
  prerequisite) run_prerequisite ;;
  up) run_up ;;
  status) run_status ;;
  down) run_down ;;
esac
