#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_COMMIT="25499870f84d77173c46e4af3021311decfb840b"
readonly PORTAL_TREE="2d845d2293603dfd8adce5362c8a9941e6ba78a9"
readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PROJECT="oxid-portal-consumer"
readonly SMOCKER_IMAGE="ghcr.io/smocker-dev/smocker@sha256:b4106c3aec1d58df09b6b94a89eba801298cbe5303f3c9236d105dbcaaaf4ab2"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly COMPOSE_FILE="$REPOSITORY_ROOT/scripts/portal-consumer-stack.yml"
readonly OPERATION="${1:-}"
readonly SOURCE="${PORTAL_INTEGRATION_CHECKOUT:-}"
readonly STATE="${OXID_PORTAL_CONSUMER_STATE_DIR:-}"
readonly ENV_FILE="$STATE/runtime.env"
readonly RECEIPT="$STATE/owner-receipt.json"
readonly STARTING_RECEIPT="$STATE/starting-receipt.json"
readonly PRIVATE_LOG="$STATE/private.log"
readonly PREPARED_RECEIPT="$STATE/prepared-receipt.json"
readonly PREPARE_CHECKPOINT="$STATE/prepare-checkpoint.json"
readonly PREPARE_LOCK="$STATE/prepare.lock"
readonly EXTERNAL_PREPARED_RECEIPT="${PORTAL_CONSUMER_PREPARED_RECEIPT:-}"
readonly TAILNET_MOCK_STATE="${PORTAL_TAILNET_MOCK_STATE_DIR:-}"
readonly TAILNET_MOCK_TRANSFORM="$REPOSITORY_ROOT/scripts/e2e/tailnet-mock-transform.mjs"

fail() {
  printf 'portal-consumer-lifecycle: FAIL phase=%s\n' "$1" >&2
  exit 1
}

case "$OPERATION" in prerequisite|prepare|prepared-status|up|status|down|services-up|services-status|services-stop) ;; *) fail usage ;; esac
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

private_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  local mode
  if mode="$(stat -c '%a' -- "$1" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' -- "$1")"; fi
  [ "$mode" = 600 ]
}

prepare_lock_held=0
release_prepare_lock() {
  if [ "$prepare_lock_held" -eq 1 ]; then
    rmdir -- "$PREPARE_LOCK" 2>/dev/null || true
    prepare_lock_held=0
  fi
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
  private_regular_file "$RECEIPT" || return 1
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

starting_receipt_valid() {
  private_regular_file "$STARTING_RECEIPT" || return 1
  jq -e \
    --arg commit "$PORTAL_COMMIT" \
    --arg tree "$PORTAL_TREE" \
    --arg compose "$(shasum -a 256 "$COMPOSE_FILE" | awk '{print $1}')" '
      .schema == "oxid-portal-consumer-starting-v1"
      and .source == {commit:$commit,tree:$tree}
      and .composeSha256 == $compose
      and .project == "oxid-portal-consumer"
      and (.startedAtEpoch | type == "number" and . >= 0)
    ' "$STARTING_RECEIPT" >/dev/null
}

image_tag_for() {
  case "$1" in
    midnight-did-resolver-image) printf '%s\n' midnight-did-resolver:0.1.0 ;;
    did-manager-image) printf '%s\n' laceid-did-manager:0.1.0 ;;
    issuer-image) printf '%s\n' laceid-issuer:0.1.0 ;;
    *) return 1 ;;
  esac
}

image_key_for() {
  case "$1" in
    midnight-did-resolver-image) printf '%s\n' resolver ;;
    did-manager-image) printf '%s\n' didManager ;;
    issuer-image) printf '%s\n' issuer ;;
    *) return 1 ;;
  esac
}

prepared_image_valid() {
  local prepared="$1" key="$2" tag="$3" prepared_directory output gc_root image_id digest current_id current_digest
  prepared_directory="$(dirname -- "$prepared")"
  [ -d "$prepared_directory" ] && [ ! -L "$prepared_directory" ] || return 1
  prepared_directory="$(cd -- "$prepared_directory" && pwd -P)" || return 1
  output="$(jq -r --arg key "$key" '.images[$key].outputPath // empty' "$prepared")"
  gc_root="$(jq -r --arg key "$key" '.images[$key].gcRoot // empty' "$prepared")"
  image_id="$(jq -r --arg key "$key" '.images[$key].id // empty' "$prepared")"
  digest="$(jq -r --arg key "$key" '.images[$key].digest // empty' "$prepared")"
  [[ "$output" = /nix/store/* ]] && [ -f "$output" ] || return 1
  [ "$gc_root" = "$prepared_directory/nix-$key" ] && [ -L "$gc_root" ] || return 1
  [ "$(readlink "$gc_root")" = "$output" ] || return 1
  [[ "$image_id" =~ ^sha256:[0-9a-f]{64}$ ]] || return 1
  [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || return 1
  current_digest="sha256:$(shasum -a 256 "$output" | awk '{print $1}')" || return 1
  [ "$digest" = "$current_digest" ] || return 1
  current_id="$(docker image inspect --format '{{.Id}}' "$tag" 2>/dev/null)" || return 1
  [ "$current_id" = "$image_id" ]
}

prepared_receipt_metadata_valid() {
  local prepared="$1" status="${2:-complete}" host_system
  private_regular_file "$prepared" || return 1
  host_system="$(nix eval --raw --impure --expr builtins.currentSystem 2>/dev/null)" || return 1
  jq -e \
    --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
    --arg status "$status" --arg host "$host_system" '
      .schema == "oxid-portal-consumer-prepared-v1"
      and .source == {commit:$commit,tree:$tree}
      and .status == $status
      and .hostSystem == $host
      and (.startedAtEpoch | type == "number" and . >= 0)
      and (.images | type == "object")
      and ((.images | keys) - ["resolver","didManager","issuer"] | length == 0)
      and ($status != "complete" or (
        (.completedAtEpoch | type == "number")
        and .completedAtEpoch >= .startedAtEpoch
        and (.metrics.prepareDurationSeconds | type == "number" and . >= 0)
      ))
    ' "$prepared" >/dev/null || return 1
}

prepared_receipt_valid() {
  local prepared="$1" status="${2:-complete}" key attribute tag
  prepared_receipt_metadata_valid "$prepared" "$status" || return 1
  [ "$status" = complete ] || return 0
  [ "$(jq -r '.images | keys | sort | join(",")' "$prepared")" = didManager,issuer,resolver ] || return 1
  for attribute in midnight-did-resolver-image did-manager-image issuer-image; do
    key="$(image_key_for "$attribute")"; tag="$(image_tag_for "$attribute")"
    if jq -e --arg key "$key" '.images[$key] != null' "$prepared" >/dev/null; then
      prepared_image_valid "$prepared" "$key" "$tag" || return 1
    else
      return 1
    fi
  done
  docker image inspect "$SMOCKER_IMAGE" >/dev/null 2>&1 || return 1
}

tailnet_mock_state_valid() {
  [ -n "$TAILNET_MOCK_STATE" ] || return 0
  [[ "$TAILNET_MOCK_STATE" = /* ]] || return 1
  [ -f "$TAILNET_MOCK_TRANSFORM" ] || return 1
  TAILNET_MOCK_FILE="$TAILNET_MOCK_STATE/didit-tailnet.yml"
  TAILNET_MOCK_RECEIPT="$TAILNET_MOCK_STATE/didit-tailnet-receipt.json"
  node "$TAILNET_MOCK_TRANSFORM" --validate "$TAILNET_MOCK_STATE" "$PORTAL_ISSUER_URL" >/dev/null
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
  local attribute="$1" variable="$2" output_variable="${3:-}" key gc_root output image_id
  key="$(image_key_for "$attribute")" || return 1
  gc_root="$STATE/nix-$key"
  output="$(nix build --option access-tokens '' "$SOURCE#$attribute" --out-link "$gc_root" --print-out-paths 2>>"$PRIVATE_LOG")" || return 1
  [ "$(count_lines "$output")" -eq 1 ] && [[ "$output" = /nix/store/* ]] && [ -f "$output" ] || return 1
  [ -L "$gc_root" ] && [ "$(readlink "$gc_root")" = "$output" ] || return 1
  docker load <"$output" >>"$PRIVATE_LOG" 2>&1 || return 1
  case "$attribute" in
    midnight-did-resolver-image) image_id="$(docker image inspect --format '{{.Id}}' midnight-did-resolver:0.1.0 2>/dev/null)" ;;
    did-manager-image) image_id="$(docker image inspect --format '{{.Id}}' laceid-did-manager:0.1.0 2>/dev/null)" ;;
    issuer-image) image_id="$(docker image inspect --format '{{.Id}}' laceid-issuer:0.1.0 2>/dev/null)" ;;
    *) return 1 ;;
  esac
  [[ "$image_id" =~ ^sha256:[0-9a-f]{64}$ ]] || return 1
  printf -v "$variable" '%s' "$image_id"
  if [ -n "$output_variable" ]; then
    printf -v "$output_variable" '%s' "$output"
  fi
}

run_prerequisite() {
  shared_midnight_ready || fail shared-midnight
  jq -cn '{schema:"oxid-portal-midnight-prerequisite-v1",state:"ready",project:"oxid-standalone"}'
}

emit_prepared_status() {
  jq -c '{
      schema:"oxid-portal-consumer-prepare-status-v1",
      state:"prepared",
      source:.source,
      metrics:.metrics,
      images:(.images | with_entries(.value = {
        id:.value.id,
        digest:.value.digest,
        durationSeconds:.value.durationSeconds,
        localCacheHit:.value.localCacheHit
      }))
    }' "$PREPARED_RECEIPT"
}

run_prepare() {
  local checkpoint_candidate started_at host_system attribute key tag prepared_image_id prepared_output prepared_digest
  local phase_started phase_duration cache_hit prepare_duration
  mkdir "$PREPARE_LOCK" 2>/dev/null || fail preparation-busy
  prepare_lock_held=1
  trap 'release_prepare_lock' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  [ "$(count_lines "$(project_ids)")" -eq 0 ] || fail occupied-project
  if prepared_receipt_valid "$PREPARED_RECEIPT" complete; then
    emit_prepared_status
    release_prepare_lock
    trap - EXIT INT TERM
    return
  fi
  if [ -e "$PREPARED_RECEIPT" ] || [ -L "$PREPARED_RECEIPT" ]; then
    prepared_receipt_metadata_valid "$PREPARED_RECEIPT" complete || fail stale-prepared-receipt
    [ ! -e "$PREPARE_CHECKPOINT" ] && [ ! -L "$PREPARE_CHECKPOINT" ] || fail ambiguous-prepare-state
    checkpoint_candidate="$(mktemp "$STATE/.prepare-checkpoint.XXXXXX")"
    jq '.status="partial" | del(.completedAtEpoch,.metrics)' "$PREPARED_RECEIPT" >"$checkpoint_candidate"
    chmod 600 "$checkpoint_candidate"
    mv "$checkpoint_candidate" "$PREPARE_CHECKPOINT"
    rm -f -- "$PREPARED_RECEIPT"
  fi
  : >"$PRIVATE_LOG"
  chmod 600 "$PRIVATE_LOG"
  docker pull "$SMOCKER_IMAGE" >>"$PRIVATE_LOG" 2>&1 || fail smocker

  if [ -e "$PREPARE_CHECKPOINT" ] || [ -L "$PREPARE_CHECKPOINT" ]; then
    prepared_receipt_valid "$PREPARE_CHECKPOINT" partial || fail prepare-checkpoint
  else
    started_at="$(date +%s)"
    host_system="$(nix eval --raw --impure --expr builtins.currentSystem 2>>"$PRIVATE_LOG")" || fail nix-system
    checkpoint_candidate="$(mktemp "$STATE/.prepare-checkpoint.XXXXXX")"
    jq -cn \
      --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
      --arg host "$host_system" --argjson started "$started_at" \
      '{schema:"oxid-portal-consumer-prepared-v1",status:"partial",source:{commit:$commit,tree:$tree},hostSystem:$host,startedAtEpoch:$started,images:{}}' \
      >"$checkpoint_candidate"
    chmod 600 "$checkpoint_candidate"
    mv "$checkpoint_candidate" "$PREPARE_CHECKPOINT"
  fi

  for attribute in midnight-did-resolver-image did-manager-image issuer-image; do
    key="$(image_key_for "$attribute")"; tag="$(image_tag_for "$attribute")"
    if jq -e --arg key "$key" '.images[$key] != null' "$PREPARE_CHECKPOINT" >/dev/null \
      && prepared_image_valid "$PREPARE_CHECKPOINT" "$key" "$tag"; then
      continue
    fi
    cache_hit=false
    if nix path-info --option access-tokens '' "$SOURCE#$attribute" >/dev/null 2>&1; then
      cache_hit=true
    fi
    phase_started="$(date +%s)"
    build_image "$attribute" prepared_image_id prepared_output || fail "$key-image"
    prepared_digest="sha256:$(shasum -a 256 "$prepared_output" | awk '{print $1}')" || fail "$key-digest"
    phase_duration="$(( $(date +%s) - phase_started ))"
    checkpoint_candidate="$(mktemp "$STATE/.prepare-checkpoint.XXXXXX")"
    jq \
      --arg key "$key" --arg attribute "$attribute" --arg output "$prepared_output" \
      --arg root "$STATE/nix-$key" --arg id "$prepared_image_id" --arg digest "$prepared_digest" \
      --argjson duration "$phase_duration" --argjson cacheHit "$cache_hit" \
      '.images[$key] = {
        attribute:$attribute,
        outputPath:$output,
        gcRoot:$root,
        id:$id,
        digest:$digest,
        durationSeconds:$duration,
        localCacheHit:$cacheHit,
        downloadedBytes:null
      }' "$PREPARE_CHECKPOINT" >"$checkpoint_candidate"
    chmod 600 "$checkpoint_candidate"
    mv "$checkpoint_candidate" "$PREPARE_CHECKPOINT"
    prepared_receipt_valid "$PREPARE_CHECKPOINT" partial || fail prepare-checkpoint
  done

  prepared_receipt_valid "$PREPARE_CHECKPOINT" partial || fail prepare-checkpoint
  prepare_duration="$(jq '[.images[].durationSeconds] | add // 0' "$PREPARE_CHECKPOINT")"
  checkpoint_candidate="$(mktemp "$STATE/.prepared-receipt.XXXXXX")"
  jq \
    --argjson completed "$(date +%s)" --argjson duration "$prepare_duration" \
    '.status="complete" | .completedAtEpoch=$completed | .metrics={prepareDurationSeconds:$duration}' \
    "$PREPARE_CHECKPOINT" >"$checkpoint_candidate"
  chmod 600 "$checkpoint_candidate"
  mv "$checkpoint_candidate" "$PREPARED_RECEIPT"
  prepared_receipt_valid "$PREPARED_RECEIPT" complete || fail prepared-receipt
  rm -f -- "$PREPARE_CHECKPOINT"
  emit_prepared_status
  release_prepare_lock
  trap - EXIT INT TERM
}

run_prepared_status() {
  prepared_receipt_valid "$PREPARED_RECEIPT" complete || fail artifacts-not-prepared
  emit_prepared_status
}

run_up() {
  [ "$(count_lines "$(project_ids)")" -eq 0 ] || fail occupied-project
  [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
  [ ! -e "$STARTING_RECEIPT" ] && [ ! -L "$STARTING_RECEIPT" ] || fail stale-starting-receipt
  shared_midnight_ready || fail shared-midnight
  : >"$PRIVATE_LOG"
  chmod 600 "$PRIVATE_LOG"
  local resolver_image did_manager_image issuer_image wallet_seed env_candidate receipt_candidate mock_state
  if [ -n "$EXTERNAL_PREPARED_RECEIPT" ]; then
    docker image inspect "$SMOCKER_IMAGE" >/dev/null 2>&1 || fail artifacts-not-prepared
  else
    docker pull "$SMOCKER_IMAGE" >>"$PRIVATE_LOG" 2>&1 || fail smocker
  fi
  if [ -n "$EXTERNAL_PREPARED_RECEIPT" ]; then
    [[ "$EXTERNAL_PREPARED_RECEIPT" = /* ]] || fail prepared-receipt
    prepared_receipt_valid "$EXTERNAL_PREPARED_RECEIPT" complete || fail artifacts-not-prepared
    resolver_image="$(jq -r '.images.resolver.id' "$EXTERNAL_PREPARED_RECEIPT")"
    did_manager_image="$(jq -r '.images.didManager.id' "$EXTERNAL_PREPARED_RECEIPT")"
    issuer_image="$(jq -r '.images.issuer.id' "$EXTERNAL_PREPARED_RECEIPT")"
  else
    build_image midnight-did-resolver-image resolver_image || fail resolver-image
    build_image did-manager-image did_manager_image || fail did-manager-image
    build_image issuer-image issuer_image || fail issuer-image
  fi
  wallet_seed="$(awk '$1 == "WALLET_SEED:" { gsub(/[\" ]/, "", $2); print $2 }' "$SOURCE/docker/docker-compose.yml")"
  [[ "$wallet_seed" =~ ^[0-9a-f]{64}$ ]] || fail wallet-input
  [[ "${PORTAL_ISSUER_URL:-}" =~ ^https?:// ]] || fail issuer-origin
  [[ "${PORTAL_HOLDER_RESOLVER_URL:-}" =~ ^http://host\.docker\.internal:[0-9]+$ ]] || fail holder-resolver
  tailnet_mock_state_valid || fail tailnet-mock
  mock_state="${TAILNET_MOCK_FILE:-$SOURCE/mock/didit.yml}"
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
  receipt_candidate="$(mktemp "$STATE/.starting-receipt.XXXXXX")"
  jq -cn \
    --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
    --arg compose "$(shasum -a 256 "$COMPOSE_FILE" | awk '{print $1}')" \
    --argjson started "$(date +%s)" \
    '{schema:"oxid-portal-consumer-starting-v1",source:{commit:$commit,tree:$tree},composeSha256:$compose,project:"oxid-portal-consumer",startedAtEpoch:$started}' \
    >"$receipt_candidate"
  chmod 600 "$receipt_candidate"
  mv "$receipt_candidate" "$STARTING_RECEIPT"
  starting_receipt_valid || fail starting-receipt
  up_cleanup_running=0
  cleanup_failed_up() {
    if [ "$up_cleanup_running" -eq 1 ]; then return; fi
    up_cleanup_running=1
    starting_receipt_valid || return 1
    compose down --volumes --remove-orphans --timeout 30 >>"$PRIVATE_LOG" 2>&1 || true
    [ -z "$(project_ids)" ] || return 1
    rm -f -- "$ENV_FILE" "$RECEIPT" "$STARTING_RECEIPT" "$PRIVATE_LOG"
  }
  trap cleanup_failed_up ERR
  trap 'cleanup_failed_up; exit 130' INT
  trap 'cleanup_failed_up; exit 143' TERM
  compose up -d --wait --wait-timeout 600 >>"$PRIVATE_LOG" 2>&1
  curl --fail --silent --show-error --max-time 30 -H 'Content-Type: application/x-yaml' \
    --data-binary "@$mock_state" 'http://127.0.0.1:8081/mocks?reset=true' \
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
  rm -f -- "$STARTING_RECEIPT"
  trap - ERR INT TERM
  emit_status running
}

run_status() {
  local ids running
  ids="$(project_ids)"; running="$(running_ids)"
  if [ -z "$ids" ]; then
    [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
    [ ! -e "$STARTING_RECEIPT" ] && [ ! -L "$STARTING_RECEIPT" ] || fail interrupted-startup
    emit_status stopped
    return
  fi
  [ "$(count_lines "$ids")" -eq 5 ] && [ "$(count_lines "$running")" -eq 4 ] || fail project-shape
  receipt_valid || fail ownership
  emit_status running
}

run_services_status() {
  local ids running state
  ids="$(project_ids)"; running="$(running_ids)"
  [ "$(count_lines "$ids")" -eq 5 ] || fail project-shape
  receipt_valid || fail ownership
  case "$(count_lines "$running")" in
    4) state=running ;;
    0) state=stopped ;;
    *) fail project-shape ;;
  esac
  jq -cn --arg state "$state" \
    '{schema:"oxid-portal-consumer-services-status-v1",state:$state}'
}

run_services_up() {
  receipt_valid || fail ownership
  [ "$(count_lines "$(project_ids)")" -eq 5 ] || fail project-shape
  compose start smocker did-resolver did-manager issuer \
    >>"$PRIVATE_LOG" 2>&1 || fail services-start
  run_services_status
}

run_services_stop() {
  receipt_valid || fail ownership
  [ "$(count_lines "$(project_ids)")" -eq 5 ] || fail project-shape
  compose stop --timeout 30 smocker did-resolver did-manager issuer >>"$PRIVATE_LOG" 2>&1 || fail services-stop
  run_services_status
}

run_down() {
  local ids
  ids="$(project_ids)"
  if [ -z "$ids" ]; then
    [ ! -e "$RECEIPT" ] && [ ! -L "$RECEIPT" ] || fail stale-receipt
    if [ -e "$STARTING_RECEIPT" ] || [ -L "$STARTING_RECEIPT" ]; then
      starting_receipt_valid || fail ownership
    fi
    rm -f -- "$ENV_FILE" "$STARTING_RECEIPT" "$PRIVATE_LOG"
    emit_status stopped
    return
  fi
  receipt_valid || starting_receipt_valid || fail ownership
  [ -f "$ENV_FILE" ] && [ ! -L "$ENV_FILE" ] || fail private-state
  compose down --volumes --remove-orphans --timeout 30 >>"$PRIVATE_LOG" 2>&1 || fail cleanup
  [ -z "$(project_ids)" ] || fail cleanup-incomplete
  rm -f -- "$ENV_FILE" "$RECEIPT" "$STARTING_RECEIPT" "$PRIVATE_LOG"
  emit_status stopped
}

case "$OPERATION" in
  prerequisite) run_prerequisite ;;
  prepare) run_prepare ;;
  prepared-status) run_prepared_status ;;
  up) run_up ;;
  status) run_status ;;
  down) run_down ;;
  services-up) run_services_up ;;
  services-status) run_services_status ;;
  services-stop) run_services_stop ;;
esac
