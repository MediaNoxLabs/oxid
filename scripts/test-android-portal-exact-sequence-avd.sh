#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=

readonly ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly PROCESS_SUPPORT="$ROOT/scripts/e2e/android-avd-process-ownership.sh"
readonly PORTAL_STATE="$ROOT/target/portal-virtual-mobile/runtime"
readonly RUN_ROOT="$ROOT/target/android-portal-exact-sequence-avd"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly PRIVATE_LOG="$PRIVATE_STATE/journey.log"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly BUILD_SOURCE="$PRIVATE_STATE/build-source"
readonly PACKAGE="io.medianox.oxid"
readonly ACTIVITY="$PACKAGE/dev.dioxus.main.MainActivity"
readonly TRIGGER="openid-credential-offer://standalone-portal-test-fetch"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly EMULATOR_PORT=5562
readonly SERIAL="emulator-$EMULATOR_PORT"
readonly CDP_PORT=19247
readonly -a REVERSE_PORTS=(6300 8088 9944 18090 18091 18093)
readonly -a PORTAL_PORTS=(18090 18091 18092 18093 18094 18095)
readonly -a SHARED_PORTS=(6300 8088 9944)

# shellcheck source=e2e/android-avd-process-ownership.sh
source "$PROCESS_SUPPORT"

portal_pid=""
portal_identity=""
emulator_pid=""
emulator_identity=""
launcher_pid=""
launcher_identity=""
forward_active=0
emulator_online=0
cleanup_running=0
cleanup_ok=true
journey_status="not_started"
failure_phase="none"
head=""
apk_sha256=""
reverse_before=""
shared_before=""
cold_result=false
holder_result=false
warm_result=false
same_pid=false
cold_handoff_before="unknown"
cold_handoff_after="unknown"
warm_handoff_before="unknown"
warm_handoff_after="unknown"
cold_capability_absent=false
warm_capability_absent=false
cold_delta='{}'
warm_delta='{}'
warm_intents=0
websocket_url=""
portal_cleanup=false
package_cleanup=false
emulator_cleanup=false
reverse_cleanup=false
forward_cleanup=false
listener_cleanup=false
shared_listeners_preserved=false
build_cleanup=false
private_logs_removed=false
head_clean=false

fail() {
  failure_phase="$1"
  printf 'android-portal-exact-sequence-avd: FAIL phase=%s\n' "$failure_phase" >&2
  exit 1
}

for command_name in awk cargo curl docker git jq lsof mktemp nix node ps rg rustup sed shasum stat tar timeout; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
 done
if timeout -k 1s 0.1s sleep 5; then fail timeout-capability; else [ "$?" -eq 124 ] || fail timeout-capability; fi
export OXID_PROCESS_TIMEOUT_COMMAND="$(command -v timeout)"

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ -d "$HOME/Library/Android/sdk" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
readonly ADB="$android_sdk/platform-tools/adb"
readonly EMULATOR="$android_sdk/emulator/emulator"
[ -x "$ADB" ] && [ -x "$EMULATOR" ] || fail android-sdk

avd="${OXID_ANDROID_AVD:-}"
[[ "$avd" =~ ^[A-Za-z0-9._-]+$ ]] || fail explicit-avd
avd_found=false
for avd_ini in "${ANDROID_AVD_HOME:-}/$avd.ini" "${ANDROID_SDK_HOME:-}/avd/$avd.ini" "$HOME/.android/avd/$avd.ini"; do
  if [ -f "$avd_ini" ] && [ ! -L "$avd_ini" ]; then avd_found=true; break; fi
 done
[ "$avd_found" = true ] || fail avd-definition

run_deadline() {
  local seconds="$1"; shift
  timeout -k 5s "${seconds}s" "$@"
}

adb_device() {
  run_deadline 30 env ANDROID_SERIAL="$SERIAL" "$ADB" "$@"
}

cleanup_adb() {
  run_deadline 15 env ANDROID_SERIAL="$SERIAL" "$ADB" "$@"
}

control_curl() {
  run_deadline 15 curl --config "$PORTAL_STATE/control-curl.conf" --noproxy '*' \
    --fail --silent --show-error --max-time 10 "$@"
}

listener_fingerprint() {
  local port output
  for port in "$@"; do
    output="$(run_deadline 5 lsof -nP -iTCP:"$port" -sTCP:LISTEN -Fpcn 2>/dev/null || true)"
    printf '%s:%s\n' "$port" "$output"
  done
}

child_active() {
  local pid="$1" stat_value
  [ -n "$pid" ] || return 1
  stat_value="$(run_deadline 5 ps -p "$pid" -o stat= 2>/dev/null || true)"
  [ -n "$stat_value" ] && [[ "$stat_value" != Z* ]]
}

wait_child_for() {
  local pid="$1" attempts="$2"
  for ((_attempt = 0; _attempt < attempts; _attempt++)); do
    child_active "$pid" || return 0
    sleep 0.2
  done
  return 1
}

remove_forward() {
  if [ "$forward_active" -eq 1 ]; then
    cleanup_adb forward --remove "tcp:$CDP_PORT" >/dev/null 2>&1 || return 1
    forward_active=0
  fi
}

write_evidence() {
  local candidate outcome incoming_status="$1"
  [ -n "$head" ] || return 0
  outcome="$journey_status"
  if [ "$incoming_status" -ne 0 ] && [ "$outcome" = not_started ]; then outcome="pre_warm_failure"; fi
  mkdir -p "$RUN_ROOT"
  candidate="$(mktemp "$RUN_ROOT/.evidence.XXXXXX")"
  jq -cn \
    --arg schema oxid-portal-android-exact-sequence-avd-v1 \
    --arg head "$head" --arg apkSha256 "$apk_sha256" --arg outcome "$outcome" \
    --arg classification "$failure_phase" \
    --arg coldBefore "$cold_handoff_before" --arg coldAfter "$cold_handoff_after" \
    --arg warmBefore "$warm_handoff_before" --arg warmAfter "$warm_handoff_after" \
    --argjson coldResult "$cold_result" --argjson holderResult "$holder_result" \
    --argjson warmResult "$warm_result" --argjson samePid "$same_pid" \
    --argjson coldCapabilityAbsent "$cold_capability_absent" \
    --argjson warmCapabilityAbsent "$warm_capability_absent" \
    --argjson coldDelta "$cold_delta" --argjson warmDelta "$warm_delta" \
    --argjson exactlyOneWarmIntent "$([ "$warm_intents" -eq 1 ] && printf true || printf false)" \
    --argjson portalCleanup "$portal_cleanup" --argjson packageCleanup "$package_cleanup" \
    --argjson emulatorCleanup "$emulator_cleanup" --argjson reverseCleanup "$reverse_cleanup" \
    --argjson forwardCleanup "$forward_cleanup" --argjson listenerCleanup "$listener_cleanup" \
    --argjson sharedListenersPreserved "$shared_listeners_preserved" \
    --argjson buildCleanup "$build_cleanup" --argjson privateLogsRemoved "$private_logs_removed" \
    --argjson headClean "$head_clean" \
    '{schema:$schema,head:$head,apkSha256:$apkSha256,outcome:$outcome,
      classification:$classification,
      observations:{
        cold:{strictRoute:$coldResult,handoffBefore:$coldBefore,handoffAfter:$coldAfter,
          capabilityAbsent:$coldCapabilityAbsent,counterDelta:$coldDelta},
        holder:{prepared:$holderResult,sameProcessSession:$samePid},
        warm:{previewRefusal:$warmResult,handoffBefore:$warmBefore,handoffAfter:$warmAfter,
          capabilityAbsent:$warmCapabilityAbsent,counterDelta:$warmDelta,
          exactlyOneIntent:$exactlyOneWarmIntent,sameProcessSession:$samePid}
      },
      cleanup:{portal:$portalCleanup,package:$packageCleanup,emulator:$emulatorCleanup,
        reverseMappings:$reverseCleanup,forwardMapping:$forwardCleanup,listeners:$listenerCleanup,
        sharedListenersPreserved:$sharedListenersPreserved,buildOutput:$buildCleanup,
        privateLogs:$privateLogsRemoved,headClean:$headClean}}' >"$candidate"
  jq -e '
    .schema == "oxid-portal-android-exact-sequence-avd-v1"
    and (.head | test("^[0-9a-f]{40}$"))
    and ((.apkSha256 == "") or (.apkSha256 | test("^[0-9a-f]{64}$")))
    and (.observations.cold.handoffBefore | IN("unknown", "ready"))
    and (.observations.cold.handoffAfter | IN("unknown", "empty"))
    and (.observations.warm.handoffBefore | IN("unknown", "ready"))
    and (.observations.warm.handoffAfter | IN("unknown", "empty", "ready", "consuming"))
    and ([.cleanup[]] | all(type == "boolean"))
  ' "$candidate" >/dev/null || { rm -f -- "$candidate"; return 1; }
  if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|serial|\.ts\.net|/Users/|/tmp/' "$candidate"; then
    rm -f -- "$candidate"
    return 1
  fi
  chmod 600 "$candidate"
  mv -f -- "$candidate" "$EVIDENCE"
}

cleanup() {
  local incoming=$? current command_line package_path after_shared after_portal port
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM HUP
  set +e

  if [ "$emulator_online" -eq 1 ]; then
    remove_forward
    [ "$?" -eq 0 ] && forward_cleanup=true || cleanup_ok=false

    package_path="$(cleanup_adb shell pm path "$PACKAGE" 2>/dev/null | tr -d '\r\n')"
    if [ -n "$package_path" ]; then
      cleanup_adb shell "run-as $PACKAGE sh -c 'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp'" \
        >/dev/null 2>&1 || cleanup_ok=false
      cleanup_adb uninstall "$PACKAGE" >/dev/null 2>&1 || cleanup_ok=false
    fi
    package_path="$(cleanup_adb shell pm path "$PACKAGE" 2>/dev/null | tr -d '\r\n')"
    [ -z "$package_path" ] && package_cleanup=true || cleanup_ok=false

    for port in "${REVERSE_PORTS[@]}"; do
      cleanup_adb reverse --remove "tcp:$port" >/dev/null 2>&1 || true
    done
    current="$(cleanup_adb reverse --list 2>/dev/null | sort || true)"
    [ "$current" = "$reverse_before" ] && reverse_cleanup=true || cleanup_ok=false
  fi

  if [ -n "$portal_pid" ]; then
    if child_active "$portal_pid"; then
      if [ -f "$PORTAL_STATE/control-curl.conf" ]; then
        control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
      fi
      wait_child_for "$portal_pid" 600 || {
        if oxid_process_still_owned "$portal_pid" "$portal_identity"; then
          oxid_terminate_owned_process "$portal_pid" "$portal_identity" 100 || cleanup_ok=false
        else
          cleanup_ok=false
        fi
      }
    fi
    wait "$portal_pid" >/dev/null 2>&1 || true
    portal_pid=""
  fi
  current="$(run_deadline 15 docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null || true)"
  [ -z "$current" ] && portal_cleanup=true || cleanup_ok=false

  if [ -n "$launcher_pid" ]; then
    if child_active "$launcher_pid"; then
      if oxid_process_still_owned "$launcher_pid" "$launcher_identity"; then
        oxid_terminate_owned_process "$launcher_pid" "$launcher_identity" 100 || cleanup_ok=false
      else
        cleanup_ok=false
      fi
    fi
    wait "$launcher_pid" >/dev/null 2>&1 || true
    launcher_pid=""
  fi

  if [ -n "$emulator_pid" ]; then
    if child_active "$emulator_pid"; then
      if oxid_process_still_owned "$emulator_pid" "$emulator_identity"; then
        command_line="$(oxid_identity_command "$emulator_identity")"
        if oxid_emulator_command_matches "$command_line" "$EMULATOR" "$avd" "$EMULATOR_PORT"; then
          oxid_terminate_owned_process "$emulator_pid" "$emulator_identity" 200 || cleanup_ok=false
        else
          cleanup_ok=false
        fi
      else
        cleanup_ok=false
      fi
    fi
    wait "$emulator_pid" >/dev/null 2>&1 || true
    emulator_pid=""
    [ "$cleanup_ok" = true ] && emulator_cleanup=true
  fi

  after_portal="$(listener_fingerprint "${PORTAL_PORTS[@]}")"
  if ! rg -q '[[:digit:]]+:p[0-9]+' <<<"$after_portal"; then listener_cleanup=true; else cleanup_ok=false; fi
  after_shared="$(listener_fingerprint "${SHARED_PORTS[@]}")"
  [ "$after_shared" = "$shared_before" ] && shared_listeners_preserved=true || cleanup_ok=false

  run_deadline 30 rm -rf -- "$BUILD_SOURCE" >/dev/null 2>&1
  [ ! -e "$BUILD_SOURCE" ] && build_cleanup=true || cleanup_ok=false
  run_deadline 30 rm -rf -- "$PRIVATE_STATE" "$PORTAL_STATE" >/dev/null 2>&1
  if [ ! -e "$PRIVATE_STATE" ] && [ ! -e "$PORTAL_STATE" ]; then private_logs_removed=true; else cleanup_ok=false; fi
  if [ "$(run_deadline 10 git -C "$ROOT" rev-parse HEAD 2>/dev/null)" = "$head" ] \
    && [ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    head_clean=true
  else
    cleanup_ok=false
  fi

  write_evidence "$incoming" || cleanup_ok=false
  if [ "$cleanup_ok" != true ]; then
    incoming=1
    printf 'android-portal-exact-sequence-avd: cleanup could not prove exact restoration\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'failure_phase=signal-int; exit 130' INT
trap 'failure_phase=signal-term; exit 143' TERM
trap 'failure_phase=signal-hup; exit 129' HUP

[ -z "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
head="$(git -C "$ROOT" rev-parse HEAD)"
[[ "$head" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head
run_deadline 20 git -C "$ROOT" verify-commit "$head" >/dev/null 2>&1 || fail oxid-signature
[ -z "$(run_deadline 10 docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] \
  || fail occupied-portal-project

for port in "${PORTAL_PORTS[@]}"; do
  [ -z "$(run_deadline 5 lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)" ] || fail occupied-portal-listener
 done
shared_before="$(listener_fingerprint "${SHARED_PORTS[@]}")"
for port in "${SHARED_PORTS[@]}"; do
  rg -q "^${port}:p[0-9]+" <<<"$shared_before" || fail shared-listener
 done

existing_avd="$(run_deadline 5 ps -axo pid=,command= | awk -v avd="$avd" '
  { for (i = 2; i <= NF; i++) if ($(i - 1) == "-avd" && $i == avd) print $1 }
')"
[ -z "$existing_avd" ] || fail avd-in-use
if adb_device get-state >/dev/null 2>&1; then fail transport-in-use; fi
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$EMULATOR_PORT" -sTCP:LISTEN 2>/dev/null || true)" ] \
  || fail console-port-in-use
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$((EMULATOR_PORT + 1))" -sTCP:LISTEN 2>/dev/null || true)" ] \
  || fail adb-port-in-use
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$CDP_PORT" -sTCP:LISTEN 2>/dev/null || true)" ] \
  || fail cdp-port-in-use

umask 077
rm -rf -- "$PRIVATE_STATE"
mkdir -p "$PRIVATE_STATE"
chmod 700 "$PRIVATE_STATE"
: >"$PRIVATE_LOG"
chmod 600 "$PRIVATE_LOG"

"$EMULATOR" -avd "$avd" -read-only -no-snapshot -no-snapshot-save -port "$EMULATOR_PORT" \
  </dev/null >>"$PRIVATE_LOG" 2>&1 &
emulator_pid=$!
for _attempt in $(seq 1 50); do
  emulator_identity="$(oxid_process_identity "$emulator_pid" 2>/dev/null || true)"
  [ -n "$emulator_identity" ] && break
  sleep 0.1
 done
[ -n "$emulator_identity" ] || fail emulator-identity
oxid_emulator_command_matches "$(oxid_identity_command "$emulator_identity")" "$EMULATOR" "$avd" "$EMULATOR_PORT" \
  || fail emulator-command

for _attempt in $(seq 1 240); do
  child_active "$emulator_pid" || fail emulator-exited
  if [ "$(adb_device get-state 2>/dev/null || true)" = device ] && \
    [ "$(adb_device shell getprop sys.boot_completed 2>/dev/null | tr -d '\r\n')" = 1 ]; then
    emulator_online=1
    break
  fi
  sleep 1
 done
[ "$emulator_online" -eq 1 ] || fail emulator-boot
[ "$(adb_device shell getprop ro.kernel.qemu | tr -d '\r\n')" = 1 ] || fail qemu
[ "$(adb_device emu avd name 2>/dev/null | sed -n '1{s/\r$//;p;}')" = "$avd" ] || fail avd-identity
[ -z "$(adb_device shell pm path "$PACKAGE" 2>/dev/null | tr -d '\r\n')" ] || fail preinstalled-package
reverse_before="$(adb_device reverse --list 2>/dev/null | sort)"
for port in "${REVERSE_PORTS[@]}"; do
  if awk -v route="tcp:$port" '$2 == route || $3 == route { found=1 } END { exit !found }' <<<"$reverse_before"; then
    fail occupied-reverse
  fi
 done

"$ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" >>"$PRIVATE_LOG" 2>&1 &
portal_pid=$!
for _attempt in $(seq 1 50); do
  portal_identity="$(oxid_process_identity "$portal_pid" 2>/dev/null || true)"
  [ -n "$portal_identity" ] && break
  sleep 0.1
 done
[ -n "$portal_identity" ] || fail portal-identity
for _attempt in $(seq 1 4500); do
  child_active "$portal_pid" || fail portal-exited
  if [ -f "$PORTAL_STATE/ready.json" ] && [ -f "$PORTAL_STATE/control-curl.conf" ] \
    && [ -f "$PORTAL_STATE/portal-offer.capability" ]; then break; fi
  sleep 0.2
 done
[ -f "$PORTAL_STATE/ready.json" ] && [ -p "$PORTAL_STATE/capability.fifo" ] || fail portal-ready
if capability_mode="$(stat -c '%a' "$PORTAL_STATE/portal-offer.capability" 2>/dev/null)"; then
  :
else
  capability_mode="$(stat -f '%Lp' "$PORTAL_STATE/portal-offer.capability")"
fi
[ "$capability_mode" = 600 ] || fail host-capability-mode
[ "$(wc -c <"$PORTAL_STATE/portal-offer.capability" | tr -d ' ')" = 64 ] || fail host-capability-size
manifest_path="$(jq -r '.manifestPath // empty' "$PORTAL_STATE/ready.json")"
manifest_sha="$(jq -r '.manifestSha256 // empty' "$PORTAL_STATE/ready.json")"
[[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] || fail portal-manifest

archive="$PRIVATE_STATE/source.tar"
run_deadline 60 git -C "$ROOT" archive --format=tar --output="$archive" "$head" || fail build-source-archive
mkdir -p "$BUILD_SOURCE"
run_deadline 60 tar -xf "$archive" -C "$BUILD_SOURCE" || fail build-source-extract
rm -f -- "$archive"
[ ! -e "$BUILD_SOURCE/target" ] || fail isolated-build-output

timeout -k 30s 3700s env \
  OXID_ANDROID_DEVICE="$SERIAL" \
  OXID_ANDROID_AVD="$avd" \
  OXID_ANDROID_ADB_TIMEOUT_SECONDS=45 \
  OXID_MOBILE_CUSTODY=development \
  OXID_STANDALONE_NETWORK_PROFILE=local \
  OXID_MOBILE_PORTAL_PROFILE=local \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$manifest_sha" \
  "$BUILD_SOURCE/scripts/run-android-emulator.sh" >>"$PRIVATE_LOG" 2>&1 &
launcher_pid=$!
for _attempt in $(seq 1 50); do
  launcher_identity="$(oxid_process_identity "$launcher_pid" 2>/dev/null || true)"
  [ -n "$launcher_identity" ] && break
  sleep 0.1
 done
[ -n "$launcher_identity" ] || fail launcher-identity
wait "$launcher_pid" || fail android-launcher
launcher_pid=""
launcher_identity=""

apk="$BUILD_SOURCE/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] && [ ! -L "$apk" ] || fail owned-apk
apk_sha256="$(shasum -a 256 "$apk" | awk '{print $1}')"
[[ "$apk_sha256" =~ ^[0-9a-f]{64}$ ]] || fail apk-digest
[ -n "$(adb_device shell pm path "$PACKAGE" 2>/dev/null | tr -d '\r\n')" ] || fail package-install
reverse_after="$(adb_device reverse --list 2>/dev/null | sort)"
for port in "${REVERSE_PORTS[@]}"; do
  awk -v route="tcp:$port" '$2 == route && $3 == route { found=1 } END { exit !found }' <<<"$reverse_after" \
    || fail reverse-install
 done

capability_absent() {
  adb_device shell "run-as $PACKAGE sh -c 'test ! -e files/portal-offer.capability && test ! -e files/.portal-offer.capability.tmp'" \
    >/dev/null 2>&1
}

wait_capability_absent() {
  for _attempt in $(seq 1 100); do capability_absent && return 0; sleep 0.1; done
  return 1
}

stage_capability_file() {
  local source_kind="$1" source_path="$2"
  local stage="run-as $PACKAGE sh -c 'umask 077; target=files/portal-offer.capability; candidate=files/.portal-offer.capability.tmp; rm -f \"\$candidate\" \"\$target\"; cat >\"\$candidate\"; test \"\$(wc -c <\"\$candidate\")\" -eq 64; chmod 600 \"\$candidate\"; mv \"\$candidate\" \"\$target\"'"
  if [ "$source_kind" = file ]; then
    run_deadline 10 cat "$source_path" | adb_device shell "$stage" >>"$PRIVATE_LOG" 2>&1
  else
    run_deadline 10 head -c 64 "$source_path" | adb_device shell "$stage" >>"$PRIVATE_LOG" 2>&1
  fi
  metadata="$(adb_device shell "run-as $PACKAGE stat -c '%s %a' files/portal-offer.capability" 2>/dev/null | tr -d '\r\n')"
  [ "$metadata" = "64 600" ]
}

handoff_state() {
  control_curl "$CONTROL_ORIGIN/handoff-status" | jq -r '.state'
}

counter_snapshot() {
  control_curl "$CONTROL_ORIGIN/counters" | jq -cS .
}

counter_delta() {
  jq -cn --argjson before "$1" --argjson after "$2" '
    $before | with_entries(.value = ($after[.key] - .value))'
}

open_webview() {
  local pid="$1" pages
  websocket_url=""
  remove_forward || return 1
  adb_device forward "tcp:$CDP_PORT" "localabstract:webview_devtools_remote_$pid" >/dev/null || return 1
  forward_active=1
  for _attempt in $(seq 1 120); do
    pages="$(run_deadline 5 curl --noproxy '*' --fail --silent --show-error --max-time 2 \
      "http://127.0.0.1:$CDP_PORT/json" 2>/dev/null || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' \
      <<<"$pages" 2>/dev/null || true)"
    [ -n "$websocket_url" ] && return 0
    sleep 0.25
  done
  return 1
}

app_pid() {
  adb_device shell pidof "$PACKAGE" 2>/dev/null | tr -d '\r\n'
}

run_scenario() {
  local mode="$1" pid control_capability result="$PRIVATE_STATE/scenario-$mode.json"
  pid="$(app_pid)"
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  open_webview "$pid" || return 1
  control_capability="$(jq -r '.controlCapability // empty' "$PORTAL_STATE/ready.json")"
  [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "$control_capability" | run_deadline 180 env OXID_PORTAL_CONTROL_ORIGIN="$CONTROL_ORIGIN" \
    node "$ROOT/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" \
    >"$result" 2>>"$PRIVATE_LOG" || return 1
  control_capability=""
  jq -e --arg mode "$mode" '.mode == $mode and .passed == true and (.measurements | type == "object")' \
    "$result" >/dev/null || return 1
  if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|serial|\.ts\.net' "$result"; then
    return 1
  fi
  chmod 600 "$result"
  remove_forward
}

journey_status="pre_warm"
cold_handoff_before="$(handoff_state)"
[ "$cold_handoff_before" = ready ] || fail cold-handoff-ready
cold_before="$(counter_snapshot)"
stage_capability_file file "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-stage
rm -f -- "$PORTAL_STATE/portal-offer.capability"
adb_device shell am force-stop "$PACKAGE" >/dev/null || fail cold-stop
adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" \
  >/dev/null 2>>"$PRIVATE_LOG" || fail cold-intent
run_scenario cold-route || fail cold-route
cold_result=true
session_pid="$(app_pid)"
[[ "$session_pid" =~ ^[1-9][0-9]*$ ]] || fail cold-pid
cold_after="$(counter_snapshot)"
cold_delta="$(counter_delta "$cold_before" "$cold_after")"
jq -e 'all(.[]; . == 0)' <<<"$cold_delta" >/dev/null || fail cold-counters
cold_handoff_after="$(handoff_state)"
[ "$cold_handoff_after" = empty ] || fail cold-handoff-empty
wait_capability_absent || fail cold-capability-present
cold_capability_absent=true

run_scenario prepare-holder || fail prepare-holder
holder_result=true
[ "$(app_pid)" = "$session_pid" ] || fail holder-pid

control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null || fail warm-offer-arm
warm_handoff_before="$(handoff_state)"
[ "$warm_handoff_before" = ready ] || fail warm-handoff-ready
stage_capability_file fifo "$PORTAL_STATE/capability.fifo" || fail warm-capability-stage
warm_before="$(counter_snapshot)"
[ "$(app_pid)" = "$session_pid" ] || fail pre-warm-pid
warm_intents=1
adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" \
  >/dev/null 2>>"$PRIVATE_LOG" || fail warm-intent
[ "$(app_pid)" = "$session_pid" ] || fail warm-pid
same_pid=true
journey_status="warm_delivered"
if run_scenario route-refuse; then
  warm_result=true
  journey_status="warm_pass"
else
  journey_status="warm_failure"
  failure_phase="warm-route-refuse"
fi
warm_after="$(counter_snapshot)"
warm_delta="$(counter_delta "$warm_before" "$warm_after")"
warm_handoff_after="$(handoff_state)"
if wait_capability_absent; then warm_capability_absent=true; fi

if [ "$warm_result" = true ]; then
  jq -e '.authorizationMetadata == 1 and .issuerMetadata == 1 and .token == 0
    and .nonce == 0 and .credential == 0 and .issuerResolution == 0
    and .issuerResolutionSuccess == 0 and .other == 0 and .kyc == 0' \
    <<<"$warm_delta" >/dev/null || fail warm-counters
  [ "$warm_handoff_after" = empty ] || fail warm-handoff-empty
  [ "$warm_capability_absent" = true ] || fail warm-capability-present
  printf 'android-portal-exact-sequence-avd: PASS head=%s outcome=warm_pass\n' "$head"
else
  printf 'android-portal-exact-sequence-avd: REPRODUCED classification=warm-route-refuse\n' >&2
  exit 1
fi
