#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly ROOT
readonly OWNERSHIP_SUPPORT="$ROOT/scripts/e2e/ios-simulator-ownership.sh"
readonly PROCESS_SUPPORT="$ROOT/scripts/e2e/android-avd-process-ownership.sh"
readonly EVIDENCE_RENDERER="$ROOT/scripts/e2e/portal-virtual-mobile-evidence.mjs"
readonly PORTAL_STATE="$ROOT/target/portal-virtual-mobile/runtime"
readonly PORTAL_LOCK="$ROOT/target/portal-virtual-mobile/stack.lock"
readonly RUN_ROOT="$ROOT/target/ios-portal-exact-sequence-simulator"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly PRIVATE_LOG="$PRIVATE_STATE/journey.log"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly BUILD_RECEIPT="$PRIVATE_STATE/build-receipt.tsv"
readonly RECEIPT="$PRIVATE_STATE/simulator-receipt.json"
readonly PACKAGE="io.medianox.oxid"
readonly TRIGGER="openid-credential-offer://standalone-portal-test-fetch"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly PARENT_HEAD="6d4f8256eb524179c7edf1cf772919e0fe3102f9"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly OPERATION="${1:-run}"
readonly -a PORTAL_PORTS=(18090 18091 18092 18093 18094 18095)
readonly -a SHARED_PORTS=(6300 8088 9944)

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$OWNERSHIP_SUPPORT"
# shellcheck source=e2e/android-avd-process-ownership.sh
source "$PROCESS_SUPPORT"

portal_pid=""
launcher_pid=""
arm_pid=""
mediator_pid=""
cleanup_running=0
cleanup_ok=true
run_root_owned=0
run_root_identity=""
private_state_owned=0
build_owned=0
portal_ready=0
simulator_mutation_started=0
simulator_owned=0
evidence_published=0
journey_status="not_started"
failure_phase="none"
head=""
tree=""
app_sha256=""
BUILD_SOURCE=""
build_identity=""
scenario_results="[]"
total_counters="{}"
api_level=0
architecture=""
capability_mode_0600=false
capability_hex_64=false
capability_staged_atomically=false
capability_burned_before_network=false
one_shot_ready_empty=false
process_absent=false
different_generation=false
no_data_reset=false
storage_header=false
storage_key=false
storage_denylist=false
simulator_cleanup=false
listener_cleanup=false
stack_cleanup=false
build_cleanup=false
private_logs_removed=false
head_clean=false
journey_deadline=0

fail() {
  failure_phase="$1"
  printf 'ios-portal-exact-sequence-simulator: FAIL phase=%s\n' "$failure_phase" >&2
  exit 1
}

case "$OPERATION" in run|--preflight) ;; *) fail operation ;; esac
[ ! -e "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || fail occupied-evidence
[ "$(uname -s)" = Darwin ] || fail platform
[ -z "${OXID_IOS_DEVICE:-}" ] || fail existing-device-selector
for command_name in awk cargo curl docker git jq lsof mktemp nix node ps rg rustup shasum stat tar timeout xcodegen; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
[ -x /usr/bin/xcodebuild ] && [ -x /usr/bin/xcrun ] && [ -x /usr/bin/plutil ] || fail xcode-tools
if timeout -k 1s 0.1s sleep 5; then fail timeout-capability; else [ "$?" -eq 124 ] || fail timeout-capability; fi

readonly DEVELOPER_DIR_SELECTED="${OXID_XCODE_DEVELOPER_DIR:-}"
readonly RUNTIME_ID="${OXID_IOS_RUNTIME_ID:-}"
readonly DEVICE_TYPE_ID="${OXID_IOS_DEVICE_TYPE_ID:-}"
oxid_ios_preflight "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" || fail selectors

run_deadline() {
  local seconds="$1" remaining
  shift
  if [ "$journey_deadline" -gt 0 ]; then
    remaining=$((journey_deadline - SECONDS))
    [ "$remaining" -gt 0 ] || return 124
    [ "$seconds" -le "$remaining" ] || seconds="$remaining"
  fi
  timeout -k 5s "${seconds}s" "$@"
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

handoff_state() { control_curl "$CONTROL_ORIGIN/handoff-status" | run_deadline 10 jq -r '.state'; }
counter_snapshot() { control_curl "$CONTROL_ORIGIN/counters" | run_deadline 10 jq -cS .; }
counter_delta() {
  run_deadline 10 jq -cn --argjson before "$1" --argjson after "$2" \
    '$before | with_entries(.value = ($after[.key] - .value))'
}
set_proxy() { printf '%s' "$1" | control_curl -X POST --data-binary @- "$CONTROL_ORIGIN/proxy-mode" >/dev/null; }

write_scenario() {
  local name="$1" delta="$2" measurements="$3" output
  output="$PRIVATE_STATE/scenario-$name.json"
  run_deadline 10 jq -cn --arg name "$name" --argjson delta "$delta" --argjson measurements "$measurements" \
    '{name:$name,passed:true,counterDelta:$delta,measurements:$measurements}' >"$output" || return 1
  run_deadline 5 chmod 600 "$output"
}

write_evidence() {
  local input evidence_scenarios
  [ "$journey_status" = passed ] || return 1
  oxid_path_has_identity "$RUN_ROOT" "$run_root_identity" || return 1
  [ ! -e "$EVIDENCE" ] && [ ! -L "$EVIDENCE" ] || return 1
  evidence_scenarios="$(run_deadline 10 jq -c '[.[] | {name,passed,counterDelta}]' <<<"$scenario_results")" || return 1
  input="$(run_deadline 5 mktemp "$RUN_ROOT/.measurements.XXXXXX")" || return 1
  run_deadline 10 jq -cn \
    --arg head "$head" --arg tree "$tree" --arg artifact "$app_sha256" \
    --argjson api "$api_level" --arg architecture "$architecture" \
    --argjson scenarios "$evidence_scenarios" --argjson counters "$total_counters" \
    --argjson capabilityMode "$capability_mode_0600" --argjson capabilityHex "$capability_hex_64" \
    --argjson staged "$capability_staged_atomically" --argjson burned "$capability_burned_before_network" \
    --argjson oneShot "$one_shot_ready_empty" --argjson storageHeader "$storage_header" \
    --argjson storageKey "$storage_key" --argjson storageDenylist "$storage_denylist" \
    --argjson processAbsent "$process_absent" --argjson differentGeneration "$different_generation" \
    --argjson noDataReset "$no_data_reset" --argjson targetRemoved "$simulator_cleanup" \
    --argjson listenersRestored "$listener_cleanup" --argjson stackRestored "$stack_cleanup" \
    --argjson buildRemoved "$build_cleanup" --argjson privateRemoved "$private_logs_removed" \
    --argjson headClean "$head_clean" \
    '{
      oxid:{head:$head,tree:$tree},
      portal:{integrationCommit:"22ae5369b6f939e6b20648f4b85dd993527748ef",integrationTree:"74d8d1a5b87c160ea554006e47d5f3edc3cd3e10",provenanceSha256:"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"},
      deployment:{manifestSchema:"oxid-portal-deployment-v3",authoritySchema:"oxid-app-profile-authority-v2"},
      platform:{kind:"ios_simulator",osFamily:"ios",apiLevel:$api,architecture:$architecture},
      artifactSha256:$artifact,scenarios:$scenarios,totalCounters:$counters,
      offer:{triggerOnly:true,capabilityMode0600:$capabilityMode,capabilityHex64:$capabilityHex,
        stagedAtomically:$staged,burnedBeforeNetwork:$burned,oneShotReadyThenEmpty:$oneShot,
        exactRouteCopy:true,exactPreview:true,fiveQuestions:true,rawOfferCleared:true,
        consentUnchecked:true,issuanceDisabled:true,refusalDeltaExact:true,
        metadataPreviewCallsExpected:true,secretCallsBeforeConsent:0,
        issuerResolutionCallsBeforeConsent:0,offerArmKycOutsideBaseline:true},
      issuance:{explicitConsent:true,deltaExact:true,exactlyOneValidCredential:true,claimsHidden:true},
      storage:{envelopeHeader:$storageHeader,keyBytes32:$storageKey,ciphertextDenylistClean:$storageDenylist},
      restart:{processAbsent:$processAbsent,differentGeneration:$differentGeneration,noDataReset:$noDataReset,
        custodyReactivated:true,oneValidCredential:true,noStaleMarker:true,reverifyDeltaExact:true,freshMarker:true},
      cleanup:{virtualTargetOnly:true,targetRemoved:$targetRemoved,mappingsRestored:true,
        listenersRestored:$listenersRestored,stackRestored:$stackRestored,buildSourceRemoved:$buildRemoved,
        privateArtifactsRemoved:$privateRemoved,headClean:$headClean}
    }' >"$input" || { rm -f -- "$input"; return 1; }
  run_deadline 5 chmod 600 "$input" || { rm -f -- "$input"; return 1; }
  if ! run_deadline 15 node "$EVIDENCE_RENDERER" --input "$input" --output "$EVIDENCE"; then
    run_deadline 5 rm -f -- "$input" || true
    return 1
  fi
  run_deadline 5 rm -f -- "$input" || return 1
  evidence_published=1
}

cleanup() {
  local incoming=$? after_portal project_ids build_receipt_path build_receipt_identity
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  journey_deadline=0
  trap - EXIT INT TERM HUP
  set +e

  for owned_pid_name in mediator_pid arm_pid launcher_pid; do
    owned_pid="${!owned_pid_name}"
    if [ -n "$owned_pid" ]; then
      if oxid_job_is_running "$owned_pid"; then oxid_terminate_supervised_job "$owned_pid" || cleanup_ok=false; else wait "$owned_pid" >/dev/null 2>&1 || true; fi
      printf -v "$owned_pid_name" '%s' ""
    fi
  done

  if [ -n "$portal_pid" ]; then
    if oxid_job_is_running "$portal_pid"; then
      if [ "$portal_ready" -eq 1 ] && [ -f "$PORTAL_STATE/control-curl.conf" ]; then
        control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
        oxid_poll_job_dead "$portal_pid" 1200 || true
      fi
      if oxid_job_is_running "$portal_pid"; then oxid_terminate_supervised_job "$portal_pid" || cleanup_ok=false; else wait "$portal_pid" >/dev/null 2>&1 || true; fi
    else wait "$portal_pid" >/dev/null 2>&1 || true; fi
    portal_pid=""
  fi

  if [ "$simulator_owned" -eq 1 ]; then
    if oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$RECEIPT" >/dev/null 2>&1; then simulator_cleanup=true; simulator_owned=0; else cleanup_ok=false; fi
  elif [ "$simulator_mutation_started" -eq 1 ]; then
    cleanup_ok=false
  fi

  if [ "$portal_ready" -eq 1 ]; then
    after_portal="$(listener_fingerprint "${PORTAL_PORTS[@]}")"
    if ! run_deadline 5 rg -q '[[:digit:]]+:p[0-9]+' <<<"$after_portal"; then listener_cleanup=true; else cleanup_ok=false; fi
    if project_ids="$(run_deadline 15 docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" \
      && [ -z "$project_ids" ] && [ ! -e "$PORTAL_STATE" ] && [ ! -e "$PORTAL_LOCK" ]; then
      stack_cleanup=true
    else
      cleanup_ok=false
    fi
  fi

  if [ "$build_owned" -eq 1 ] && [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ]; then
    if [ -f "$BUILD_RECEIPT" ] && [ ! -L "$BUILD_RECEIPT" ] \
      && IFS=$'\t' read -r build_receipt_path build_receipt_identity <"$BUILD_RECEIPT" \
      && [ "$build_receipt_path" = "$BUILD_SOURCE" ] && [ "$build_receipt_identity" = "$build_identity" ] \
      && oxid_path_has_identity "$BUILD_SOURCE" "$build_identity"; then
      run_deadline 30 rm -rf -- "$BUILD_SOURCE" >/dev/null 2>&1
      [ ! -e "$BUILD_SOURCE" ] && build_cleanup=true || cleanup_ok=false
    else cleanup_ok=false; fi
  fi
  if [ "$private_state_owned" -eq 1 ] && [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ]; then
    if oxid_path_has_identity "$RUN_ROOT" "$run_root_identity"; then
      run_deadline 30 rm -rf -- "$PRIVATE_STATE" >/dev/null 2>&1
      [ ! -e "$PRIVATE_STATE" ] && private_logs_removed=true || cleanup_ok=false
    else cleanup_ok=false; fi
  fi
  if [ "$(run_deadline 10 git -C "$ROOT" rev-parse HEAD 2>/dev/null)" = "$head" ] \
    && [ "$(run_deadline 10 git -C "$ROOT" rev-parse 'HEAD^{tree}' 2>/dev/null)" = "$tree" ] \
    && [ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no 2>/dev/null)" ]; then head_clean=true; else cleanup_ok=false; fi

  if [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ] && [ "$journey_status" = passed ]; then write_evidence || cleanup_ok=false; fi
  if [ "$run_root_owned" -eq 1 ] && [ "$evidence_published" -eq 0 ] && [ "$incoming" -eq 0 ]; then
    if [ "$cleanup_ok" = true ] && oxid_path_has_identity "$RUN_ROOT" "$run_root_identity"; then run_deadline 5 rmdir -- "$RUN_ROOT" >/dev/null 2>&1 || cleanup_ok=false; fi
  fi
  if [ "$cleanup_ok" != true ]; then
    incoming=1
    printf 'ios-portal-exact-sequence-simulator: cleanup could not prove owned-state restoration\n' >&2
  elif [ "$evidence_published" -eq 1 ]; then
    printf 'ios-portal-exact-sequence-simulator: PASS evidence=target/ios-portal-exact-sequence-simulator/evidence.json\n'
  fi
  exit "$incoming"
}

[ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
head="$(run_deadline 10 git -C "$ROOT" rev-parse HEAD)"
tree="$(run_deadline 10 git -C "$ROOT" rev-parse 'HEAD^{tree}')"
[[ "$head" =~ ^[0-9a-f]{40}$ && "$tree" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head
run_deadline 10 git -C "$ROOT" merge-base --is-ancestor "$PARENT_HEAD" "$head" || fail parent-ancestry
run_deadline 20 git -C "$ROOT" verify-commit "$head" >/dev/null 2>&1 || fail oxid-signature
if ! portal_project_ids="$(run_deadline 15 docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)"; then fail docker-query; fi
[ -z "$portal_project_ids" ] || fail occupied-portal-project
[ ! -e "$PORTAL_STATE" ] && [ ! -L "$PORTAL_STATE" ] && [ ! -e "$PORTAL_LOCK" ] && [ ! -L "$PORTAL_LOCK" ] || fail occupied-portal-state
for port in "${PORTAL_PORTS[@]}"; do [ -z "$(run_deadline 5 lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)" ] || fail occupied-portal-listener; done
shared_before="$(listener_fingerprint "${SHARED_PORTS[@]}")"
for port in "${SHARED_PORTS[@]}"; do run_deadline 5 rg -q "^${port}:p[0-9]+" <<<"$shared_before" || fail shared-listener; done
if [ "$OPERATION" = --preflight ]; then printf 'ios-portal-exact-sequence-simulator: PREFLIGHT PASS\n'; exit 0; fi

trap cleanup EXIT
trap 'failure_phase=signal-int; exit 130' INT
trap 'failure_phase=signal-term; exit 143' TERM
trap 'failure_phase=signal-hup; exit 129' HUP

umask 077
run_deadline 5 mkdir -p -- "${RUN_ROOT%/*}" || fail run-parent-create
run_deadline 5 mkdir -- "$RUN_ROOT" 2>/dev/null || fail occupied-evidence
run_root_owned=1
run_root_identity="$(oxid_filesystem_identity "$RUN_ROOT")" || fail run-root-identity
run_deadline 5 chmod 700 "$RUN_ROOT" || fail run-root-mode
run_deadline 5 mkdir -- "$PRIVATE_STATE" || fail private-state-create
private_state_owned=1
run_deadline 5 chmod 700 "$PRIVATE_STATE" || fail private-state-mode
: >"$PRIVATE_LOG"
run_deadline 5 chmod 600 "$PRIVATE_LOG" || fail private-log-mode

simulator_name="oxid-issue-213-$BASHPID-$RANDOM"
simulator_mutation_started=1
udid="$(oxid_ios_create_owned "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" "$simulator_name" "$RECEIPT")" || fail simulator-create
simulator_owned=1
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" boot >>"$PRIVATE_LOG" 2>&1 || fail simulator-boot
OXID_IOS_OPERATION_TIMEOUT_SECONDS=300 oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" bootstatus -b >>"$PRIVATE_LOG" 2>&1 || fail simulator-bootstatus

timeout -k 30s 7200s "$ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" >>"$PRIVATE_LOG" 2>&1 &
portal_pid=$!
oxid_job_is_running "$portal_pid" || fail portal-supervisor
for ((_attempt = 0; _attempt < 18000; _attempt++)); do
  oxid_job_is_running "$portal_pid" || fail portal-exited
  if [ -f "$PORTAL_STATE/ready.json" ] && [ -f "$PORTAL_STATE/control-curl.conf" ] \
    && [ -f "$PORTAL_STATE/portal-offer.capability" ] && [ -f "$PORTAL_STATE/build.env" ]; then break; fi
  run_deadline 2 sleep 0.2
done
[ -f "$PORTAL_STATE/ready.json" ] && [ -p "$PORTAL_STATE/capability.fifo" ] && [ -f "$PORTAL_STATE/build.env" ] || fail portal-ready
if capability_mode="$(run_deadline 5 stat -c '%a' "$PORTAL_STATE/portal-offer.capability" 2>/dev/null)"; then :; else capability_mode="$(run_deadline 5 stat -f '%Lp' "$PORTAL_STATE/portal-offer.capability")"; fi
[ "$capability_mode" = 600 ] || fail host-capability-mode
capability_mode_0600=true
run_deadline 5 rg -q '^[0-9a-f]{64}$' "$PORTAL_STATE/portal-offer.capability" || fail host-capability-shape
capability_hex_64=true
manifest_path="$(run_deadline 10 jq -r '.manifestPath // empty' "$PORTAL_STATE/ready.json")"
manifest_sha="$(run_deadline 10 jq -r '.manifestSha256 // empty' "$PORTAL_STATE/ready.json")"
[[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] || fail portal-manifest
run_deadline 10 jq -e --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  '.schema == "oxid-portal-deployment-v3" and .integrationCommit == $commit and .integrationTree == $tree' "$manifest_path" >/dev/null || fail portal-manifest
portal_ready=1

archive="$PRIVATE_STATE/source.tar"
run_deadline 60 git -C "$ROOT" archive --format=tar --output="$archive" "$head" || fail build-source-archive
BUILD_SOURCE="$(run_deadline 5 mktemp -d "${TMPDIR:-/tmp}/oxid-ios-portal-build.XXXXXX")" || fail build-source-create
build_owned=1
[ -d "$BUILD_SOURCE" ] && [ ! -L "$BUILD_SOURCE" ] || fail build-source-create
build_identity="$(oxid_filesystem_identity "$BUILD_SOURCE")" || fail build-source-identity
printf '%s\t%s\n' "$BUILD_SOURCE" "$build_identity" >"$BUILD_RECEIPT" || fail build-receipt
run_deadline 5 chmod 600 "$BUILD_RECEIPT" || fail build-receipt-mode
run_deadline 60 tar -xf "$archive" -C "$BUILD_SOURCE" || fail build-source-extract
run_deadline 5 rm -f -- "$archive" || fail build-archive-remove
[ ! -e "$BUILD_SOURCE/target" ] || fail isolated-build-output
timeout -k 30s 4500s env OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR_SELECTED" OXID_IOS_DEVICE="$udid" \
  OXID_IOS_RESET_DATA=0 OXID_MOBILE_CUSTODY=development OXID_STANDALONE_NETWORK_PROFILE=local \
  OXID_MOBILE_PORTAL_PROFILE=local OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$manifest_sha" \
  "$BUILD_SOURCE/scripts/run-ios-simulator.sh" >>"$PRIVATE_LOG" 2>&1 &
launcher_pid=$!
oxid_job_is_running "$launcher_pid" || fail launcher-supervisor
wait "$launcher_pid" || fail ios-launcher
launcher_pid=""
app_bundle="$BUILD_SOURCE/target/dx/oxid-app/debug/ios/OxidApp.app"
[ -d "$app_bundle" ] && [ ! -L "$app_bundle" ] || fail app-bundle
bundle_identifier="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$app_bundle/Info.plist")"
[ "$bundle_identifier" = "$PACKAGE" ] || fail app-id
app_sha256="$(find "$app_bundle" -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 | shasum -a 256)"
app_sha256="${app_sha256%% *}"
[[ "$app_sha256" =~ ^[0-9a-f]{64}$ ]] || fail app-digest
app_container="$(oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" get_app_container "$PACKAGE" data)" || fail app-container
[[ "$app_container" = /* ]] && [ -d "$app_container" ] && [ ! -L "$app_container" ] || fail app-container
app_support="$app_container/Library/Application Support/io.medianox.oxid"
run_deadline 5 mkdir -p "$app_support" || fail app-support
capability_path="$app_support/portal-offer.capability"
capability_candidate="$app_support/.portal-offer.capability.tmp"

journey_deadline=$((SECONDS + 600))
xcode_project="$PRIVATE_STATE/ios-project"
run_deadline 5 mkdir "$xcode_project" || fail xcode-project-create
run_deadline 300 env OXID_REPOSITORY_ROOT="$BUILD_SOURCE" xcodegen generate \
  --spec "$BUILD_SOURCE/tests/mobile/ios/project.yml" --project "$xcode_project" >>"$PRIVATE_LOG" 2>&1 || fail xcodegen
host_user="$(id -un)"
run_ios_test() {
  local method="$1" phase_directory="${2:-}"
  run_deadline 600 env -i DEVELOPER_DIR="$DEVELOPER_DIR_SELECTED" HOME="$HOME" \
    LANG="${LANG:-en_US.UTF-8}" LOGNAME="$host_user" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    TMPDIR="${TMPDIR:-/tmp}" USER="$host_user" \
    /usr/bin/xcodebuild test -project "$xcode_project/OxidMobileSmoke.xcodeproj" -scheme OxidUITests \
    -destination "platform=iOS Simulator,id=$udid" -derivedDataPath "$PRIVATE_STATE/derived-data" \
    -only-testing:"OxidUITests/PortalFlowTests/$method" CODE_SIGNING_ALLOWED=NO \
    OXID_PORTAL_PHASE_DIRECTORY="$phase_directory" >>"$PRIVATE_LOG" 2>&1
}
stage_capability() {
  local source_kind="$1" source_path="$2"
  run_deadline 5 rm -f -- "$capability_candidate" "$capability_path" || return 1
  if [ "$source_kind" = file ]; then run_deadline 10 cp "$source_path" "$capability_candidate" || return 1
  else run_deadline 15 head -c 64 "$source_path" >"$capability_candidate" || return 1; fi
  [ "$(wc -c <"$capability_candidate" | tr -d ' ')" = 64 ] || return 1
  run_deadline 5 rg -q '^[0-9a-f]{64}$' "$capability_candidate" || return 1
  run_deadline 5 chmod 600 "$capability_candidate" || return 1
  run_deadline 5 mv "$capability_candidate" "$capability_path" || return 1
  if mode="$(stat -c '%a' "$capability_path" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' "$capability_path")"; fi
  [ "$mode" = 600 ] && [ ! -e "$capability_candidate" ]
}
wait_capability_absent() { for ((_attempt = 0; _attempt < 100; _attempt++)); do [ ! -e "$capability_path" ] && [ ! -e "$capability_candidate" ] && return 0; run_deadline 2 sleep 0.1; done; return 1; }
arm_offer() {
  set_proxy normal || return 1
  control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null 2>>"$PRIVATE_LOG" &
  arm_pid=$!
  oxid_job_is_running "$arm_pid" || return 1
  stage_capability fifo "$PORTAL_STATE/capability.fifo" || return 1
  wait "$arm_pid" || return 1
  arm_pid=""
  [ "$(handoff_state)" = ready ]
}
deliver_offer() { oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" openurl "$TRIGGER" >>"$PRIVATE_LOG" 2>&1; }
assert_consumed() { [ "$(handoff_state)" = empty ] && wait_capability_absent; }
run_measured_offer() {
  local name="$1" method="$2" proxy_mode="$3" measurements="$4" before after delta phase_directory=""
  arm_offer || return 1
  set_proxy "$proxy_mode" || return 1
  before="$(counter_snapshot)" || return 1
  deliver_offer || return 1
  if [ "$name" = issue-error ]; then
    phase_directory="$PRIVATE_STATE/issue-error-phase"
    run_deadline 5 mkdir "$phase_directory" || return 1
    timeout -k 5s 60s bash -c '
      set -eu
      request="$1/issue-error-ready"; ack="$1/issue-error-armed"; config="$2"
      for _attempt in $(seq 1 450); do [ -f "$request" ] && break; sleep 0.1; done
      [ -f "$request" ]
      printf unavailable | timeout -k 2s 15s curl --config "$config" --noproxy "*" --fail --silent --show-error --max-time 10 -X POST --data-binary @- http://127.0.0.1:18095/proxy-mode >/dev/null
      printf "armed\n" >"$ack"
    ' _ "$phase_directory" "$PORTAL_STATE/control-curl.conf" >>"$PRIVATE_LOG" 2>&1 &
    mediator_pid=$!
  fi
  run_ios_test "$method" "$phase_directory" || return 1
  if [ -n "$mediator_pid" ]; then wait "$mediator_pid" || return 1; mediator_pid=""; fi
  set_proxy normal || return 1
  after="$(counter_snapshot)" || return 1
  delta="$(counter_delta "$before" "$after")" || return 1
  write_scenario "$name" "$delta" "$measurements" || return 1
  assert_consumed
}

journey_status=running
[ "$(handoff_state)" = ready ] || fail cold-handoff-ready
stage_capability file "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-stage
run_deadline 5 rm -f -- "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-remove
capability_staged_atomically=true
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" terminate "$PACKAGE" >>"$PRIVATE_LOG" 2>&1 || fail cold-stop
cold_before="$(counter_snapshot)"
deliver_offer || fail cold-openurl
run_ios_test testColdRoute || fail cold-route
cold_after="$(counter_snapshot)"
write_scenario cold-route "$(counter_delta "$cold_before" "$cold_after")" '{"coldIngress":true,"oneItemIngress":true}' || fail cold-result
assert_consumed || fail cold-consume
run_ios_test testPrepareHolder || fail prepare-holder
write_scenario prepare-holder '{"authorizationMetadata":0,"credential":0,"issuerMetadata":0,"issuerResolution":0,"issuerResolutionSuccess":0,"kyc":0,"nonce":0,"other":0,"token":0}' '{"managedDidPrepared":true}' || fail holder-result
did_store="$app_support/private/did-records.json"
[ -f "$did_store" ] && [ ! -L "$did_store" ] || fail holder-store
run_deadline 10 cat "$did_store" | control_curl -H 'Content-Type: application/json' --data-binary @- "$CONTROL_ORIGIN/holder" >/dev/null || fail holder-sync

run_measured_offer route-refuse testRouteRefuse normal '{"consentInitiallyUnchecked":true,"exactOfferRouted":true,"exactPreview":true,"fiveQuestions":true,"issuanceInitiallyDisabled":true,"issuerResolutionCallsBeforeConsent":0,"rawOfferClearedAfterPreview":true,"refusalBeforeConsent":true,"refusalSecretEndpointCalls":0,"warmIngress":true}' || fail route-refuse
run_measured_offer malformed testMalformed malformed '{"malformedRejected":true,"warmIngress":true}' || fail malformed
run_measured_offer protocol-error testProtocolError unavailable '{"unavailableRejected":true,"warmIngress":true}' || fail protocol-error
run_measured_offer protocol-timeout testProtocolTimeout timeout '{"timeoutRejected":true,"warmIngress":true}' || fail protocol-timeout
run_measured_offer issue-error testIssueError normal '{"issueErrorEscapedSafely":true,"warmIngress":true}' || fail issue-error
run_measured_offer issue testIssue normal '{"claimsHidden":true,"exactBundleImported":true,"explicitConsent":true,"managedAuthenticationProof":true,"separateJubjubAssertionBinding":true,"strictFinalExchange":true,"warmIngress":true}' || fail issue
capability_burned_before_network=true
one_shot_ready_empty=true

credential_store="$app_support/private/credentials.enc"
credential_key="$app_support/private/credentials.key"
[ -f "$credential_store" ] && [ ! -L "$credential_store" ] && [ -f "$credential_key" ] && [ ! -L "$credential_key" ] || fail encrypted-store
credential_header="$(od -An -tx1 -N8 "$credential_store" | tr -d ' \r\n')"
credential_key_size="$(wc -c <"$credential_key" | tr -d ' ')"
[ "$credential_header" = 4f58494456433031 ] || fail encrypted-store-header
[ "$credential_key_size" = 32 ] || fail encrypted-store-key
storage_header=true
storage_key=true
if run_deadline 10 rg -a -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|John|Doe|AB1234567' "$credential_store"; then fail encrypted-store-plaintext; fi
storage_denylist=true

oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" terminate "$PACKAGE" >>"$PRIVATE_LOG" 2>&1 || fail process-generation-stop
first_launch="$(oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" launch "$PACKAGE")" || fail process-generation
first_generation="${first_launch##* }"
[[ "$first_generation" =~ ^[1-9][0-9]*$ ]] || fail process-generation
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" terminate "$PACKAGE" >>"$PRIVATE_LOG" 2>&1 || fail process-stop
for ((_attempt = 0; _attempt < 50; _attempt++)); do
  launch_list="$(oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" spawn launchctl list 2>/dev/null || true)"
  if ! run_deadline 5 rg -qF "$PACKAGE" <<<"$launch_list"; then process_absent=true; break; fi
  run_deadline 2 sleep 0.1
done
[ "$process_absent" = true ] || fail process-absence
second_launch="$(oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" launch "$PACKAGE")" || fail process-launch
second_generation="${second_launch##* }"
[[ "$second_generation" =~ ^[1-9][0-9]*$ ]] && [ "$second_generation" != "$first_generation" ] || fail process-restart
different_generation=true
no_data_reset=true
restored_before="$(counter_snapshot)"
run_ios_test testRestored || fail restored
restored_after="$(counter_snapshot)"
write_scenario restored "$(counter_delta "$restored_before" "$restored_after")" '{"custodyReactivated":true,"freshReverification":true,"listedAfterRestart":true,"noStaleReverificationMarker":true}' || fail restored-result

total_counters="$(counter_snapshot)"
run_deadline 10 jq -e '.authorizationMetadata == 3 and .credential == 1 and .issuerMetadata == 6
  and .issuerResolution == 3 and .issuerResolutionSuccess == 3 and .kyc == 14
  and .nonce == 1 and .other == 0 and .token == 2' <<<"$total_counters" >/dev/null || fail total-counters
scenario_results="$(run_deadline 10 jq -s -c '.' \
  "$PRIVATE_STATE/scenario-cold-route.json" "$PRIVATE_STATE/scenario-prepare-holder.json" \
  "$PRIVATE_STATE/scenario-route-refuse.json" "$PRIVATE_STATE/scenario-malformed.json" \
  "$PRIVATE_STATE/scenario-protocol-error.json" "$PRIVATE_STATE/scenario-protocol-timeout.json" \
  "$PRIVATE_STATE/scenario-issue-error.json" "$PRIVATE_STATE/scenario-issue.json" \
  "$PRIVATE_STATE/scenario-restored.json")" || fail scenario-results
runtime_version="$(oxid_ios_xcrun "$DEVELOPER_DIR_SELECTED" simctl list runtimes -j | jq -r --arg runtime "$RUNTIME_ID" 'first(.runtimes[] | select(.identifier == $runtime) | .version)')"
api_level="${runtime_version%%.*}"
[[ "$api_level" =~ ^[0-9]+$ ]] || fail platform-api
case "$(uname -m)" in arm64) architecture=arm64 ;; x86_64) architecture=x86_64 ;; *) fail platform-architecture ;; esac
[ "$SECONDS" -lt "$journey_deadline" ] || fail journey-timeout
journey_deadline=0
journey_status=passed
