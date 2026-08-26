#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly STATE="$REPOSITORY_ROOT/target/portal-virtual-mobile/runtime"
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

support_pid=""
cleanup_running=0

fail() {
  printf 'portal-virtual-mobile-stack: FAIL phase=%s\n' "$1" >&2
  exit 1
}

control_curl() {
  curl --config "$CONTROL_CONFIG" --noproxy '*' --fail --silent --show-error --max-time 30 "$@"
}

cleanup() {
  local incoming=$? cleanup_status=0
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM
  if [ -n "$support_pid" ] && kill -0 "$support_pid" 2>/dev/null; then
    if [ -f "$CONTROL_CONFIG" ]; then
      control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 \
        || kill -TERM "$support_pid" >/dev/null 2>&1 || true
    else
      kill -TERM "$support_pid" >/dev/null 2>&1 || true
    fi
    for _attempt in $(seq 1 90); do
      kill -0 "$support_pid" 2>/dev/null || break
      sleep 1
    done
    if kill -0 "$support_pid" 2>/dev/null; then
      kill -KILL "$support_pid" >/dev/null 2>&1 || true
      cleanup_status=1
    fi
  fi
  if [ -n "$support_pid" ]; then
    wait "$support_pid" >/dev/null 2>&1 || cleanup_status=1
    support_pid=""
  fi
  exec 8>&- 9>&- 2>/dev/null || true
  [ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" ] \
    || cleanup_status=1
  if [ "$cleanup_status" -eq 0 ]; then
    rm -rf -- "$STATE"
  else
    incoming=1
    printf 'portal-virtual-mobile-stack: cleanup could not prove exact restoration\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

case "$OPERATION" in serve|--contract-test) ;; *) fail usage ;; esac
for command_name in curl docker git jq node shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
docker info >/dev/null 2>&1 || fail docker
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] \
  || fail occupied-project

umask 077
rm -rf -- "$STATE"
mkdir -p "$STATE"
chmod 700 "$STATE"
: >"$PRIVATE_LOG"
chmod 600 "$PRIVATE_LOG"

if ! git clone --no-checkout "$SOURCE_INPUT" "$SOURCE" >>"$PRIVATE_LOG" 2>&1; then
  fail source-clone
fi
git -C "$SOURCE" remote set-url origin "$PORTAL_REMOTE"
git -C "$SOURCE" fetch origin integration >>"$PRIVATE_LOG" 2>&1 || fail source-fetch
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{commit})" = "$PORTAL_COMMIT" ] || fail source-commit
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{tree})" = "$PORTAL_TREE" ] || fail source-tree
git -C "$SOURCE" checkout --detach "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all)" ] || fail source-dirty
provenance_path="crates/issuer-integration/fixtures/openid4vci-final/provenance.json"
[ "$(git -C "$SOURCE" show "$PORTAL_COMMIT:$provenance_path" | shasum -a 256 | awk '{print $1}')" = "$PORTAL_PROVENANCE_SHA256" ] \
  || fail source-provenance

PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_CONSUMER_STATE_DIR="$STATE/portal-consumer" \
  "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" prerequisite \
    >>"$PRIVATE_LOG" 2>&1 || fail shared-midnight-prerequisite

mkfifo "$READY_FIFO" "$CAPABILITY_FIFO"
chmod 600 "$READY_FIFO" "$CAPABILITY_FIFO"
exec 8<>"$CAPABILITY_FIFO"
exec 9<>"$READY_FIFO"
PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_MOBILE_STATE_DIR="$STATE" \
OXID_PORTAL_MOBILE_READY_FIFO="$READY_FIFO" \
OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$CAPABILITY_FIFO" \
OXID_PORTAL_MOBILE_SUPPORT_PROFILE=virtual-mobile \
PORTAL_CONSUMER_LIFECYCLE="$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" \
  node "$REPOSITORY_ROOT/scripts/e2e/portal-android-support.mjs" \
    >>"$PRIVATE_LOG" 2>&1 &
support_pid=$!
if ! IFS= read -r -t 900 -u 9 ready_status; then fail support-timeout; fi
exec 9>&-
rm -f -- "$READY_FIFO"
[ "$ready_status" = READY ] || fail "${ready_status#FAIL:}"
kill -0 "$support_pid" 2>/dev/null || fail support

manifest_path="$(jq -r '.manifestPath // empty' "$READY")"
manifest_sha="$(jq -r '.manifestSha256 // empty' "$READY")"
control_capability="$(jq -r '.controlCapability // empty' "$READY")"
[ "$(jq -r '.schema // empty' "$READY")" = oxid-portal-virtual-ready-v1 ] \
  && [ "$(jq -r '.controlOrigin // empty' "$READY")" = "$CONTROL_ORIGIN" ] \
  && [ "$(jq -r '.issuerProxyPort // empty' "$READY")" = 18090 ] \
  && [ "$(jq -r '.resolverProxyPort // empty' "$READY")" = 18093 ] \
  && [ "$(jq -r '.offerPort // empty' "$READY")" = 18091 ] \
  && [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] \
  && [[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] \
  || fail ready
[ -f "$manifest_path" ] && [ ! -L "$manifest_path" ] || fail manifest
[ "$(shasum -a 256 "$manifest_path" | awk '{print $1}')" = "$manifest_sha" ] || fail manifest
jq -e --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  '.schema == "oxid-portal-deployment-v3"
    and .integrationCommit == $commit
    and .integrationTree == $tree
    and .issuerOrigin == "http://127.0.0.1:18090"
    and .issuerResolverOrigin == "http://127.0.0.1:18093"' \
  "$manifest_path" >/dev/null || fail manifest

printf 'header = "Authorization: Bearer %s"\n' "$control_capability" >"$CONTROL_CONFIG"
chmod 600 "$CONTROL_CONFIG"
control_capability=""
control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null || fail offer-arm
if ! IFS= read -r -N 64 -t 30 -u 8 capability; then fail capability; fi
[[ "$capability" =~ ^[0-9a-f]{64}$ ]] || fail capability
printf '%s' "$capability" >"$CAPABILITY_FILE"
chmod 600 "$CAPABILITY_FILE"
printf 'header = "Authorization: Bearer %s"\n' "$capability" >"$OFFER_CONFIG"
chmod 600 "$OFFER_CONFIG"
capability=""
printf 'export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH=%q\n' "$manifest_path" >"$BUILD_ENV"
printf 'export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256=%q\n' "$manifest_sha" >>"$BUILD_ENV"
chmod 600 "$BUILD_ENV"

if [ "$OPERATION" = --contract-test ]; then
  [ "$(curl --noproxy '*' --silent --output /dev/null --write-out '%{http_code}' --max-time 10 \
    http://127.0.0.1:18091/offer)" = 401 ] || fail offer-authentication
  curl --noproxy '*' --fail --silent --show-error --max-time 30 \
    http://127.0.0.1:18090/.well-known/openid-credential-issuer >/dev/null \
    || fail issuer-endpoint
  issuer_did="$(jq -r '.issuerDid' "$manifest_path")"
  jq -cn --arg did "$issuer_did" '{did:$did}' | \
    curl --noproxy '*' --fail --silent --show-error --max-time 30 \
      -H 'Content-Type: application/json' --data-binary @- \
      http://127.0.0.1:18093/resolve | \
    jq -e --arg did "$issuer_did" '.didDocument.id == $did' >/dev/null \
    || fail resolver-endpoint
  offer_candidate="$STATE/offer-candidate"
  curl --config "$OFFER_CONFIG" --noproxy '*' --fail --silent --show-error --max-time 30 \
    http://127.0.0.1:18091/offer >"$offer_candidate" || fail offer-endpoint
  grep -q '^openid-credential-offer://' "$offer_candidate" || fail offer-shape
  rm -f -- "$offer_candidate" "$CAPABILITY_FILE" "$OFFER_CONFIG"
  control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null || fail support-complete
  wait "$support_pid" || fail support-stop
  support_pid=""
  [ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] \
    || fail portal-cleanup
  rm -rf -- "$STATE"
  printf 'portal-virtual-mobile-stack-contract: PASS endpoints=18090,18091,18093 manifest=digest-authenticated\n'
  exit 0
fi

printf 'portal-virtual-mobile-stack: READY endpoints=18090,18091,18093\n'
printf 'portal-virtual-mobile-stack: build_env=%s\n' "${BUILD_ENV#"$REPOSITORY_ROOT/"}"
printf 'portal-virtual-mobile-stack: capability_file=%s\n' "${CAPABILITY_FILE#"$REPOSITORY_ROOT/"}"
printf 'portal-virtual-mobile-stack: keep this command running; press Ctrl-C for exact cleanup\n'
while kill -0 "$support_pid" 2>/dev/null; do sleep 1; done
wait "$support_pid" || fail support
support_pid=""
fail unexpected-stop
