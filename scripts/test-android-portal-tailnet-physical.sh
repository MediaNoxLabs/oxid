#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly STATE="$REPOSITORY_ROOT/target/portal-android-physical/runtime"
readonly SOURCE="$STATE/portal-source"
readonly PRIVATE_LOG="$STATE/private.log"
readonly READY_FIFO="$STATE/ready.fifo"
readonly CAPABILITY_FIFO="$STATE/capability.fifo"
readonly XDG_CONFIG="$STATE/xdg-config"
readonly XDG_STATE="$STATE/xdg-state"
readonly CONTROL_ORIGIN="http://127.0.0.1:18091"
readonly CONTROL_CONFIG="$STATE/control-curl.conf"
readonly ORIGIN_POLICY="$REPOSITORY_ROOT/scripts/e2e/tailnet-origin-policy.mjs"
readonly TRIGGER="openid-credential-offer://standalone-portal-test-fetch"
readonly SOURCE_INPUT="${OXID_PORTAL_SOURCE_REPOSITORY:-$PORTAL_REMOTE}"

support_pid=""
profile_active=0
forward_port=""
websocket_url=""
cleanup_running=0

fail() {
  printf 'android-portal-tailnet: FAIL phase=%s\n' "$1" >&2
  exit 1
}

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = Darwin ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
readonly adb="$android_sdk/platform-tools/adb"

control_curl() {
  curl --config "$CONTROL_CONFIG" --noproxy '*' --fail --silent --show-error --max-time 30 "$@"
}

cleanup() {
  local incoming=$? cleanup_status=0
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM
  if [ -n "$forward_port" ]; then
    adb_device forward --remove "tcp:$forward_port" >/dev/null 2>&1 || cleanup_status=1
    forward_port=""
  fi
  adb_device shell run-as io.medianox.oxid sh -c \
    'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp' \
    >/dev/null 2>&1 || true
  if [ "$profile_active" -eq 1 ]; then
    XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
      "$SOURCE/scripts/tailscale-https-profile.sh" cleanup \
      >>"$PRIVATE_LOG" 2>&1 || cleanup_status=1
    profile_active=0
  fi
  if [ -n "$support_pid" ]; then
    if [ -f "$CONTROL_CONFIG" ]; then
      control_curl --max-time 10 -X POST "$CONTROL_ORIGIN/complete" \
        >/dev/null 2>&1 || kill -TERM "$support_pid" >/dev/null 2>&1 || true
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
    wait "$support_pid" >/dev/null 2>&1 || cleanup_status=1
    support_pid=""
  fi
  if [ "$cleanup_status" -eq 0 ]; then
    rm -rf -- "$STATE"
  else
    incoming=1
    printf 'android-portal-tailnet: cleanup could not prove exact restoration\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command_name in cargo curl docker git jq nix node openssl rg shasum tailscale; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
[ -x "$adb" ] || fail adb
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
readonly OXID_HEAD="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)"
[[ "$OXID_HEAD" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head

physical_devices="$($adb devices | awk 'NR > 1 && $2 == "device" && $1 !~ /^emulator-/ { print $1 }')"
[ "$(awk 'NF { count++ } END { print count + 0 }' <<<"$physical_devices")" -eq 1 ] || fail physical-device
device="$physical_devices"
adb_device() { ANDROID_SERIAL="$device" "$adb" "$@"; }
[ "$(adb_device shell getprop ro.kernel.qemu | tr -d '\r\n')" = 0 ] || fail physical-device
[ "$(adb_device get-state 2>/dev/null)" = device ] || fail physical-device
if $adb devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
  fail emulator-running
fi

status_json="$(tailscale status --json)"
[ "$(jq -r '.BackendState' <<<"$status_json")" = Running ] || fail tailscale
dns_name="$(jq -r '.Self.DNSName | rtrimstr(".")' <<<"$status_json")"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$dns_name" node "$ORIGIN_POLICY" --host-env \
  || fail tailscale-identity
baseline="$(tailscale serve status --json | jq -S -c '.')"
printf '%s' "$baseline" | jq -e '
  .TCP["443"].HTTPS == true
  and .TCP["8443"].HTTPS == true
  and .TCP["10000"].HTTPS == true
' >/dev/null || fail preserved-routes
listener=""
for candidate in $(seq 11000 11999); do
  key="$dns_name:$candidate"
  if jq -e --arg port "$candidate" --arg key "$key" \
    '(.TCP[$port] == null) and (.Web[$key] == null)' <<<"$baseline" >/dev/null; then
    listener="$candidate"
    break
  fi
done
[ -n "$listener" ] || fail listener
public_origin="https://$dns_name:$listener"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$public_origin" node "$ORIGIN_POLICY" --origin-env \
  || fail listener

umask 077
rm -rf -- "$STATE"
mkdir -p "$STATE" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE"
chmod 700 "$STATE" "$XDG_CONFIG" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE"
: >"$PRIVATE_LOG"
chmod 600 "$PRIVATE_LOG"
printf '%s' "$baseline" >"$STATE/tailscale-baseline.json"
chmod 600 "$STATE/tailscale-baseline.json"

if ! git clone --no-checkout "$SOURCE_INPUT" "$SOURCE" >>"$PRIVATE_LOG" 2>&1; then fail source-clone; fi
git -C "$SOURCE" remote set-url origin "$PORTAL_REMOTE"
git -C "$SOURCE" fetch origin integration >>"$PRIVATE_LOG" 2>&1 || fail source-fetch
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{commit})" = "$PORTAL_COMMIT" ] || fail source-commit
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{tree})" = "$PORTAL_TREE" ] || fail source-tree
git -C "$SOURCE" checkout --detach "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all)" ] || fail source-dirty
[ -x "$SOURCE/scripts/tailscale-https-profile.sh" ] || fail profile-source

mkfifo "$READY_FIFO" "$CAPABILITY_FIFO"
chmod 600 "$READY_FIFO" "$CAPABILITY_FIFO"
exec 8<>"$CAPABILITY_FIFO"
exec 9<>"$READY_FIFO"
PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_MOBILE_STATE_DIR="$STATE" \
OXID_PORTAL_MOBILE_READY_FIFO="$READY_FIFO" \
OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$CAPABILITY_FIFO" \
PORTAL_CONSUMER_LIFECYCLE="$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" \
OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$public_origin" \
  node "$REPOSITORY_ROOT/scripts/e2e/portal-android-support.mjs" \
    >>"$PRIVATE_LOG" 2>&1 &
support_pid=$!
if ! IFS= read -r -t 900 -u 9 ready_status; then fail support-timeout; fi
exec 9>&-
rm -f -- "$READY_FIFO"
[ "$ready_status" = READY ] || fail "${ready_status#FAIL:}"
kill -0 "$support_pid" 2>/dev/null || fail support

ready="$STATE/ready.json"
manifest_path="$(jq -r '.manifestPath // empty' "$ready")"
manifest_sha="$(jq -r '.manifestSha256 // empty' "$ready")"
control_capability="$(jq -r '.controlCapability // empty' "$ready")"
[ "$(jq -r '.schema // empty' "$ready")" = oxid-portal-android-ready-v2 ] \
  && [ "$(jq -r '.offerPort // empty' "$ready")" = 18094 ] \
  && [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] \
  && [[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] || fail manifest
printf 'header = "Authorization: Bearer %s"\n' "$control_capability" >"$CONTROL_CONFIG"
chmod 600 "$CONTROL_CONFIG"
control_capability=""
[ -f "$manifest_path" ] && [ ! -L "$manifest_path" ] || fail manifest
[ "$(shasum -a 256 "$manifest_path" | awk '{print $1}')" = "$manifest_sha" ] || fail manifest

jq -cn --arg dns "$dns_name" --argjson port "$listener" '
  {PORTAL_TAILSCALE_DNS_NAME:$dns,routes:[
    {path:"/",httpsPort:$port,upstream:"http://127.0.0.1:18090"},
    {path:"/issuer-resolver",httpsPort:$port,upstream:"http://127.0.0.1:18093"},
    {path:"/offer",httpsPort:$port,upstream:"http://127.0.0.1:18094"}
  ]}' >"$XDG_CONFIG/lace-id-portal/tailscale-https.json"
chmod 600 "$XDG_CONFIG/lace-id-portal/tailscale-https.json"
XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
  "$SOURCE/scripts/tailscale-https-profile.sh" setup >>"$PRIVATE_LOG" 2>&1 || fail profile-setup
profile_active=1

adb_reverse_before="$(adb_device reverse --list 2>/dev/null | sort)"
adb_device shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
if ! OXID_MOBILE_CUSTODY=development \
  OXID_MOBILE_PORTAL_PROFILE=tailnet-android \
  OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$public_origin" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$manifest_sha" \
    "$REPOSITORY_ROOT/scripts/run-android-tailnet.sh" >>"$PRIVATE_LOG" 2>&1; then
  build_diagnostic="$(rg '^(error(\[[A-Z0-9]+\])?:|error: could not compile|Caused by:)' "$PRIVATE_LOG" | tail -n 20 || true)"
  if [ -n "$build_diagnostic" ]; then
    sed -E 's#https?://[^[:space:]]+#<redacted-url>#g; s/[0-9a-f]{64}/<redacted-digest>/g' \
      <<<"$build_diagnostic" >&2
  fi
  build_diagnostic=""
  tail -n 80 "$PRIVATE_LOG" | sed -E \
    -e 's#https?://[^[:space:]]+#<redacted-url>#g' \
    -e 's/[[:alnum:]_-]+(\.[[:alnum:]_-]+)+\.ts\.net/<redacted-tailnet>/g' \
    -e 's/did:[^[:space:]"'"'"']+/<redacted-did>/g' \
    -e 's/[0-9a-f]{64}/<redacted-digest>/g' \
    -e 's/(Bearer )[0-9a-f]+/\1<redacted>/g' >&2
  fail android-build
fi

open_webview() {
  local pid socket_list pages
  for _attempt in $(seq 1 120); do
    pid="$(adb_device shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r\n' || true)"
    socket_list="$(adb_device shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$pid" ] && rg -q "@webview_devtools_remote_${pid}$" <<<"$socket_list"; then break; fi
    sleep 0.25
  done
  [ -n "$pid" ] && rg -q "@webview_devtools_remote_${pid}$" <<<"$socket_list" || return 1
  forward_port="$(adb_device forward tcp:0 "localabstract:webview_devtools_remote_$pid" | tr -d '\r\n')"
  [[ "$forward_port" =~ ^[0-9]+$ ]] || return 1
  websocket_url=""
  for _attempt in $(seq 1 120); do
    pages="$(curl --noproxy '*' --silent --fail --max-time 2 "http://127.0.0.1:$forward_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' <<<"$pages")"
    [ -n "$websocket_url" ] && break
    sleep 0.25
  done
  [ -n "$websocket_url" ]
}

run_scenario() {
  local mode="$1" scenario_log="$STATE/scenario-error.log"
  open_webview || fail webview
  rm -f -- "$scenario_log"
  control_capability="$(jq -r '.controlCapability' "$ready")"
  if ! OXID_PORTAL_CONTROL_ORIGIN="$CONTROL_ORIGIN" \
    OXID_PORTAL_CONTROL_CAPABILITY="$control_capability" \
    node "$REPOSITORY_ROOT/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" \
    >>"$PRIVATE_LOG" 2>"$scenario_log"; then
    if ! rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$scenario_log"; then
      tail -n 12 "$scenario_log" >&2
    fi
    fail "$mode"
  fi
  control_capability=""
  rm -f -- "$scenario_log"
  adb_device forward --remove "tcp:$forward_port" >/dev/null 2>&1 || fail forward-cleanup
  forward_port=""
}

arm_and_route_offer() {
  local launch_mode="${1:-warm}"
  printf 'normal' | control_curl -X POST --data-binary @- "$CONTROL_ORIGIN/proxy-mode" \
    >/dev/null || fail proxy-mode
  control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null || fail offer-arm
  capability_stage_command="run-as io.medianox.oxid sh -c 'umask 077; target=files/portal-offer.capability; candidate=files/.portal-offer.capability.tmp; rm -f \"\$candidate\" \"\$target\"; cat >\"\$candidate\"; test \"\$(wc -c <\"\$candidate\")\" -eq 64; mv \"\$candidate\" \"\$target\"'"
  head -c 64 <&8 | adb_device shell "$capability_stage_command" \
    >/dev/null 2>>"$PRIVATE_LOG" || fail capability-stage
  if [ "$launch_mode" = cold ]; then
    adb_device shell am force-stop io.medianox.oxid >/dev/null || fail cold-stop
  fi
  adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" io.medianox.oxid \
    >/dev/null 2>>"$PRIVATE_LOG" || fail ingress
}

SECONDS=0
arm_and_route_offer cold
run_scenario cold-route
run_scenario prepare-holder
adb_device exec-out run-as io.medianox.oxid cat files/oxid/private/did-records.json | \
  control_curl --max-time 10 \
    -H 'Content-Type: application/json' --data-binary @- "$CONTROL_ORIGIN/holder" \
    >/dev/null || fail holder-sync

for negative_mode in route-refuse malformed protocol-error protocol-timeout issue-error; do
  arm_and_route_offer warm
  run_scenario "$negative_mode"
done
arm_and_route_offer warm
run_scenario issue

credential_header="$(adb_device shell run-as io.medianox.oxid od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$(adb_device shell run-as io.medianox.oxid wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r\n')"
[ "$credential_header" = 4f58494456433031 ] && [ "$credential_key_size" = 32 ] || fail encrypted-store
old_pid="$(adb_device shell pidof io.medianox.oxid | tr -d '\r\n')"
adb_device shell am force-stop io.medianox.oxid
adb_device shell am start -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
for _attempt in $(seq 1 60); do
  new_pid="$(adb_device shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r\n' || true)"
  [ -n "$new_pid" ] && [ "$new_pid" != "$old_pid" ] && break
  sleep 0.25
done
[ -n "${new_pid:-}" ] && [ "$new_pid" != "$old_pid" ] || fail process-restart
run_scenario restored
[ "$SECONDS" -le 300 ] || fail duration

counters="$(control_curl --max-time 10 "$CONTROL_ORIGIN/counters")"
jq -e '
  .authorizationMetadata == 3
  and .credential == 1
  and .issuerMetadata == 6
  and .issuerResolution == 3
  and .issuerResolutionSuccess == 3
  and .kyc == 14
  and .nonce == 1
  and .other == 0
  and .token == 2
' >/dev/null <<<"$counters" || fail protocol-counts
[ "$(adb_device reverse --list 2>/dev/null | sort)" = "$adb_reverse_before" ] || fail adb-reverse

receipt="$STATE/portal-consumer/owner-receipt.json"
resolver_image="$(jq -r '.images.resolver' "$receipt")"
did_manager_image="$(jq -r '.images.didManager' "$receipt")"
issuer_image="$(jq -r '.images.issuer' "$receipt")"
for image in "$resolver_image" "$did_manager_image" "$issuer_image"; do
  [[ "$image" =~ ^sha256:[0-9a-f]{64}$ ]] || fail image-evidence
done

XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
  "$SOURCE/scripts/tailscale-https-profile.sh" cleanup >>"$PRIVATE_LOG" 2>&1 || fail profile-cleanup
profile_active=0
after_cleanup="$(tailscale serve status --json | jq -S -c '.')"
[ "$after_cleanup" = "$baseline" ] || fail serve-restoration
control_curl --max-time 10 -X POST "$CONTROL_ORIGIN/complete" >/dev/null || fail support-finish
for _attempt in $(seq 1 90); do
  kill -0 "$support_pid" 2>/dev/null || break
  sleep 1
done
kill -0 "$support_pid" 2>/dev/null && fail support-stop
wait "$support_pid" || fail support-stop
support_pid=""
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail portal-cleanup

os_version="$(adb_device shell getprop ro.build.version.release | tr -d '\r\n')"
api_level="$(adb_device shell getprop ro.build.version.sdk | tr -d '\r\n')"
[[ "$api_level" =~ ^[0-9]+$ ]] || fail platform-evidence
[ "$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)" = "$OXID_HEAD" ] || fail head-changed
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty

evidence="$REPOSITORY_ROOT/target/portal-android-physical/evidence.json"
mkdir -p "$(dirname -- "$evidence")"
candidate="$(mktemp "$(dirname -- "$evidence")/.evidence.XXXXXX")"
jq -cn \
  --arg head "$OXID_HEAD" --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  --arg resolver "$resolver_image" --arg didManager "$did_manager_image" --arg issuer "$issuer_image" \
  --arg os "$os_version" --arg api "$api_level" --argjson duration "$SECONDS" \
  '{schema:"oxid-portal-android-evidence-v1",oxid:{head:$head},portal:{integrationCommit:$commit,integrationTree:$tree,images:{resolver:$resolver,didManager:$didManager,issuer:$issuer}},platform:{kind:"android_physical_tailnet",os:$os,apiLevel:$api,applicationId:"io.medianox.oxid"},acceptance:{mockKycApproved:true,warmIngress:true,coldIngress:true,refusalBeforeConsent:true,refusalSecretEndpointCalls:0,malformedRejected:true,unavailableRejected:true,timeoutRejected:true,issueErrorEscapedSafely:true,exactProtocolCounters:true,strictFinalExchange:true,explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,exactBundleImported:true,encryptedPersistence:true,processRestart:true,custodyReactivated:true,listedAfterRestart:true,freshReverification:true,oneItemIngress:true,noAdbReverse:true,tailnetIdentityDiscovered:true,temporaryListenerDiscovered:true,preservedStandaloneRoutes:true,exactServeReceiptCleanup:true,secretFreeEvidence:true,completedWithin300Seconds:($duration <= 300)}}' \
  >"$candidate"
if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$candidate"; then
  rm -f -- "$candidate"
  fail evidence-schema
fi
chmod 600 "$candidate"
mv -f -- "$candidate" "$evidence"
printf 'android-portal-tailnet: PASS head=%s evidence=%s\n' "$OXID_HEAD" "${evidence#"$REPOSITORY_ROOT/"}"
