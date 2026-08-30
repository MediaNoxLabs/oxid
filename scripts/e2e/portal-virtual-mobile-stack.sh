#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly REPOSITORY_ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly PROCESS_SUPPORT="$REPOSITORY_ROOT/scripts/e2e/android-avd-process-ownership.sh"
readonly STACK_ROOT="$REPOSITORY_ROOT/target/portal-virtual-mobile"
readonly STACK_LOCK="$STACK_ROOT/stack.lock"
readonly STATE="$STACK_ROOT/runtime"
readonly SOURCE="$STATE/portal-source"
readonly READY_FIFO="$STATE/ready.fifo"
readonly CAPABILITY_FIFO="$STATE/capability.fifo"
readonly READY="$STATE/ready.json"
readonly PRIVATE_LOG="$STATE/private.log"
readonly CONTROL_CONFIG="$STATE/control-curl.conf"
readonly OFFER_CONFIG="$STATE/offer-curl.conf"
readonly CAPABILITY_FILE="$STATE/portal-offer.capability"
readonly BUILD_ENV="$STATE/build.env"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly SOURCE_INPUT="${OXID_PORTAL_SOURCE_REPOSITORY:-$PORTAL_REMOTE}"
readonly OPERATION="${1:-serve}"
readonly DOCKER_QUERY_TIMEOUT_SECONDS="${OXID_STACK_DOCKER_QUERY_TIMEOUT_SECONDS:-15}"

# shellcheck source=android-avd-process-ownership.sh
source "$PROCESS_SUPPORT"

support_pid=""
lock_receipt=""
lock_identity=""
lock_receipt_identity=""
state_identity=""
lock_owned=0
state_owned=0
restoration_proven=0
cleanup_running=0

fail() {
  printf 'portal-virtual-mobile-stack: FAIL phase=%s\n' "$1" >&2
  exit 1
}

run_deadline() {
  local seconds="$1"
  shift
  timeout -k 5s "${seconds}s" "$@"
}

control_curl() {
  run_deadline 35 curl --config "$CONTROL_CONFIG" --noproxy '*' \
    --fail --silent --show-error --max-time 30 "$@"
}

docker_project_ids() {
  run_deadline "$DOCKER_QUERY_TIMEOUT_SECONDS" docker ps -a \
    --filter label=com.docker.compose.project=oxid-portal-consumer --quiet
}

cleanup() {
  local incoming=$? cleanup_status=0 project_ids="" query_status=0
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM HUP
  set +e

  if [ -n "$support_pid" ]; then
    if oxid_job_is_running "$support_pid"; then
      if [ -f "$CONTROL_CONFIG" ]; then
        control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
      fi
      # The support process owns receipt-scoped Portal Compose cleanup. Give it
      # a bounded opportunity to remove that project before terminating its
      # process group; killing immediately after the accepted control response
      # can strand containers after the receipt has already been removed.
      for _cleanup_attempt in {1..120}; do
        oxid_job_is_running "$support_pid" || break
        sleep 1
      done
      if oxid_job_is_running "$support_pid"; then
        oxid_terminate_supervised_job "$support_pid" || cleanup_status=1
      else
        wait "$support_pid" >/dev/null 2>&1 || true
      fi
    else
      wait "$support_pid" >/dev/null 2>&1 || true
    fi
    support_pid=""
  fi
  exec 8>&- 9>&- || true

  if [ "$restoration_proven" -eq 0 ] \
    && { [ "$state_owned" -eq 1 ] || [ "$lock_owned" -eq 1 ]; }; then
    project_ids="$(docker_project_ids 2>/dev/null)"
    query_status=$?
    if [ "$query_status" -ne 0 ]; then
      cleanup_status=1
      printf 'portal-virtual-mobile-stack: cleanup Docker query failed; preserving owned state and lock for owner recovery\n' >&2
    elif [ -n "$project_ids" ]; then
      cleanup_status=1
      printf 'portal-virtual-mobile-stack: cleanup found project containers; preserving owned state and lock for owner recovery\n' >&2
    else
      if [ "$state_owned" -eq 1 ]; then
        if oxid_path_has_identity "$STATE" "$state_identity"; then
          run_deadline 30 rm -rf -- "$STATE" || cleanup_status=1
        else
          cleanup_status=1
        fi
        if [ ! -e "$STATE" ] && [ ! -L "$STATE" ]; then
          state_owned=0
        else
          cleanup_status=1
        fi
      fi
      [ "$state_owned" -eq 0 ] && [ "$cleanup_status" -eq 0 ] && restoration_proven=1
    fi
  fi

  if [ "$lock_owned" -eq 1 ] && [ "$restoration_proven" -eq 1 ]; then
    if [ -n "$lock_receipt" ] \
      && oxid_path_has_identity "$STACK_LOCK" "$lock_identity" \
      && oxid_path_has_identity "$STACK_LOCK/$lock_receipt" "$lock_receipt_identity" \
      && run_deadline 5 rmdir -- "$STACK_LOCK/$lock_receipt" >/dev/null 2>&1 \
      && [ ! -e "$STACK_LOCK/$lock_receipt" ] && [ ! -L "$STACK_LOCK/$lock_receipt" ] \
      && oxid_path_has_identity "$STACK_LOCK" "$lock_identity" \
      && run_deadline 5 rmdir -- "$STACK_LOCK" >/dev/null 2>&1; then
      lock_owned=0
    else
      cleanup_status=1
      printf 'portal-virtual-mobile-stack: owned lock proof/removal failed; owner review is required before stale-lock recovery\n' >&2
    fi
  fi

  if [ "$cleanup_status" -ne 0 ]; then
    incoming=1
    printf 'portal-virtual-mobile-stack: cleanup could not prove owned-state restoration\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

case "$OPERATION" in serve|--contract-test) ;; *) fail usage ;; esac
for command_name in curl docker git grep jq node shasum stat timeout; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
if timeout -k 1s 0.1s sleep 5; then fail timeout-capability; else [ "$?" -eq 124 ] || fail timeout-capability; fi
run_deadline 15 docker info >/dev/null 2>&1 || fail docker
[ -z "$(run_deadline 10 git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
if ! project_ids="$(docker_project_ids)"; then fail docker-query; fi
[ -z "$project_ids" ] || fail occupied-project
[ ! -e "$STATE" ] && [ ! -L "$STATE" ] || fail occupied-state

umask 077
run_deadline 5 mkdir -p -- "$STACK_ROOT" || fail stack-root
if ! run_deadline 5 mkdir -- "$STACK_LOCK" 2>/dev/null; then
  printf 'portal-virtual-mobile-stack: existing lock requires owner-reviewed stale-lock recovery; automatic deletion is disabled\n' >&2
  fail occupied-lock
fi
lock_owned=1
lock_identity="$(oxid_filesystem_identity "$STACK_LOCK")" || fail lock-identity
lock_receipt="receipt-$$-$RANDOM-$RANDOM-$RANDOM"
run_deadline 5 mkdir -- "$STACK_LOCK/$lock_receipt" || fail lock-receipt
lock_receipt_identity="$(oxid_filesystem_identity "$STACK_LOCK/$lock_receipt")" || fail lock-receipt-identity
run_deadline 5 chmod 700 "$STACK_LOCK" "$STACK_LOCK/$lock_receipt" || fail lock-mode
run_deadline 5 mkdir -- "$STATE" || fail state-create
state_owned=1
state_identity="$(oxid_filesystem_identity "$STATE")" || fail state-identity
run_deadline 5 chmod 700 "$STATE" || fail state-mode
: >"$PRIVATE_LOG"
run_deadline 5 chmod 600 "$PRIVATE_LOG" || fail log-mode

run_deadline 900 git clone --no-checkout "$SOURCE_INPUT" "$SOURCE" >>"$PRIVATE_LOG" 2>&1 \
  || fail source-clone
run_deadline 10 git -C "$SOURCE" remote set-url origin "$PORTAL_REMOTE" || fail source-remote
run_deadline 900 git -C "$SOURCE" fetch origin integration >>"$PRIVATE_LOG" 2>&1 || fail source-fetch
[ "$(run_deadline 10 git -C "$SOURCE" rev-parse FETCH_HEAD^{commit})" = "$PORTAL_COMMIT" ] || fail source-commit
[ "$(run_deadline 10 git -C "$SOURCE" rev-parse FETCH_HEAD^{tree})" = "$PORTAL_TREE" ] || fail source-tree
run_deadline 60 git -C "$SOURCE" checkout --detach "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1 || fail source-checkout
[ -z "$(run_deadline 10 git -C "$SOURCE" status --porcelain --untracked-files=all)" ] || fail source-dirty
provenance_path="crates/issuer-integration/fixtures/openid4vci-final/provenance.json"
provenance_digest="$(
  run_deadline 10 git -C "$SOURCE" show "$PORTAL_COMMIT:$provenance_path" |
    run_deadline 10 shasum -a 256
)"
provenance_digest="${provenance_digest%% *}"
[ "$provenance_digest" = "$PORTAL_PROVENANCE_SHA256" ] || fail source-provenance

run_deadline 1800 env \
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$STATE/portal-consumer" \
  "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" prerequisite \
  >>"$PRIVATE_LOG" 2>&1 || fail shared-midnight-prerequisite

run_deadline 5 mkfifo "$READY_FIFO" "$CAPABILITY_FIFO" || fail fifo-create
run_deadline 5 chmod 600 "$READY_FIFO" "$CAPABILITY_FIFO" || fail fifo-mode
exec 8<>"$CAPABILITY_FIFO"
exec 9<>"$READY_FIFO"
timeout -k 30s 14400s env \
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_MOBILE_STATE_DIR="$STATE" \
  OXID_PORTAL_MOBILE_READY_FIFO="$READY_FIFO" \
  OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$CAPABILITY_FIFO" \
  OXID_PORTAL_MOBILE_SUPPORT_PROFILE=virtual-mobile \
  PORTAL_CONSUMER_LIFECYCLE="$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" \
  node "$REPOSITORY_ROOT/scripts/e2e/portal-android-support.mjs" \
  >>"$PRIVATE_LOG" 2>&1 &
support_pid=$!
oxid_job_is_running "$support_pid" || fail support-supervisor
if ! IFS= read -r -t 900 -u 9 ready_status; then fail support-timeout; fi
exec 9>&-
run_deadline 5 rm -f -- "$READY_FIFO" || fail ready-fifo-remove
[ "$ready_status" = READY ] || fail "${ready_status#FAIL:}"
oxid_job_is_running "$support_pid" || fail support

manifest_path="$(run_deadline 10 jq -r '.manifestPath // empty' "$READY")"
manifest_sha="$(run_deadline 10 jq -r '.manifestSha256 // empty' "$READY")"
control_capability="$(run_deadline 10 jq -r '.controlCapability // empty' "$READY")"
[ "$(run_deadline 10 jq -r '.schema // empty' "$READY")" = oxid-portal-virtual-ready-v1 ] \
  && [ "$(run_deadline 10 jq -r '.controlOrigin // empty' "$READY")" = "$CONTROL_ORIGIN" ] \
  && [ "$(run_deadline 10 jq -r '.issuerProxyPort // empty' "$READY")" = 18090 ] \
  && [ "$(run_deadline 10 jq -r '.resolverProxyPort // empty' "$READY")" = 18093 ] \
  && [ "$(run_deadline 10 jq -r '.offerPort // empty' "$READY")" = 18091 ] \
  && [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] \
  && [[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] \
  || fail ready
[ -f "$manifest_path" ] && [ ! -L "$manifest_path" ] || fail manifest
actual_manifest_sha="$(run_deadline 10 shasum -a 256 "$manifest_path")"
actual_manifest_sha="${actual_manifest_sha%% *}"
[ "$actual_manifest_sha" = "$manifest_sha" ] || fail manifest
run_deadline 10 jq -e --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  '.schema == "oxid-portal-deployment-v3"
    and .integrationCommit == $commit
    and .integrationTree == $tree
    and .issuerOrigin == "http://127.0.0.1:18090"
    and .issuerResolverOrigin == "http://127.0.0.1:18093"' \
  "$manifest_path" >/dev/null || fail manifest

printf 'header = "Authorization: Bearer %s"\n' "$control_capability" >"$CONTROL_CONFIG"
run_deadline 5 chmod 600 "$CONTROL_CONFIG" || fail control-mode
control_capability=""
control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null || fail offer-arm
if ! IFS= read -r -N 64 -t 30 -u 8 capability; then fail capability; fi
[[ "$capability" =~ ^[0-9a-f]{64}$ ]] || fail capability
printf '%s' "$capability" >"$CAPABILITY_FILE"
run_deadline 5 chmod 600 "$CAPABILITY_FILE" || fail capability-mode
printf 'header = "Authorization: Bearer %s"\n' "$capability" >"$OFFER_CONFIG"
run_deadline 5 chmod 600 "$OFFER_CONFIG" || fail offer-mode
capability=""
printf 'export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH=%q\n' "$manifest_path" >"$BUILD_ENV"
printf 'export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256=%q\n' "$manifest_sha" >>"$BUILD_ENV"
run_deadline 5 chmod 600 "$BUILD_ENV" || fail build-env-mode

if [ "$OPERATION" = --contract-test ]; then
  offer_status="$(run_deadline 15 curl --noproxy '*' --silent --output /dev/null \
    --write-out '%{http_code}' --max-time 10 http://127.0.0.1:18091/offer)"
  [ "$offer_status" = 401 ] || fail offer-authentication
  run_deadline 35 curl --noproxy '*' --fail --silent --show-error --max-time 30 \
    http://127.0.0.1:18090/.well-known/openid-credential-issuer >/dev/null \
    || fail issuer-endpoint
  issuer_did="$(run_deadline 10 jq -r '.issuerDid' "$manifest_path")"
  run_deadline 10 jq -cn --arg did "$issuer_did" '{did:$did}' | \
    run_deadline 35 curl --noproxy '*' --fail --silent --show-error --max-time 30 \
      -H 'Content-Type: application/json' --data-binary @- \
      http://127.0.0.1:18093/resolve | \
    run_deadline 10 jq -e --arg did "$issuer_did" '.didDocument.id == $did' >/dev/null \
    || fail resolver-endpoint
  offer_candidate="$STATE/offer-candidate"
  run_deadline 35 curl --config "$OFFER_CONFIG" --noproxy '*' --fail --silent \
    --show-error --max-time 30 http://127.0.0.1:18091/offer >"$offer_candidate" \
    || fail offer-endpoint
  run_deadline 5 grep -q '^openid-credential-offer://' "$offer_candidate" || fail offer-shape
  run_deadline 5 rm -f -- "$offer_candidate" "$CAPABILITY_FILE" "$OFFER_CONFIG" || fail offer-remove
  control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null || fail support-complete
  wait "$support_pid" || fail support-stop
  support_pid=""
  if ! project_ids="$(docker_project_ids)"; then fail portal-cleanup-query; fi
  [ -z "$project_ids" ] || fail portal-cleanup
  oxid_path_has_identity "$STATE" "$state_identity" || fail state-identity
  run_deadline 30 rm -rf -- "$STATE" || fail state-cleanup
  [ ! -e "$STATE" ] && [ ! -L "$STATE" ] || fail state-cleanup
  state_owned=0
  restoration_proven=1
  printf 'portal-virtual-mobile-stack-contract: PASS endpoints=18090,18091,18093 manifest=digest-authenticated\n'
  exit 0
fi

printf 'portal-virtual-mobile-stack: READY endpoints=18090,18091,18093\n'
printf 'portal-virtual-mobile-stack: build_env=%s\n' "${BUILD_ENV#"$REPOSITORY_ROOT/"}"
printf 'portal-virtual-mobile-stack: capability_file=%s\n' "${CAPABILITY_FILE#"$REPOSITORY_ROOT/"}"
printf 'portal-virtual-mobile-stack: keep this command running; press Ctrl-C for exact cleanup\n'
while oxid_job_is_running "$support_pid"; do run_deadline 2 sleep 1; done
wait "$support_pid" || fail support
support_pid=""
fail unexpected-stop
