#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="25499870f84d77173c46e4af3021311decfb840b"
readonly PORTAL_TREE="2d845d2293603dfd8adce5362c8a9941e6ba78a9"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly OPERATION="${1:-automated}"
case "$OPERATION" in automated|manual-prepare|manual-prepared-status|manual-doctor|manual-start|manual-status|manual-reset|manual-stop) ;; *)
  printf '%s\n' 'android-portal-tailnet: FAIL phase=usage' >&2
  exit 1
  ;;
esac
readonly AUTOMATED_STATE="$REPOSITORY_ROOT/target/portal-android-physical/runtime"
readonly MANUAL_STATE="$REPOSITORY_ROOT/target/portal-tailnet-manual/runtime"
readonly MANUAL_PREPARED_ROOT="$REPOSITORY_ROOT/target/portal-tailnet-manual/prepared"
readonly MANUAL_PREPARED_SOURCE="$MANUAL_PREPARED_ROOT/portal-source"
readonly MANUAL_PREPARED_CONSUMER_STATE="$MANUAL_PREPARED_ROOT/portal-consumer"
readonly MANUAL_PREPARED_RECEIPT="$MANUAL_PREPARED_CONSUMER_STATE/prepared-receipt.json"
if [ "$OPERATION" = automated ]; then
  readonly STATE="$AUTOMATED_STATE"
  readonly SOURCE="$STATE/portal-source"
else
  readonly STATE="$MANUAL_STATE"
  readonly SOURCE="$MANUAL_PREPARED_SOURCE"
fi
readonly PRIVATE_LOG="$STATE/private.log"
readonly READY_FIFO="$STATE/ready.fifo"
readonly CAPABILITY_FIFO="$STATE/capability.fifo"
readonly XDG_CONFIG="$STATE/xdg-config"
readonly XDG_STATE="$STATE/xdg-state"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly CONTROL_CONFIG="$STATE/control-curl.conf"
readonly MANUAL_RECEIPT="$STATE/manual-session-receipt.json"
readonly MANUAL_STOP_REQUEST="$STATE/manual-stop-request"
readonly MANUAL_PAGE_URL="$STATE/manual-public-page-url"
readonly MOCK_STATE="$STATE/mock-state"
readonly ORIGIN_POLICY="$REPOSITORY_ROOT/scripts/e2e/tailnet-origin-policy.mjs"
readonly MOCK_TRANSFORM="$REPOSITORY_ROOT/scripts/e2e/tailnet-mock-transform.mjs"
readonly MOCK_ROUTE="$REPOSITORY_ROOT/scripts/e2e/tailnet-mock-route.mjs"
readonly EVIDENCE_FILTER="$REPOSITORY_ROOT/scripts/e2e/portal-android-evidence.jq"
readonly TRIGGER="openid-credential-offer://standalone-portal-test-fetch"
readonly SOURCE_INPUT="${OXID_PORTAL_SOURCE_REPOSITORY:-$PORTAL_REMOTE}"

support_pid=""
profile_active=0
forward_port=""
websocket_url=""
cleanup_running=0
manual_public_origin=""
manual_mock_receipt_sha=""
manual_start_epoch=0
services_seconds=0
tailnet_seconds=0
android_seconds=0

fail() {
  printf 'android-portal-tailnet: FAIL phase=%s\n' "$1" >&2
  exit 1
}

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = Darwin ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
readonly adb="$android_sdk/platform-tools/adb"

file_mode() {
  if stat -c '%a' -- "$1" 2>/dev/null; then :; else stat -f '%Lp' -- "$1"; fi
}

private_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ] && [ "$(file_mode "$1")" = 600 ]
}

sha256_text() {
  printf '%s' "$1" | shasum -a 256 | awk '{print $1}'
}

process_command_sha256() {
  local process_id="$1" command_line
  command_line="$(ps -p "$process_id" -o command= 2>/dev/null)" || return 1
  [ -n "$command_line" ] || return 1
  sha256_text "$command_line"
}

manual_select_connected_physical_device() {
  local physical_devices
  physical_devices="$("$adb" devices | awk 'NR > 1 && $2 == "device" && $1 !~ /^emulator-/ { print $1 }')"
  [ "$(awk 'NF { count++ } END { print count + 0 }' <<<"$physical_devices")" -eq 1 ] || return 1
  device="$physical_devices"
  adb_device() { ANDROID_SERIAL="$device" "$adb" "$@"; }
  [ "$(adb_device shell getprop ro.kernel.qemu | tr -d '\r\n')" = 0 ] || return 1
  [ "$(adb_device get-state 2>/dev/null)" = device ] || return 1
  if "$adb" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
    return 1
  fi
}

manual_select_physical_device() {
  manual_select_connected_physical_device || return 1
  adb_device shell pm path io.medianox.oxid 2>/dev/null | grep -q '^package:'
}

manual_doctor_fail() {
  printf 'portal-tailnet-manual: NOT-READY phase=%s remediation=%s\n' "$1" "$2" >&2
  exit 1
}

manual_session_load() {
  local current_head current_tree
  [ -d "$STATE" ] && [ ! -L "$STATE" ] && [ "$(file_mode "$STATE")" = 700 ] || return 1
  private_regular_file "$MANUAL_RECEIPT" || return 1
  current_head="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD 2>/dev/null)" || return 1
  current_tree="$(git -C "$REPOSITORY_ROOT" rev-parse 'HEAD^{tree}' 2>/dev/null)" || return 1
  jq -e --arg head "$current_head" --arg tree "$current_tree" \
    --arg commit "$PORTAL_COMMIT" --arg portal_tree "$PORTAL_TREE" '
      .schema == "oxid-portal-tailnet-manual-session-v1"
      and .oxid == {head:$head,tree:$tree}
      and .portal == {commit:$commit,tree:$portal_tree}
      and (.support.pid | type == "number" and . > 1)
      and (.supervisor.pid | type == "number" and . > 1)
      and (.support.commandSha256 | test("^[0-9a-f]{64}$"))
      and (.supervisor.commandSha256 | test("^[0-9a-f]{64}$"))
      and (.serve.baselineSha256 | test("^[0-9a-f]{64}$"))
      and (.serve.activeSha256 | test("^[0-9a-f]{64}$"))
      and (.mock.transformReceiptSha256 | test("^[0-9a-f]{64}$"))
      and (.metrics.servicesSeconds | type == "number" and . >= 0)
      and (.metrics.tailnetSeconds | type == "number" and . >= 0)
      and (.metrics.androidSeconds | type == "number" and . >= 0)
      and (.metrics.readySeconds | type == "number" and . >= 0)
      and .mock.externalPath == "/kyc/mock-verification"
      and .mock.upstreamPath == "/mock-verification"
      and .page == {html:true,mockRoute:true,holderBootstrap:true}
      and .deviceDataMode == "preserved"
    ' "$MANUAL_RECEIPT" >/dev/null || return 1
  manual_support_pid="$(jq -r '.support.pid' "$MANUAL_RECEIPT")"
  manual_support_command_sha="$(jq -r '.support.commandSha256' "$MANUAL_RECEIPT")"
  manual_supervisor_pid="$(jq -r '.supervisor.pid' "$MANUAL_RECEIPT")"
  manual_supervisor_command_sha="$(jq -r '.supervisor.commandSha256' "$MANUAL_RECEIPT")"
  manual_baseline_sha="$(jq -r '.serve.baselineSha256' "$MANUAL_RECEIPT")"
  manual_active_serve_sha="$(jq -r '.serve.activeSha256' "$MANUAL_RECEIPT")"
  manual_mock_receipt_sha="$(jq -r '.mock.transformReceiptSha256' "$MANUAL_RECEIPT")"
}

manual_process_matches() {
  local process_id="$1" expected_sha="$2"
  kill -0 "$process_id" 2>/dev/null \
    && [ "$(process_command_sha256 "$process_id")" = "$expected_sha" ]
}

manual_page_url_valid() {
  local page_url
  private_regular_file "$MANUAL_PAGE_URL" || return 1
  page_url="$(<"$MANUAL_PAGE_URL")"
  manual_public_origin="${page_url%/issue/index.html}"
  [ "$manual_public_origin/issue/index.html" = "$page_url" ] || return 1
  OXID_TAILNET_ORIGIN_POLICY_INPUT="$manual_public_origin" node "$ORIGIN_POLICY" --origin-env
}

manual_mock_state_valid() {
  [ -d "$MOCK_STATE" ] && [ ! -L "$MOCK_STATE" ] && [ "$(file_mode "$MOCK_STATE")" = 700 ] || return 1
  private_regular_file "$MOCK_STATE/didit-tailnet.yml" || return 1
  private_regular_file "$MOCK_STATE/didit-tailnet-receipt.json" || return 1
  [ "$(shasum -a 256 "$MOCK_STATE/didit-tailnet-receipt.json" | awk '{print $1}')" = "$manual_mock_receipt_sha" ] || return 1
  node "$MOCK_TRANSFORM" --validate "$MOCK_STATE" "$manual_public_origin" >/dev/null
}

manual_serve_receipt_valid() {
  local active
  private_regular_file "$STATE/tailscale-baseline.json" || return 1
  [ "$(shasum -a 256 "$STATE/tailscale-baseline.json" | awk '{print $1}')" = "$manual_baseline_sha" ] || return 1
  active="$(tailscale serve status --json | jq -S -c '.')" || return 1
  [ "$(sha256_text "$active")" = "$manual_active_serve_sha" ]
}

manual_consumer_running() {
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$STATE/portal-consumer" \
    "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" status >/dev/null 2>&1
}

manual_consumer_stopped() {
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$STATE/portal-consumer" \
    "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" services-status \
      | jq -e '.state == "stopped"' >/dev/null 2>&1
}

manual_public_page_ready() {
  local page_content_type
  page_content_type="$(curl --noproxy '*' --fail --silent --show-error --max-time 30 \
    --output /dev/null --write-out '%{content_type}' "$manual_public_origin/issue/index.html")" || return 1
  [[ "$page_content_type" = text/html* ]]
}

manual_ready() {
  manual_session_load \
    && manual_page_url_valid \
    && manual_mock_state_valid \
    && manual_serve_receipt_valid \
    && manual_process_matches "$manual_support_pid" "$manual_support_command_sha" \
    && manual_process_matches "$manual_supervisor_pid" "$manual_supervisor_command_sha" \
    && manual_select_physical_device \
    && manual_consumer_running \
    && manual_public_page_ready
}

manual_status() {
  for command_name in git jq node ps shasum tailscale; do
    command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
  done
  [ -x "$adb" ] || fail adb
  manual_ready || fail manual-not-ready
  jq -r '"portal-tailnet-manual: READY services=\(.metrics.servicesSeconds)s tailnet=\(.metrics.tailnetSeconds)s android=\(.metrics.androidSeconds)s total=\(.metrics.readySeconds)s"' "$MANUAL_RECEIPT"
}

manual_reset() {
  command -v awk >/dev/null 2>&1 || fail missing-tool
  [ -x "$adb" ] || fail adb
  [ ! -e "$STATE" ] && [ ! -L "$STATE" ] || fail manual-session-active
  manual_select_physical_device || fail physical-device
  printf '%s\n' 'portal-tailnet-manual: RESET package=io.medianox.oxid scope=application-data'
  adb_device shell am force-stop io.medianox.oxid >/dev/null 2>&1 || fail manual-reset-stop
  adb_device shell pm clear io.medianox.oxid >/dev/null 2>&1 || fail manual-reset-data
  adb_device shell pm path io.medianox.oxid 2>/dev/null | grep -q '^package:' || fail manual-reset-package
  printf '%s\n' 'portal-tailnet-manual: RESET-COMPLETE package=io.medianox.oxid scope=application-data'
}

portal_source_valid() {
  [ -d "$SOURCE" ] && [ ! -L "$SOURCE" ] \
    && [ "$(git -C "$SOURCE" remote get-url origin 2>/dev/null)" = "$PORTAL_REMOTE" ] \
    && [ "$(git -C "$SOURCE" rev-parse HEAD 2>/dev/null)" = "$PORTAL_COMMIT" ] \
    && [ "$(git -C "$SOURCE" rev-parse 'HEAD^{tree}' 2>/dev/null)" = "$PORTAL_TREE" ] \
    && [ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all 2>/dev/null)" ]
}

manual_prepare() {
  local prepare_log="$MANUAL_PREPARED_ROOT/prepare.log"
  local source_candidate="$MANUAL_PREPARED_ROOT/.portal-source.pending"
  for command_name in docker git jq nix openssl shasum; do
    command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
  done
  umask 077
  mkdir -p "$MANUAL_PREPARED_ROOT"
  chmod 700 "$MANUAL_PREPARED_ROOT"
  if [ -e "$SOURCE" ] || [ -L "$SOURCE" ]; then
    portal_source_valid || fail prepared-source
  else
    rm -rf -- "$source_candidate"
    git clone --no-checkout "$SOURCE_INPUT" "$source_candidate" >>"$prepare_log" 2>&1 || fail source-clone
    git -C "$source_candidate" remote set-url origin "$PORTAL_REMOTE"
    git -C "$source_candidate" fetch origin "$PORTAL_COMMIT" >>"$prepare_log" 2>&1 || fail source-fetch
    [ "$(git -C "$source_candidate" rev-parse 'FETCH_HEAD^{commit}')" = "$PORTAL_COMMIT" ] || fail source-commit
    [ "$(git -C "$source_candidate" rev-parse 'FETCH_HEAD^{tree}')" = "$PORTAL_TREE" ] || fail source-tree
    git -C "$source_candidate" checkout --detach "$PORTAL_COMMIT" >>"$prepare_log" 2>&1 || fail source-checkout
    chmod 700 "$source_candidate"
    mv "$source_candidate" "$SOURCE"
    portal_source_valid || fail prepared-source
  fi
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$MANUAL_PREPARED_CONSUMER_STATE" \
    "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" prepare
}

manual_prepared_valid() {
  PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$MANUAL_PREPARED_CONSUMER_STATE" \
    "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" prepared-status >/dev/null
}

manual_prepared_status() {
  manual_prepared_valid || fail artifacts-not-prepared
}

manual_doctor() {
  local portal_containers status_json dns_name
  for command_name in awk curl docker git jq nix node ps shasum tailscale; do
    command -v "$command_name" >/dev/null 2>&1 \
      || manual_doctor_fail missing-tool "install-$command_name"
  done
  [ -x "$adb" ] || manual_doctor_fail adb "install-android-platform-tools"
  [ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] \
    || manual_doctor_fail oxid-dirty "commit-or-stash-tracked-changes"
  [ ! -e "$STATE" ] && [ ! -L "$STATE" ] \
    || manual_doctor_fail manual-session-active "just-portal-tailnet-manual-status-or-stop"
  manual_prepared_valid \
    || manual_doctor_fail artifacts-not-prepared "just-portal-tailnet-manual-prepare"
  docker info >/dev/null 2>&1 \
    || manual_doctor_fail docker "start-docker-desktop"
  portal_containers="$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" \
    || manual_doctor_fail docker "start-docker-desktop"
  [ -z "$portal_containers" ] \
    || manual_doctor_fail portal-session-conflict "review-and-cleanup-owned-portal-receipt"
  "$REPOSITORY_ROOT/scripts/standalone-status.sh" phone >/dev/null 2>&1 \
    || manual_doctor_fail standalone-tailnet "just-standalone-phone-up"
  status_json="$(tailscale status --json 2>/dev/null)" \
    || manual_doctor_fail tailscale "connect-tailscale"
  [ "$(jq -r '.BackendState' <<<"$status_json")" = Running ] \
    || manual_doctor_fail tailscale "connect-tailscale"
  dns_name="$(jq -r '.Self.DNSName | rtrimstr(".")' <<<"$status_json")"
  OXID_TAILNET_ORIGIN_POLICY_INPUT="$dns_name" node "$ORIGIN_POLICY" --host-env >/dev/null \
    || manual_doctor_fail tailscale-identity "enable-magicdns-and-https"
  manual_select_connected_physical_device \
    || manual_doctor_fail physical-device "connect-exactly-one-authorized-phone-and-close-emulators"
  printf '%s\n' 'portal-tailnet-manual: DOCTOR-READY artifacts=ready standalone=ready tailnet=ready device=ready app-data=preserved'
}

manual_cleanup() {
  local after_cleanup project_ids cleanup_status=0
  manual_session_load && manual_page_url_valid && manual_mock_state_valid && manual_serve_receipt_valid || return 1
  manual_select_physical_device || return 1
  adb_device shell \
    "run-as io.medianox.oxid sh -c 'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp files/portal-holder.capability files/.portal-holder.capability.tmp && test ! -e files/portal-offer.capability && test ! -e files/.portal-offer.capability.tmp && test ! -e files/portal-holder.capability && test ! -e files/.portal-holder.capability.tmp'" \
    >/dev/null 2>&1 || return 1
  XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
    "$SOURCE/scripts/tailscale-https-profile.sh" cleanup >>"$PRIVATE_LOG" 2>&1 || return 1
  after_cleanup="$(tailscale serve status --json | jq -S -c '.')" || return 1
  [ "$after_cleanup" = "$(<"$STATE/tailscale-baseline.json")" ] || return 1
  if manual_process_matches "$manual_support_pid" "$manual_support_command_sha"; then
    kill -TERM "$manual_support_pid" >/dev/null 2>&1 || return 1
  fi
  for _attempt in $(seq 1 120); do
    kill -0 "$manual_support_pid" 2>/dev/null || break
    sleep 1
  done
  kill -0 "$manual_support_pid" 2>/dev/null && return 1
  project_ids="$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" || return 1
  [ -z "$project_ids" ] || return 1
  [ -d "$STATE" ] && [ ! -L "$STATE" ] || return 1
  rm -rf -- "$STATE" || return 1
  [ ! -e "$STATE" ] && [ ! -L "$STATE" ]
}

manual_stop() {
  local candidate
  for command_name in docker git jq node ps shasum tailscale; do
    command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
  done
  [ -x "$adb" ] || fail adb
  manual_session_load \
    && manual_page_url_valid \
    && manual_mock_state_valid \
    && manual_serve_receipt_valid \
    && manual_process_matches "$manual_support_pid" "$manual_support_command_sha" \
    && manual_process_matches "$manual_supervisor_pid" "$manual_supervisor_command_sha" \
    && manual_select_physical_device \
    || fail manual-stop-receipt
  [ ! -e "$MANUAL_STOP_REQUEST" ] && [ ! -L "$MANUAL_STOP_REQUEST" ] || fail manual-stop-pending
  candidate="$(mktemp "$STATE/.manual-stop.XXXXXX")" || fail manual-stop-request
  printf '%s\n' stop >"$candidate"
  chmod 600 "$candidate"
  mv "$candidate" "$MANUAL_STOP_REQUEST"
  for _attempt in $(seq 1 180); do
    [ ! -e "$STATE" ] && [ ! -L "$STATE" ] && {
      printf '%s\n' 'portal-tailnet-manual: STOPPED'
      return
    }
    sleep 1
  done
  fail manual-stop-timeout
}

manual_supervise() {
  manual_ready \
    && [ "$manual_supervisor_pid" = "$$" ] \
    && [ "$(process_command_sha256 "$$")" = "$manual_supervisor_command_sha" ] \
    || return 1
  trap 'manual_cleanup; result=$?; trap - EXIT INT TERM HUP; exit "$result"' INT TERM HUP
  while :; do
    if [ -e "$MANUAL_STOP_REQUEST" ] || [ -L "$MANUAL_STOP_REQUEST" ]; then
      private_regular_file "$MANUAL_STOP_REQUEST" \
        && [ "$(<"$MANUAL_STOP_REQUEST")" = stop ] \
        && manual_cleanup \
        || return 1
      return
    fi
    manual_session_load \
      && manual_page_url_valid \
      && manual_mock_state_valid \
      && manual_serve_receipt_valid \
      || return 1
    if ! manual_process_matches "$manual_support_pid" "$manual_support_command_sha"; then
      manual_cleanup || return 1
      return
    fi
    if ! manual_process_matches "$manual_supervisor_pid" "$manual_supervisor_command_sha" \
      || ! manual_select_physical_device \
      || ! manual_public_page_ready; then
      manual_cleanup || return 1
      return 1
    fi
    if ! manual_consumer_running && ! manual_consumer_stopped; then
      manual_cleanup || return 1
      return 1
    fi
    sleep 2
  done
}

case "$OPERATION" in
  manual-prepare) manual_prepare; exit 0 ;;
  manual-prepared-status) manual_prepared_status; printf '%s\n' 'portal-tailnet-manual: PREPARED'; exit 0 ;;
  manual-doctor) manual_doctor; exit 0 ;;
  manual-start) manual_prepared_status; manual_start_epoch="$(date +%s)" ;;
  manual-status) manual_status; exit 0 ;;
  manual-reset) manual_reset; exit 0 ;;
  manual-stop) manual_stop; exit 0 ;;
esac

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
  if adb_device shell pm path io.medianox.oxid >/dev/null 2>&1 \
    && ! adb_device shell \
      "run-as io.medianox.oxid sh -c 'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp files/portal-holder.capability files/.portal-holder.capability.tmp && test ! -e files/portal-offer.capability && test ! -e files/.portal-offer.capability.tmp && test ! -e files/portal-holder.capability && test ! -e files/.portal-holder.capability.tmp'" \
      >/dev/null 2>&1; then
    cleanup_status=1
  fi
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
  exec 8>&- 2>/dev/null || true
  rm -f -- "$READY_FIFO" "$CAPABILITY_FIFO" "$STATE/ready.json" "$CONTROL_CONFIG"
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
redact_physical_failure() {
  sed -E \
    -e "s#${device}#<redacted-device>#g" \
    -e 's#(https?|wss?)://[^[:space:]]+#<redacted-url>#g' \
    -e 's/[[:alnum:]_-]+(\.[[:alnum:]_-]+)+\.ts\.net/<redacted-tailnet>/g' \
    -e 's/did:[^[:space:]"'"'"']+/<redacted-did>/g' \
    -e 's/[0-9a-fA-F]{64}/<redacted-digest>/g' \
    -e 's/(Bearer )[0-9a-fA-F]+/\1<redacted>/g'
}
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
tailnet_identity_discovered=true
baseline="$(tailscale serve status --json | jq -S -c '.')"
preserved_standalone_routes="$(printf '%s' "$baseline" | jq -r '
  .TCP["443"].HTTPS == true
  and .TCP["8443"].HTTPS == true
  and .TCP["10000"].HTTPS == true
')"
[ "$preserved_standalone_routes" = true ] || fail preserved-routes
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
temporary_listener_discovered=true
public_origin="https://$dns_name:$listener"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$public_origin" node "$ORIGIN_POLICY" --origin-env \
  || fail listener
if [ "$OPERATION" = manual-start ]; then
  mock_route_config="$(node "$MOCK_ROUTE" --config "$public_origin" "$listener")" || fail manual-mock-route
  jq -e --argjson port "$listener" '
    . == {route:{path:"/kyc",httpsPort:$port,upstream:"http://127.0.0.1:9090"},externalRequestPath:"/kyc/mock-verification",upstreamRequestPath:"/mock-verification"}
  ' <<<"$mock_route_config" >/dev/null || fail manual-mock-route
  mock_external_path="$(jq -r '.externalRequestPath' <<<"$mock_route_config")"
fi

umask 077
if [ "$OPERATION" = manual-start ]; then
  [ ! -e "$STATE" ] && [ ! -L "$STATE" ] || fail manual-session-exists
else
  rm -rf -- "$STATE"
fi
mkdir -p "$STATE" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE"
chmod 700 "$STATE" "$XDG_CONFIG" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE"
if [ "$OPERATION" = manual-start ]; then
  mkdir -p "$MOCK_STATE"
  chmod 700 "$MOCK_STATE"
fi
: >"$PRIVATE_LOG"
chmod 600 "$PRIVATE_LOG"
printf '%s' "$baseline" >"$STATE/tailscale-baseline.json"
chmod 600 "$STATE/tailscale-baseline.json"

if [ "$OPERATION" = automated ]; then
  if ! git clone --no-checkout "$SOURCE_INPUT" "$SOURCE" >>"$PRIVATE_LOG" 2>&1; then fail source-clone; fi
  git -C "$SOURCE" remote set-url origin "$PORTAL_REMOTE"
  git -C "$SOURCE" fetch origin "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1 || fail source-fetch
  [ "$(git -C "$SOURCE" rev-parse 'FETCH_HEAD^{commit}')" = "$PORTAL_COMMIT" ] || fail source-commit
  [ "$(git -C "$SOURCE" rev-parse 'FETCH_HEAD^{tree}')" = "$PORTAL_TREE" ] || fail source-tree
  git -C "$SOURCE" checkout --detach "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1 || fail source-checkout
  chmod 700 "$SOURCE"
  portal_source_valid || fail source-dirty
else
  portal_source_valid || fail prepared-source
fi
[ -x "$SOURCE/scripts/tailscale-https-profile.sh" ] || fail profile-source
if [ "$OPERATION" = manual-start ]; then
  node "$MOCK_TRANSFORM" --create "$SOURCE" "$MOCK_STATE" "$public_origin" || fail manual-mock-transform
  private_regular_file "$MOCK_STATE/didit-tailnet.yml" || fail manual-mock-mode
  private_regular_file "$MOCK_STATE/didit-tailnet-receipt.json" || fail manual-mock-receipt
  export PORTAL_TAILNET_MOCK_STATE_DIR="$MOCK_STATE"
fi

mkfifo "$READY_FIFO" "$CAPABILITY_FIFO"
chmod 600 "$READY_FIFO" "$CAPABILITY_FIFO"
exec 8<>"$CAPABILITY_FIFO"
exec 9<>"$READY_FIFO"
manual_control_receipt=""
prepared_receipt_for_support=""
if [ "$OPERATION" = manual-start ]; then
  manual_control_receipt=none
  prepared_receipt_for_support="$MANUAL_PREPARED_RECEIPT"
fi
services_started_epoch="$(date +%s)"
PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_MOBILE_STATE_DIR="$STATE" \
OXID_PORTAL_MOBILE_READY_FIFO="$READY_FIFO" \
OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$CAPABILITY_FIFO" \
PORTAL_CONSUMER_LIFECYCLE="$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" \
PORTAL_CONSUMER_PREPARED_RECEIPT="$prepared_receipt_for_support" \
OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$public_origin" \
OXID_PORTAL_MOBILE_CONTROL_RECEIPT="$manual_control_receipt" \
  nohup node "$REPOSITORY_ROOT/scripts/e2e/portal-android-support.mjs" \
    >>"$PRIVATE_LOG" 2>&1 &
support_pid=$!
if ! IFS= read -r -t 900 -u 9 ready_status; then fail support-timeout; fi
exec 9>&-
rm -f -- "$READY_FIFO"
[ "$ready_status" = READY ] || fail "${ready_status#FAIL:}"
kill -0 "$support_pid" 2>/dev/null || fail support
services_seconds="$(( $(date +%s) - services_started_epoch ))"

ready="$STATE/ready.json"
manifest_path="$(jq -r '.manifestPath // empty' "$ready")"
manifest_sha="$(jq -r '.manifestSha256 // empty' "$ready")"
control_capability="$(jq -r '.controlCapability // empty' "$ready")"
holder_capability="$(jq -r '.holderCapability // empty' "$ready")"
[ "$(jq -r '.schema // empty' "$ready")" = oxid-portal-android-ready-v2 ] \
  && [ "$(jq -r '.controlOrigin // empty' "$ready")" = "$CONTROL_ORIGIN" ] \
  && [ "$(jq -r '.offerPort // empty' "$ready")" = 18094 ] \
  && [[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] \
  && [[ "$holder_capability" =~ ^[0-9a-f]{64}$ ]] || fail manifest
if [ "$OPERATION" = manual-start ]; then
  [ "$control_capability" = "$(printf '0%.0s' {1..64})" ] || fail manual-control-receipt
else
  [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] || fail manifest
  printf 'header = "Authorization: Bearer %s"\n' "$control_capability" >"$CONTROL_CONFIG"
  chmod 600 "$CONTROL_CONFIG"
fi
control_capability=""
[ -f "$manifest_path" ] && [ ! -L "$manifest_path" ] || fail manifest
[ "$(shasum -a 256 "$manifest_path" | awk '{print $1}')" = "$manifest_sha" ] || fail manifest

if [ "$OPERATION" = manual-start ]; then
  jq -cn --arg dns "$dns_name" --argjson port "$listener" --argjson mock_route "$mock_route_config" '
    {PORTAL_TAILSCALE_DNS_NAME:$dns,routes:[
      {path:"/",httpsPort:$port,upstream:"http://127.0.0.1:18090"},
      {path:"/issuer-resolver",httpsPort:$port,upstream:"http://127.0.0.1:18093"},
      {path:"/offer",httpsPort:$port,upstream:"http://127.0.0.1:18094"},
      {path:"/holder",httpsPort:$port,upstream:"http://127.0.0.1:18094"},
      $mock_route.route
    ]}' >"$XDG_CONFIG/lace-id-portal/tailscale-https.json"
else
  jq -cn --arg dns "$dns_name" --argjson port "$listener" '
    {PORTAL_TAILSCALE_DNS_NAME:$dns,routes:[
      {path:"/",httpsPort:$port,upstream:"http://127.0.0.1:18090"},
      {path:"/issuer-resolver",httpsPort:$port,upstream:"http://127.0.0.1:18093"},
      {path:"/offer",httpsPort:$port,upstream:"http://127.0.0.1:18094"},
      {path:"/holder",httpsPort:$port,upstream:"http://127.0.0.1:18094"}
    ]}' >"$XDG_CONFIG/lace-id-portal/tailscale-https.json"
fi
chmod 600 "$XDG_CONFIG/lace-id-portal/tailscale-https.json"
tailnet_started_epoch="$(date +%s)"
XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
  "$SOURCE/scripts/tailscale-https-profile.sh" setup >>"$PRIVATE_LOG" 2>&1 || fail profile-setup
profile_active=1
if [ "$OPERATION" = manual-start ]; then
  curl --noproxy '*' --fail --silent --show-error --max-time 30 \
    "$public_origin$mock_external_path" >"$STATE/manual-mock-page.html" || fail manual-mock-route
  chmod 600 "$STATE/manual-mock-page.html"
  grep -qF 'id="approve-btn"' "$STATE/manual-mock-page.html" || fail manual-mock-route
  rm -f -- "$STATE/manual-mock-page.html"
fi
tailnet_seconds="$(( $(date +%s) - tailnet_started_epoch ))"

adb_reverse_before="$(adb_device reverse --list 2>/dev/null | sort)"
if [ "$OPERATION" = automated ]; then
  adb_device shell pm clear io.medianox.oxid >/dev/null 2>&1 || fail clean-room-reset
fi
android_started_epoch="$(date +%s)"
if ! OXID_MOBILE_CUSTODY=development \
  OXID_MOBILE_PORTAL_PROFILE=tailnet-android \
  OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$public_origin" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$manifest_sha" \
    "$REPOSITORY_ROOT/scripts/run-android-tailnet.sh" >>"$PRIVATE_LOG" 2>&1; then
  build_diagnostic="$(rg '^(error(\[[A-Z0-9]+\])?:|error: could not compile|Caused by:)' "$PRIVATE_LOG" | tail -n 20 || true)"
  if [ -n "$build_diagnostic" ]; then
    redact_physical_failure <<<"$build_diagnostic" >&2
  fi
  build_diagnostic=""
  tail -n 80 "$PRIVATE_LOG" | redact_physical_failure >&2
  fail android-build
fi
android_seconds="$(( $(date +%s) - android_started_epoch ))"

holder_stage_command="run-as io.medianox.oxid sh -c 'umask 077; target=files/portal-holder.capability; candidate=files/.portal-holder.capability.tmp; rm -f \"\$candidate\" \"\$target\"; cat >\"\$candidate\"; test \"\$(wc -c <\"\$candidate\")\" -eq 64; mv \"\$candidate\" \"\$target\"'"
printf '%s' "$holder_capability" | adb_device shell "$holder_stage_command" \
  >/dev/null 2>>"$PRIVATE_LOG" || fail holder-capability-stage
holder_capability=""

if [ "$OPERATION" = manual-start ]; then
  command -v open >/dev/null 2>&1 || fail browser
  public_page_url="$public_origin/issue/index.html"
  page_content_type="$(curl --noproxy '*' --fail --silent --show-error --max-time 30 \
    --output /dev/null --write-out '%{content_type}' "$public_page_url")" || fail manual-page-html
  [[ "$page_content_type" = text/html* ]] || fail manual-page-html
  printf '%s\n' "$public_page_url" >"$MANUAL_PAGE_URL"
  chmod 600 "$MANUAL_PAGE_URL"
  baseline_sha="$(shasum -a 256 "$STATE/tailscale-baseline.json" | awk '{print $1}')"
  mock_receipt_sha="$(shasum -a 256 "$MOCK_STATE/didit-tailnet-receipt.json" | awk '{print $1}')"
  active_serve="$(tailscale serve status --json | jq -S -c '.')" || fail manual-serve-receipt
  active_serve_sha="$(sha256_text "$active_serve")"
  support_command_sha="$(process_command_sha256 "$support_pid")" || fail manual-support-receipt
  ready_seconds="$(( $(date +%s) - manual_start_epoch ))"
  supervisor_pid="$$"
  supervisor_command_sha="$(process_command_sha256 "$supervisor_pid")" || fail manual-supervisor-receipt
  jq -cn \
    --arg head "$OXID_HEAD" --arg tree "$(git -C "$REPOSITORY_ROOT" rev-parse 'HEAD^{tree}')" \
    --arg commit "$PORTAL_COMMIT" --arg portal_tree "$PORTAL_TREE" \
    --argjson support_pid "$support_pid" --arg support_sha "$support_command_sha" \
    --argjson supervisor_pid "$supervisor_pid" --arg supervisor_sha "$supervisor_command_sha" \
    --arg baseline_sha "$baseline_sha" --arg active_sha "$active_serve_sha" \
    --arg mock_receipt_sha "$mock_receipt_sha" \
    --argjson services "$services_seconds" --argjson tailnet "$tailnet_seconds" \
    --argjson android "$android_seconds" --argjson ready "$ready_seconds" \
    '{schema:"oxid-portal-tailnet-manual-session-v1",oxid:{head:$head,tree:$tree},portal:{commit:$commit,tree:$portal_tree},support:{pid:$support_pid,commandSha256:$support_sha},supervisor:{pid:$supervisor_pid,commandSha256:$supervisor_sha},serve:{baselineSha256:$baseline_sha,activeSha256:$active_sha},mock:{transformReceiptSha256:$mock_receipt_sha,externalPath:"/kyc/mock-verification",upstreamPath:"/mock-verification"},page:{html:true,mockRoute:true,holderBootstrap:true},deviceDataMode:"preserved",metrics:{servicesSeconds:$services,tailnetSeconds:$tailnet,androidSeconds:$android,readySeconds:$ready}}' \
    >"$MANUAL_RECEIPT"
  chmod 600 "$MANUAL_RECEIPT"
  exec 8>&-
  rm -f -- "$CAPABILITY_FIFO" "$ready" "$manifest_path"
  manual_ready || fail manual-not-ready
  sleep 2
  manual_ready || fail manual-readiness-unstable
  open "$public_page_url" >>"$PRIVATE_LOG" 2>&1 || fail browser
  printf 'portal-tailnet-manual: READY url=%s services=%ss tailnet=%ss android=%ss total=%ss\n' \
    "$public_page_url" "$services_seconds" "$tailnet_seconds" "$android_seconds" "$ready_seconds"
  manual_supervise || fail manual-supervisor
  trap - EXIT INT TERM
  exit 0
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
  local scenario_result="$STATE/scenario-result-$mode.json"
  open_webview || fail webview
  rm -f -- "$scenario_log" "$scenario_result"
  control_capability="$(jq -r '.controlCapability' "$ready")"
  if ! printf '%s' "$control_capability" | \
    OXID_PORTAL_CONTROL_ORIGIN="$CONTROL_ORIGIN" \
      node "$REPOSITORY_ROOT/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" \
      >"$scenario_result" 2>"$scenario_log"; then
    if ! rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$scenario_log"; then
      tail -n 12 "$scenario_log" >&2
    fi
    fail "$mode"
  fi
  control_capability=""
  jq -e --arg mode "$mode" '
    type == "object"
    and .mode == $mode
    and .passed == true
    and (.measurements | type == "object")
  ' "$scenario_result" >/dev/null || fail "$mode-marker"
  if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$scenario_result"; then
    fail "$mode-marker-secret"
  fi
  chmod 600 "$scenario_result"
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

for negative_mode in route-refuse malformed protocol-error protocol-timeout issue-error; do
  arm_and_route_offer warm
  run_scenario "$negative_mode"
done
arm_and_route_offer warm
run_scenario issue

credential_header="$(adb_device shell run-as io.medianox.oxid od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$(adb_device shell run-as io.medianox.oxid wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r\n')"
[ "$credential_header" = 4f58494456433031 ] && [ "$credential_key_size" = 32 ] || fail encrypted-store
encrypted_persistence=true
old_pid="$(adb_device shell pidof io.medianox.oxid | tr -d '\r\n')"
adb_device shell am force-stop io.medianox.oxid
adb_device shell am start -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
for _attempt in $(seq 1 60); do
  new_pid="$(adb_device shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r\n' || true)"
  [ -n "$new_pid" ] && [ "$new_pid" != "$old_pid" ] && break
  sleep 0.25
done
[ -n "${new_pid:-}" ] && [ "$new_pid" != "$old_pid" ] || fail process-restart
process_restart=true
run_scenario restored
duration_seconds="$SECONDS"
[ "$duration_seconds" -le 300 ] || fail duration
completed_within_300_seconds=true

counters="$(control_curl --max-time 10 "$CONTROL_ORIGIN/counters")"
exact_protocol_counters="$(jq -r '
  .authorizationMetadata == 3
  and .credential == 1
  and .issuerMetadata == 6
  and .issuerResolution == 3
  and .issuerResolutionSuccess == 3
  and .holderPublications == 1
  and .kyc == 14
  and .nonce == 1
  and .other == 0
  and .token == 2
' <<<"$counters")"
[ "$exact_protocol_counters" = true ] || fail protocol-counts
[ "$(adb_device reverse --list 2>/dev/null | sort)" = "$adb_reverse_before" ] || fail adb-reverse
no_adb_reverse=true

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
exact_serve_receipt_cleanup=true
control_curl --max-time 10 -X POST "$CONTROL_ORIGIN/complete" >/dev/null || fail support-finish
for _attempt in $(seq 1 90); do
  kill -0 "$support_pid" 2>/dev/null || break
  sleep 1
done
kill -0 "$support_pid" 2>/dev/null && fail support-stop
wait "$support_pid" || fail support-stop
support_pid=""
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail portal-cleanup
portal_consumer_cleanup=true

os_version="$(adb_device shell getprop ro.build.version.release | tr -d '\r\n')"
api_level="$(adb_device shell getprop ro.build.version.sdk | tr -d '\r\n')"
[[ "$api_level" =~ ^[0-9]+$ ]] || fail platform-evidence
[ "$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)" = "$OXID_HEAD" ] || fail head-changed
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty

scenario_results="$(jq -s -c 'sort_by(.mode)' "$STATE"/scenario-result-*.json)"
jq -e '
  length == 9
  and ([.[].mode] == [
    "cold-route",
    "issue",
    "issue-error",
    "malformed",
    "prepare-holder",
    "protocol-error",
    "protocol-timeout",
    "restored",
    "route-refuse"
  ])
  and all(.[]; .passed == true)
' <<<"$scenario_results" >/dev/null || fail scenario-results

evidence="$REPOSITORY_ROOT/target/portal-android-physical/evidence.json"
mkdir -p "$(dirname -- "$evidence")"
candidate_base="$(mktemp "$(dirname -- "$evidence")/.evidence-base.XXXXXX")"
candidate="$(mktemp "$(dirname -- "$evidence")/.evidence.XXXXXX")"
jq -cn \
  --arg head "$OXID_HEAD" --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  --arg resolver "$resolver_image" --arg didManager "$did_manager_image" --arg issuer "$issuer_image" \
  --arg os "$os_version" --arg api "$api_level" --argjson duration "$duration_seconds" \
  --argjson counters "$counters" --argjson scenarios "$scenario_results" \
  --argjson encryptedPersistence "$encrypted_persistence" \
  --argjson processRestart "$process_restart" \
  --argjson noAdbReverse "$no_adb_reverse" \
  --argjson tailnetIdentityDiscovered "$tailnet_identity_discovered" \
  --argjson temporaryListenerDiscovered "$temporary_listener_discovered" \
  --argjson preservedStandaloneRoutes "$preserved_standalone_routes" \
  --argjson exactServeReceiptCleanup "$exact_serve_receipt_cleanup" \
  --argjson portalConsumerCleanup "$portal_consumer_cleanup" \
  -f "$EVIDENCE_FILTER" >"$candidate_base"
if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$candidate_base"; then
  rm -f -- "$candidate_base" "$candidate"
  fail evidence-schema
fi
secret_free_evidence=true
jq -c --argjson secretFreeEvidence "$secret_free_evidence" \
  '.acceptance.secretFreeEvidence = $secretFreeEvidence' "$candidate_base" >"$candidate"
rm -f -- "$candidate_base"
if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|capability|seed|serial|\.ts\.net' "$candidate"; then
  rm -f -- "$candidate"
  fail evidence-schema
fi
jq -e '
  .acceptance.refusalSecretEndpointCalls == 0
  and (.acceptance | del(.refusalSecretEndpointCalls) | [.[]] | all(.[]; . == true))
' "$candidate" >/dev/null || {
  rm -f -- "$candidate"
  fail evidence-measurements
}
chmod 600 "$candidate"
mv -f -- "$candidate" "$evidence"
printf 'android-portal-tailnet: PASS head=%s evidence=%s\n' "$OXID_HEAD" "${evidence#"$REPOSITORY_ROOT/"}"
