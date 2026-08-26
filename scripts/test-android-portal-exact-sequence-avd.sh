#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly PROCESS_SUPPORT="$ROOT/scripts/e2e/android-avd-process-ownership.sh"
readonly PORTAL_STATE="$ROOT/target/portal-virtual-mobile/runtime"
readonly RUN_ROOT="$ROOT/target/android-portal-exact-sequence-avd"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly PRIVATE_LOG="$PRIVATE_STATE/journey.log"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly BUILD_SOURCE="$PRIVATE_STATE/build-source"
readonly PACKAGE="io.medianox.oxid"
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
emulator_pid=""
launcher_pid=""
forward_active=0
emulator_online=0
cleanup_running=0
cleanup_ok=true
private_state_owned=0
portal_ready=0
build_owned=0
launcher_mutation_owned=0
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
warm_intents_attempted=0
warm_intents_delivered=0
websocket_url=""
emulator_cleanup=false
reverse_cleanup=false
forward_cleanup=false
listener_cleanup=false
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

adb_text() {
  local output
  output="$(adb_device "$@")" || return 1
  printf '%s' "${output//$'\r'/}"
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

remove_forward() {
  if [ "$forward_active" -eq 1 ]; then
    cleanup_adb forward --remove "tcp:$CDP_PORT" >/dev/null 2>&1 || return 1
    forward_active=0
  fi
}

write_evidence() {
  local candidate outcome incoming_status="$1"
  [ -n "$head" ] && [[ "$journey_status" = warm_* ]] || return 0
  outcome="$journey_status"
  if [ "$incoming_status" -ne 0 ] && [ "$outcome" = not_started ]; then outcome="pre_warm_failure"; fi
  run_deadline 5 mkdir -p "$RUN_ROOT" || return 1
  candidate="$(run_deadline 5 mktemp "$RUN_ROOT/.evidence.XXXXXX")" || return 1
  run_deadline 10 jq -cn \
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
    --argjson warmIntentAttempted "$([ "$warm_intents_attempted" -eq 1 ] && printf true || printf false)" \
    --argjson exactlyOneWarmIntent "$([ "$warm_intents_delivered" -eq 1 ] && printf true || printf false)" \
    --argjson emulatorCleanup "$emulator_cleanup" --argjson reverseCleanup "$reverse_cleanup" \
    --argjson forwardCleanup "$forward_cleanup" --argjson listenerCleanup "$listener_cleanup" \
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
          intentAttempted:$warmIntentAttempted,exactlyOneIntent:$exactlyOneWarmIntent,
          sameProcessSession:$samePid}
      },
      cleanup:{emulator:$emulatorCleanup,
        reverseMappings:$reverseCleanup,forwardMapping:$forwardCleanup,listeners:$listenerCleanup,
        buildOutput:$buildCleanup,
        privateLogs:$privateLogsRemoved,headClean:$headClean}}' >"$candidate"
  run_deadline 10 jq -e '
    .schema == "oxid-portal-android-exact-sequence-avd-v1"
    and (.head | test("^[0-9a-f]{40}$"))
    and ((.apkSha256 == "") or (.apkSha256 | test("^[0-9a-f]{64}$")))
    and (.observations.cold.handoffBefore | IN("unknown", "ready"))
    and (.observations.cold.handoffAfter | IN("unknown", "empty"))
    and (.observations.warm.handoffBefore | IN("unknown", "ready"))
    and (.observations.warm.handoffAfter | IN("unknown", "empty", "ready", "consuming"))
    and ([.cleanup[]] | all(type == "boolean"))
  ' "$candidate" >/dev/null || { run_deadline 5 rm -f -- "$candidate"; return 1; }
  if run_deadline 5 rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|serial|\.ts\.net|/Users/|/tmp/' "$candidate"; then
    run_deadline 5 rm -f -- "$candidate"
    return 1
  fi
  run_deadline 5 chmod 600 "$candidate" || return 1
  run_deadline 5 mv -f -- "$candidate" "$EVIDENCE"
}

cleanup() {
  local incoming=$? current package_path after_portal port emulator_status=0 emulator_wait=1
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM HUP
  set +e

  if [ -n "$launcher_pid" ]; then
    if oxid_job_is_running "$launcher_pid"; then
      oxid_terminate_supervised_job "$launcher_pid" || cleanup_ok=false
    else
      wait "$launcher_pid" >/dev/null 2>&1 || true
    fi
    launcher_pid=""
  fi

  if [ "$emulator_online" -eq 1 ]; then
    remove_forward
    [ "$?" -eq 0 ] && forward_cleanup=true || cleanup_ok=false

    if [ "$launcher_mutation_owned" -eq 1 ]; then
      package_path="$(cleanup_adb shell pm path "$PACKAGE" 2>/dev/null || true)"
      package_path="${package_path//$'\r'/}"
      if [ -n "$package_path" ]; then
        cleanup_adb shell "run-as $PACKAGE sh -c 'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp'" \
          >/dev/null 2>&1 || cleanup_ok=false
        cleanup_adb uninstall "$PACKAGE" >/dev/null 2>&1 || cleanup_ok=false
      fi
      for port in "${REVERSE_PORTS[@]}"; do
        cleanup_adb reverse --remove "tcp:$port" >/dev/null 2>&1 || true
      done
      current="$(cleanup_adb reverse --list 2>/dev/null | run_deadline 5 sort || true)"
      [ "$current" = "$reverse_before" ] && reverse_cleanup=true || cleanup_ok=false
    fi
  fi

  if [ -n "$portal_pid" ]; then
    if oxid_job_is_running "$portal_pid"; then
      if [ "$portal_ready" -eq 1 ] && [ -f "$PORTAL_STATE/control-curl.conf" ]; then
        control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
      fi
      if oxid_job_is_running "$portal_pid"; then
        oxid_terminate_supervised_job "$portal_pid" || cleanup_ok=false
      else
        wait "$portal_pid" >/dev/null 2>&1 || true
      fi
    else
      wait "$portal_pid" >/dev/null 2>&1 || true
    fi
    portal_pid=""
  fi

  if [ -n "$emulator_pid" ]; then
    if oxid_job_is_running "$emulator_pid"; then
      if oxid_emulator_job_owned "$emulator_pid" "$BASHPID" "$EMULATOR" "$avd" "$EMULATOR_PORT"; then
        if ! kill -TERM "$emulator_pid" 2>/dev/null; then
          cleanup_ok=false
          emulator_wait=0
        fi
        for ((_attempt = 0; _attempt < 200; _attempt++)); do
          oxid_job_is_running "$emulator_pid" || break
          run_deadline 2 sleep 0.1
        done
        if oxid_job_is_running "$emulator_pid"; then
          if oxid_emulator_job_owned "$emulator_pid" "$BASHPID" "$EMULATOR" "$avd" "$EMULATOR_PORT"; then
            kill -KILL "$emulator_pid" 2>/dev/null || cleanup_ok=false
          else
            cleanup_ok=false
            emulator_wait=0
          fi
        fi
      else
        cleanup_ok=false
        emulator_wait=0
      fi
    fi
    if [ "$emulator_wait" -eq 1 ]; then
      wait "$emulator_pid" >/dev/null 2>&1 || emulator_status=$?
      case "$emulator_status" in 0|137|143) ;; *) cleanup_ok=false ;; esac
      [ "$cleanup_ok" = true ] && emulator_cleanup=true
    fi
    emulator_pid=""
  fi

  if [ "$portal_ready" -eq 1 ]; then
    after_portal="$(listener_fingerprint "${PORTAL_PORTS[@]}")"
    if ! run_deadline 5 rg -q '[[:digit:]]+:p[0-9]+' <<<"$after_portal"; then
      listener_cleanup=true
    else
      cleanup_ok=false
    fi
  fi

  if [ "$build_owned" -eq 1 ]; then
    run_deadline 30 rm -rf -- "$BUILD_SOURCE" >/dev/null 2>&1
    [ ! -e "$BUILD_SOURCE" ] && build_cleanup=true || cleanup_ok=false
  fi
  if [ "$private_state_owned" -eq 1 ]; then
    run_deadline 30 rm -rf -- "$PRIVATE_STATE" >/dev/null 2>&1
    [ ! -e "$PRIVATE_STATE" ] && private_logs_removed=true || cleanup_ok=false
  fi
  if [ "$(run_deadline 10 git -C "$ROOT" rev-parse HEAD 2>/dev/null)" = "$head" ] \
    && [ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    head_clean=true
  else
    cleanup_ok=false
  fi

  write_evidence "$incoming" || cleanup_ok=false
  if [ "$cleanup_ok" != true ]; then
    incoming=1
    printf 'android-portal-exact-sequence-avd: cleanup could not prove owned-state restoration\n' >&2
  fi
  exit "$incoming"
}
[ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
head="$(run_deadline 10 git -C "$ROOT" rev-parse HEAD)"
[[ "$head" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head
run_deadline 20 git -C "$ROOT" verify-commit "$head" >/dev/null 2>&1 || fail oxid-signature
[ -z "$(run_deadline 10 docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] \
  || fail occupied-portal-project
[ ! -e "$PORTAL_STATE" ] && [ ! -L "$PORTAL_STATE" ] || fail occupied-portal-state

for port in "${PORTAL_PORTS[@]}"; do
  [ -z "$(run_deadline 5 lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)" ] || fail occupied-portal-listener
 done
shared_before="$(listener_fingerprint "${SHARED_PORTS[@]}")"
for port in "${SHARED_PORTS[@]}"; do
  run_deadline 5 rg -q "^${port}:p[0-9]+" <<<"$shared_before" || fail shared-listener
 done

existing_avd="$(run_deadline 5 ps -axo pid=,command= | run_deadline 5 awk -v avd="$avd" '
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

trap cleanup EXIT
trap 'failure_phase=signal-int; exit 130' INT
trap 'failure_phase=signal-term; exit 143' TERM
trap 'failure_phase=signal-hup; exit 129' HUP

umask 077
[ ! -e "$PRIVATE_STATE" ] && [ ! -L "$PRIVATE_STATE" ] || fail occupied-private-state
run_deadline 5 mkdir -p "$PRIVATE_STATE" || fail private-state-create
private_state_owned=1
run_deadline 5 chmod 700 "$PRIVATE_STATE" || fail private-state-mode
: >"$PRIVATE_LOG"
run_deadline 5 chmod 600 "$PRIVATE_LOG" || fail private-log-mode

"$EMULATOR" -avd "$avd" -read-only -no-snapshot -no-snapshot-save -port "$EMULATOR_PORT" \
  </dev/null >>"$PRIVATE_LOG" 2>&1 &
emulator_pid=$!
for ((_attempt = 0; _attempt < 50; _attempt++)); do
  oxid_emulator_job_owned "$emulator_pid" "$BASHPID" "$EMULATOR" "$avd" "$EMULATOR_PORT" && break
  run_deadline 2 sleep 0.1
 done
oxid_emulator_job_owned "$emulator_pid" "$BASHPID" "$EMULATOR" "$avd" "$EMULATOR_PORT" \
  || fail emulator-ownership

for ((_attempt = 0; _attempt < 240; _attempt++)); do
  oxid_emulator_job_owned "$emulator_pid" "$BASHPID" "$EMULATOR" "$avd" "$EMULATOR_PORT" \
    || fail emulator-ownership-lost
  if [ "$(adb_device get-state 2>/dev/null || true)" = device ] && \
    [ "$(adb_text shell getprop sys.boot_completed 2>/dev/null)" = 1 ]; then
    emulator_online=1
    break
  fi
  run_deadline 2 sleep 1
 done
[ "$emulator_online" -eq 1 ] || fail emulator-boot
[ "$(adb_text shell getprop ro.kernel.qemu)" = 1 ] || fail qemu
avd_name="$(adb_text emu avd name 2>/dev/null)"
avd_name="${avd_name%%$'\n'*}"
[ "$avd_name" = "$avd" ] || fail avd-identity
[ -z "$(adb_text shell pm path "$PACKAGE" 2>/dev/null)" ] || fail preinstalled-package
reverse_before="$(adb_device reverse --list 2>/dev/null | run_deadline 5 sort)"
for port in "${REVERSE_PORTS[@]}"; do
  if run_deadline 5 awk -v route="tcp:$port" '$2 == route || $3 == route { found=1 } END { exit !found }' <<<"$reverse_before"; then
    fail occupied-reverse
  fi
 done

timeout -k 30s 7200s "$ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" >>"$PRIVATE_LOG" 2>&1 &
portal_pid=$!
oxid_job_is_running "$portal_pid" || fail portal-supervisor
for ((_attempt = 0; _attempt < 4500; _attempt++)); do
  oxid_job_is_running "$portal_pid" || fail portal-exited
  if [ -f "$PORTAL_STATE/ready.json" ] && [ -f "$PORTAL_STATE/control-curl.conf" ] \
    && [ -f "$PORTAL_STATE/portal-offer.capability" ] && [ -f "$PORTAL_STATE/build.env" ]; then break; fi
  run_deadline 2 sleep 0.2
 done
[ -f "$PORTAL_STATE/ready.json" ] && [ -p "$PORTAL_STATE/capability.fifo" ] \
  && [ -f "$PORTAL_STATE/build.env" ] || fail portal-ready
if capability_mode="$(run_deadline 5 stat -c '%a' "$PORTAL_STATE/portal-offer.capability" 2>/dev/null)"; then
  :
else
  capability_mode="$(run_deadline 5 stat -f '%Lp' "$PORTAL_STATE/portal-offer.capability")"
fi
[ "$capability_mode" = 600 ] || fail host-capability-mode
capability_size="$(run_deadline 5 wc -c <"$PORTAL_STATE/portal-offer.capability")"
capability_size="${capability_size// /}"
[ "$capability_size" = 64 ] || fail host-capability-size
manifest_path="$(run_deadline 10 jq -r '.manifestPath // empty' "$PORTAL_STATE/ready.json")"
manifest_sha="$(run_deadline 10 jq -r '.manifestSha256 // empty' "$PORTAL_STATE/ready.json")"
[[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] || fail portal-manifest
run_deadline 5 rg -qF "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH=" "$PORTAL_STATE/build.env" \
  || fail portal-build-env
oxid_job_is_running "$portal_pid" || fail portal-ownership-lost
portal_ready=1

archive="$PRIVATE_STATE/source.tar"
run_deadline 60 git -C "$ROOT" archive --format=tar --output="$archive" "$head" || fail build-source-archive
run_deadline 5 mkdir -p "$BUILD_SOURCE" || fail build-source-create
build_owned=1
run_deadline 60 tar -xf "$archive" -C "$BUILD_SOURCE" || fail build-source-extract
run_deadline 5 rm -f -- "$archive" || fail build-archive-remove
[ ! -e "$BUILD_SOURCE/target" ] || fail isolated-build-output

launcher_mutation_owned=1
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
oxid_job_is_running "$launcher_pid" || fail launcher-supervisor
wait "$launcher_pid" || fail android-launcher
launcher_pid=""

apk="$BUILD_SOURCE/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] && [ ! -L "$apk" ] || fail owned-apk
apk_sha256="$(run_deadline 30 shasum -a 256 "$apk")"
apk_sha256="${apk_sha256%% *}"
[[ "$apk_sha256" =~ ^[0-9a-f]{64}$ ]] || fail apk-digest
[ -n "$(adb_text shell pm path "$PACKAGE" 2>/dev/null)" ] || fail package-install
reverse_after="$(adb_device reverse --list 2>/dev/null | run_deadline 5 sort)"
for port in "${REVERSE_PORTS[@]}"; do
  run_deadline 5 awk -v route="tcp:$port" '$2 == route && $3 == route { found=1 } END { exit !found }' <<<"$reverse_after" \
    || fail reverse-install
 done

capability_absent() {
  adb_device shell "run-as $PACKAGE sh -c 'test ! -e files/portal-offer.capability && test ! -e files/.portal-offer.capability.tmp'" \
    >/dev/null 2>&1
}

wait_capability_absent() {
  for ((_attempt = 0; _attempt < 100; _attempt++)); do
    capability_absent && return 0
    run_deadline 2 sleep 0.1
  done
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
  metadata="$(adb_text shell "run-as $PACKAGE stat -c '%s %a' files/portal-offer.capability" 2>/dev/null)"
  [ "$metadata" = "64 600" ]
}

handoff_state() {
  control_curl "$CONTROL_ORIGIN/handoff-status" | run_deadline 10 jq -r '.state'
}

counter_snapshot() {
  control_curl "$CONTROL_ORIGIN/counters" | run_deadline 10 jq -cS .
}

counter_delta() {
  run_deadline 10 jq -cn --argjson before "$1" --argjson after "$2" '
    $before | with_entries(.value = ($after[.key] - .value))'
}

open_webview() {
  local pid="$1" pages
  websocket_url=""
  remove_forward || return 1
  adb_device forward "tcp:$CDP_PORT" "localabstract:webview_devtools_remote_$pid" >/dev/null || return 1
  forward_active=1
  for ((_attempt = 0; _attempt < 120; _attempt++)); do
    pages="$(run_deadline 5 curl --noproxy '*' --fail --silent --show-error --max-time 2 \
      "http://127.0.0.1:$CDP_PORT/json" 2>/dev/null || true)"
    websocket_url="$(run_deadline 5 jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' \
      <<<"$pages" 2>/dev/null || true)"
    [ -n "$websocket_url" ] && return 0
    run_deadline 2 sleep 0.25
  done
  return 1
}

app_pid() {
  adb_text shell pidof "$PACKAGE" 2>/dev/null
}

run_scenario() {
  local mode="$1" pid control_capability result="$PRIVATE_STATE/scenario-$mode.json"
  pid="$(app_pid)"
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  open_webview "$pid" || return 1
  control_capability="$(run_deadline 10 jq -r '.controlCapability // empty' "$PORTAL_STATE/ready.json")"
  [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "$control_capability" | run_deadline 180 env OXID_PORTAL_CONTROL_ORIGIN="$CONTROL_ORIGIN" \
    node "$ROOT/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" \
    >"$result" 2>>"$PRIVATE_LOG" || return 1
  control_capability=""
  run_deadline 10 jq -e --arg mode "$mode" '.mode == $mode and .passed == true and (.measurements | type == "object")' \
    "$result" >/dev/null || return 1
  if run_deadline 5 rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|serial|\.ts\.net' "$result"; then
    return 1
  fi
  run_deadline 5 chmod 600 "$result" || return 1
  remove_forward
}

journey_status="pre_warm"
cold_handoff_before="$(handoff_state)"
[ "$cold_handoff_before" = ready ] || fail cold-handoff-ready
cold_before="$(counter_snapshot)"
stage_capability_file file "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-stage
run_deadline 5 rm -f -- "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-remove
adb_device shell am force-stop "$PACKAGE" >/dev/null || fail cold-stop
adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" \
  >/dev/null 2>>"$PRIVATE_LOG" || fail cold-intent
run_scenario cold-route || fail cold-route
cold_result=true
session_pid="$(app_pid)"
[[ "$session_pid" =~ ^[1-9][0-9]*$ ]] || fail cold-pid
cold_after="$(counter_snapshot)"
cold_delta="$(counter_delta "$cold_before" "$cold_after")"
run_deadline 10 jq -e 'all(.[]; . == 0)' <<<"$cold_delta" >/dev/null || fail cold-counters
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
warm_intents_attempted=1
adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" \
  >/dev/null 2>>"$PRIVATE_LOG" || fail warm-intent
warm_intents_delivered=1
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
  run_deadline 10 jq -e '.authorizationMetadata == 1 and .issuerMetadata == 1 and .token == 0
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
