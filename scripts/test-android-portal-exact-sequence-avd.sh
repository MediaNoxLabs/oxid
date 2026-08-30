#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly ROOT
readonly PROCESS_SUPPORT="$ROOT/scripts/e2e/android-avd-process-ownership.sh"
readonly EVIDENCE_RENDERER="$ROOT/scripts/e2e/portal-virtual-mobile-evidence.mjs"
readonly PORTAL_STATE="$ROOT/target/portal-virtual-mobile/runtime"
readonly PORTAL_LOCK="$ROOT/target/portal-virtual-mobile/stack.lock"
readonly RUN_ROOT="$ROOT/target/android-portal-exact-sequence-avd"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly PRIVATE_LOG="$PRIVATE_STATE/journey.log"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly BUILD_RECEIPT="$PRIVATE_STATE/build-receipt.tsv"
readonly PACKAGE="io.medianox.oxid"
readonly TRIGGER="openid-credential-offer://standalone-portal-test-fetch"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly PARENT_HEAD="6d4f8256eb524179c7edf1cf772919e0fe3102f9"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly EMULATOR_PORT=5562
readonly SERIAL="emulator-$EMULATOR_PORT"
readonly CDP_PORT=19247
readonly OPERATION="${1:-run}"
readonly -a REVERSE_PORTS=(6300 8088 9944 18090 18091 18093)
readonly -a PORTAL_PORTS=(18090 18091 18092 18093 18094 18095)
readonly -a SHARED_PORTS=(6300 8088 9944)

# shellcheck source=e2e/android-avd-process-ownership.sh
source "$PROCESS_SUPPORT"

portal_pid=""
emulator_pid=""
launcher_pid=""
arm_pid=""
forward_active=0
emulator_online=0
cleanup_running=0
cleanup_ok=true
run_root_owned=0
run_root_identity=""
private_state_owned=0
evidence_published=0
portal_ready=0
build_owned=0
launcher_mutation_owned=0
journey_status="not_started"
failure_phase="none"
head=""
tree=""
apk_sha256=""
BUILD_SOURCE=""
build_identity=""
reverse_before=""
shared_before=""
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
emulator_cleanup=false
reverse_cleanup=false
forward_cleanup=false
listener_cleanup=false
stack_cleanup=false
build_cleanup=false
private_logs_removed=false
head_clean=false
journey_deadline=0

fail() {
  failure_phase="$1"
  printf 'android-portal-exact-sequence-avd: FAIL phase=%s\n' "$failure_phase" >&2
  exit 1
}

case "$OPERATION" in
  run|--preflight) ;;
  *) fail operation ;;
esac

# Occupied evidence is owner state. Reject it before probing tools or devices.
[ ! -e "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || fail occupied-evidence

for command_name in awk cargo curl docker git jq lsof mktemp nix node ps rg rustup sed shasum stat tar timeout; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
if timeout -k 1s 0.1s sleep 5; then fail timeout-capability; else [ "$?" -eq 124 ] || fail timeout-capability; fi

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ -d "$HOME/Library/Android/sdk" ]; then android_sdk="$HOME/Library/Android/sdk"; fi
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
  local seconds="$1" remaining
  shift
  if [ "$journey_deadline" -gt 0 ]; then
    remaining=$((journey_deadline - SECONDS))
    [ "$remaining" -gt 0 ] || return 124
    [ "$seconds" -le "$remaining" ] || seconds="$remaining"
  fi
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
  local input evidence_scenarios exact_offer exact_preview consent_unchecked issuance_disabled refusal_secret refusal_resolution
  local explicit_consent exactly_one claims_hidden custody_reactivated listed fresh_reverification no_stale_marker
  [ "$journey_status" = passed ] || return 1
  oxid_path_has_identity "$RUN_ROOT" "$run_root_identity" || return 1
  [ ! -e "$EVIDENCE" ] && [ ! -L "$EVIDENCE" ] || return 1

  exact_offer="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.exactOfferRouted) // false' <<<"$scenario_results")"
  exact_preview="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.exactPreview) // false' <<<"$scenario_results")"
  consent_unchecked="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.consentInitiallyUnchecked) // false' <<<"$scenario_results")"
  issuance_disabled="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.issuanceInitiallyDisabled) // false' <<<"$scenario_results")"
  refusal_secret="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.refusalSecretEndpointCalls) // -1' <<<"$scenario_results")"
  refusal_resolution="$(jq -r 'first(.[] | select(.name == "route-refuse") | .measurements.issuerResolutionCallsBeforeConsent) // -1' <<<"$scenario_results")"
  explicit_consent="$(jq -r 'first(.[] | select(.name == "issue") | .measurements.explicitConsent) // false' <<<"$scenario_results")"
  exactly_one="$(jq -r 'first(.[] | select(.name == "issue") | .measurements.exactBundleImported) // false' <<<"$scenario_results")"
  claims_hidden="$(jq -r 'first(.[] | select(.name == "issue") | .measurements.claimsHidden) // false' <<<"$scenario_results")"
  custody_reactivated="$(jq -r 'first(.[] | select(.name == "restored") | .measurements.custodyReactivated) // false' <<<"$scenario_results")"
  listed="$(jq -r 'first(.[] | select(.name == "restored") | .measurements.listedAfterRestart) // false' <<<"$scenario_results")"
  fresh_reverification="$(jq -r 'first(.[] | select(.name == "restored") | .measurements.freshReverification) // false' <<<"$scenario_results")"
  no_stale_marker="$(jq -r 'first(.[] | select(.name == "restored") | .measurements.noStaleReverificationMarker) // false' <<<"$scenario_results")"
  evidence_scenarios="$(run_deadline 10 jq -c '[.[] | {name,passed,counterDelta}]' <<<"$scenario_results")" || return 1

  input="$(run_deadline 5 mktemp "$RUN_ROOT/.measurements.XXXXXX")" || return 1
  run_deadline 10 jq -cn \
    --arg head "$head" --arg tree "$tree" --arg artifact "$apk_sha256" \
    --argjson api "$api_level" --arg architecture "$architecture" \
    --argjson scenarios "$evidence_scenarios" --argjson counters "$total_counters" \
    --argjson capabilityMode "$capability_mode_0600" --argjson capabilityHex "$capability_hex_64" \
    --argjson staged "$capability_staged_atomically" --argjson burned "$capability_burned_before_network" \
    --argjson oneShot "$one_shot_ready_empty" --argjson exactOffer "$exact_offer" \
    --argjson exactPreview "$exact_preview" --argjson consentUnchecked "$consent_unchecked" \
    --argjson issuanceDisabled "$issuance_disabled" --argjson refusalSecret "$refusal_secret" \
    --argjson refusalResolution "$refusal_resolution" --argjson explicitConsent "$explicit_consent" \
    --argjson exactlyOne "$exactly_one" --argjson claimsHidden "$claims_hidden" \
    --argjson storageHeader "$storage_header" --argjson storageKey "$storage_key" \
    --argjson storageDenylist "$storage_denylist" --argjson processAbsent "$process_absent" \
    --argjson differentGeneration "$different_generation" --argjson noDataReset "$no_data_reset" \
    --argjson custodyReactivated "$custody_reactivated" --argjson listed "$listed" \
    --argjson noStaleMarker "$no_stale_marker" --argjson freshReverification "$fresh_reverification" \
    --argjson targetRemoved "$emulator_cleanup" --argjson mappingsRestored "$reverse_cleanup" \
    --argjson listenersRestored "$listener_cleanup" --argjson stackRestored "$stack_cleanup" \
    --argjson buildRemoved "$build_cleanup" --argjson privateRemoved "$private_logs_removed" \
    --argjson headClean "$head_clean" \
    '{
      oxid:{head:$head,tree:$tree},
      portal:{integrationCommit:"22ae5369b6f939e6b20648f4b85dd993527748ef",integrationTree:"74d8d1a5b87c160ea554006e47d5f3edc3cd3e10",provenanceSha256:"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"},
      deployment:{manifestSchema:"oxid-portal-deployment-v3",authoritySchema:"oxid-app-profile-authority-v2"},
      platform:{kind:"android_emulator",osFamily:"android",apiLevel:$api,architecture:$architecture},
      artifactSha256:$artifact,scenarios:$scenarios,totalCounters:$counters,
      offer:{triggerOnly:true,capabilityMode0600:$capabilityMode,capabilityHex64:$capabilityHex,
        stagedAtomically:$staged,burnedBeforeNetwork:$burned,oneShotReadyThenEmpty:$oneShot,
        exactRouteCopy:$exactOffer,exactPreview:$exactPreview,fiveQuestions:$exactPreview,
        rawOfferCleared:$exactPreview,consentUnchecked:$consentUnchecked,issuanceDisabled:$issuanceDisabled,
        refusalDeltaExact:true,metadataPreviewCallsExpected:true,secretCallsBeforeConsent:$refusalSecret,
        issuerResolutionCallsBeforeConsent:$refusalResolution,offerArmKycOutsideBaseline:true},
      issuance:{explicitConsent:$explicitConsent,deltaExact:true,exactlyOneValidCredential:$exactlyOne,claimsHidden:$claimsHidden},
      storage:{envelopeHeader:$storageHeader,keyBytes32:$storageKey,ciphertextDenylistClean:$storageDenylist},
      restart:{processAbsent:$processAbsent,differentGeneration:$differentGeneration,noDataReset:$noDataReset,
        custodyReactivated:$custodyReactivated,oneValidCredential:$listed,noStaleMarker:$noStaleMarker,
        reverifyDeltaExact:$freshReverification,freshMarker:$freshReverification},
      cleanup:{virtualTargetOnly:true,targetRemoved:$targetRemoved,mappingsRestored:$mappingsRestored,
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
  local incoming=$? current package_path after_portal project_ids emulator_status=0
  local build_receipt_path build_receipt_identity
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  journey_deadline=0
  trap - EXIT INT TERM HUP
  set +e

  if [ -n "$arm_pid" ]; then
    if oxid_job_is_running "$arm_pid"; then oxid_terminate_supervised_job "$arm_pid" || cleanup_ok=false; else wait "$arm_pid" >/dev/null 2>&1 || true; fi
    arm_pid=""
  fi
  if [ -n "$launcher_pid" ]; then
    if oxid_job_is_running "$launcher_pid"; then oxid_terminate_supervised_job "$launcher_pid" || cleanup_ok=false; else wait "$launcher_pid" >/dev/null 2>&1 || true; fi
    launcher_pid=""
  fi

  if [ "$emulator_online" -eq 1 ]; then
    if remove_forward; then forward_cleanup=true; else cleanup_ok=false; fi
    if [ "$launcher_mutation_owned" -eq 1 ]; then
      package_path="$(cleanup_adb shell pm path "$PACKAGE" 2>/dev/null || true)"
      package_path="${package_path//$'\r'/}"
      if [ -n "$package_path" ]; then
        cleanup_adb shell "run-as $PACKAGE sh -c 'rm -f files/portal-offer.capability files/.portal-offer.capability.tmp'" >/dev/null 2>&1 || cleanup_ok=false
        cleanup_adb uninstall "$PACKAGE" >/dev/null 2>&1 || cleanup_ok=false
      fi
      for port in "${REVERSE_PORTS[@]}"; do cleanup_adb reverse --remove "tcp:$port" >/dev/null 2>&1 || true; done
      current="$(cleanup_adb reverse --list 2>/dev/null | run_deadline 5 sort || true)"
      if [ "$forward_cleanup" = true ] && [ "$current" = "$reverse_before" ]; then
        reverse_cleanup=true
      else
        cleanup_ok=false
      fi
    fi
  fi

  if [ -n "$portal_pid" ]; then
    if oxid_job_is_running "$portal_pid"; then
      if [ "$portal_ready" -eq 1 ] && [ -f "$PORTAL_STATE/control-curl.conf" ]; then
        control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
        oxid_poll_job_dead "$portal_pid" 1200 || true
      fi
      if oxid_job_is_running "$portal_pid"; then oxid_terminate_supervised_job "$portal_pid" || cleanup_ok=false; else wait "$portal_pid" >/dev/null 2>&1 || true; fi
    else
      wait "$portal_pid" >/dev/null 2>&1 || true
    fi
    portal_pid=""
  fi

  if [ -n "$emulator_pid" ]; then
    if oxid_job_is_running "$emulator_pid"; then
      if oxid_terminate_emulator_job "$emulator_pid" "$$" "$EMULATOR" "$avd" "$EMULATOR_PORT"; then emulator_cleanup=true; else cleanup_ok=false; fi
    else
      wait "$emulator_pid" >/dev/null 2>&1 || emulator_status=$?
      case "$emulator_status" in 0|137|143) emulator_cleanup=true ;; *) cleanup_ok=false ;; esac
    fi
    emulator_pid=""
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
    && [ -z "$(run_deadline 10 git -C "$ROOT" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    head_clean=true
  else cleanup_ok=false; fi

  if [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ] && [ "$journey_status" = passed ]; then
    write_evidence || cleanup_ok=false
  fi
  if [ "$run_root_owned" -eq 1 ] && [ "$evidence_published" -eq 0 ] && [ "$incoming" -eq 0 ]; then
    if [ "$cleanup_ok" = true ] && oxid_path_has_identity "$RUN_ROOT" "$run_root_identity"; then
      run_deadline 5 rmdir -- "$RUN_ROOT" >/dev/null 2>&1 || cleanup_ok=false
    fi
  fi
  if [ "$cleanup_ok" != true ]; then
    incoming=1
    printf 'android-portal-exact-sequence-avd: cleanup could not prove owned-state restoration\n' >&2
  elif [ "$evidence_published" -eq 1 ]; then
    printf 'android-portal-exact-sequence-avd: PASS evidence=target/android-portal-exact-sequence-avd/evidence.json\n'
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
existing_avd="$(run_deadline 5 ps -axo pid=,command= | run_deadline 5 awk -v avd="$avd" '{ for (i = 2; i <= NF; i++) if ($(i - 1) == "-avd" && $i == avd) print $1 }')"
[ -z "$existing_avd" ] || fail avd-in-use
oxid_require_empty_adb_inventory "$ADB" || fail adb-inventory
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$EMULATOR_PORT" -sTCP:LISTEN 2>/dev/null || true)" ] || fail console-port-in-use
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$((EMULATOR_PORT + 1))" -sTCP:LISTEN 2>/dev/null || true)" ] || fail adb-port-in-use
[ -z "$(run_deadline 5 lsof -nP -iTCP:"$CDP_PORT" -sTCP:LISTEN 2>/dev/null || true)" ] || fail cdp-port-in-use
if [ "$OPERATION" = --preflight ]; then
  printf 'android-portal-exact-sequence-avd: PREFLIGHT PASS\n'
  exit 0
fi

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

"$EMULATOR" -avd "$avd" -read-only -no-snapshot -no-snapshot-save -port "$EMULATOR_PORT" </dev/null >>"$PRIVATE_LOG" 2>&1 &
emulator_pid=$!
for ((_attempt = 0; _attempt < 50; _attempt++)); do
  oxid_emulator_job_owned "$emulator_pid" "$$" "$EMULATOR" "$avd" "$EMULATOR_PORT" && break
  run_deadline 2 sleep 0.1
done
oxid_emulator_job_owned "$emulator_pid" "$$" "$EMULATOR" "$avd" "$EMULATOR_PORT" || fail emulator-ownership
for ((_attempt = 0; _attempt < 300; _attempt++)); do
  oxid_emulator_job_owned "$emulator_pid" "$$" "$EMULATOR" "$avd" "$EMULATOR_PORT" || fail emulator-ownership-lost
  inventory="$(oxid_adb_inventory_snapshot "$ADB" 2>/dev/null || true)"
  if oxid_adb_inventory_is_exact_online "$inventory" "$SERIAL" \
    && [ "$(adb_text shell getprop sys.boot_completed 2>/dev/null)" = 1 ]; then emulator_online=1; break; fi
  run_deadline 2 sleep 1
done
[ "$emulator_online" -eq 1 ] || fail emulator-boot
inventory="$(oxid_adb_inventory_snapshot "$ADB")" || fail adb-inventory-post-boot
oxid_adb_inventory_is_exact_online "$inventory" "$SERIAL" || fail adb-inventory-post-boot
[ "$(adb_text shell getprop ro.kernel.qemu)" = 1 ] || fail qemu
avd_name="$(adb_text emu avd name 2>/dev/null)"; avd_name="${avd_name%%$'\n'*}"
[ "$avd_name" = "$avd" ] || fail avd-identity
[ -z "$(adb_text shell pm path "$PACKAGE" 2>/dev/null)" ] || fail preinstalled-package
reverse_before="$(adb_device reverse --list 2>/dev/null | run_deadline 5 sort)"
for port in "${REVERSE_PORTS[@]}"; do
  if run_deadline 5 awk -v route="tcp:$port" '$2 == route || $3 == route { found=1 } END { exit !found }' <<<"$reverse_before"; then fail occupied-reverse; fi
done

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
[ "$(run_deadline 5 wc -c <"$PORTAL_STATE/portal-offer.capability" | tr -d ' ')" = 64 ] || fail host-capability-size
capability_hex_64=true
manifest_path="$(run_deadline 10 jq -r '.manifestPath // empty' "$PORTAL_STATE/ready.json")"
manifest_sha="$(run_deadline 10 jq -r '.manifestSha256 // empty' "$PORTAL_STATE/ready.json")"
[[ "$manifest_path" = /* && "$manifest_sha" =~ ^[0-9a-f]{64}$ ]] || fail portal-manifest
run_deadline 10 jq -e --arg commit "$PORTAL_COMMIT" --arg tree "$PORTAL_TREE" \
  '.schema == "oxid-portal-deployment-v3" and .integrationCommit == $commit and .integrationTree == $tree' \
  "$manifest_path" >/dev/null || fail portal-manifest
portal_ready=1

archive="$PRIVATE_STATE/source.tar"
run_deadline 60 git -C "$ROOT" archive --format=tar --output="$archive" "$head" || fail build-source-archive
BUILD_SOURCE="$(run_deadline 5 mktemp -d "${TMPDIR:-/tmp}/oxid-android-portal-build.XXXXXX")" || fail build-source-create
build_owned=1
[ -d "$BUILD_SOURCE" ] && [ ! -L "$BUILD_SOURCE" ] || fail build-source-create
build_identity="$(oxid_filesystem_identity "$BUILD_SOURCE")" || fail build-source-identity
printf '%s\t%s\n' "$BUILD_SOURCE" "$build_identity" >"$BUILD_RECEIPT" || fail build-receipt
run_deadline 5 chmod 600 "$BUILD_RECEIPT" || fail build-receipt-mode
run_deadline 60 tar -xf "$archive" -C "$BUILD_SOURCE" || fail build-source-extract
run_deadline 5 rm -f -- "$archive" || fail build-archive-remove
[ ! -e "$BUILD_SOURCE/target" ] || fail isolated-build-output
launcher_mutation_owned=1
timeout -k 30s 4500s env OXID_ANDROID_DEVICE="$SERIAL" OXID_ANDROID_AVD="$avd" \
  OXID_ANDROID_ADB_TIMEOUT_SECONDS=45 OXID_MOBILE_CUSTODY=development \
  OXID_STANDALONE_NETWORK_PROFILE=local OXID_MOBILE_PORTAL_PROFILE=local \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$manifest_sha" \
  "$BUILD_SOURCE/scripts/run-android-emulator.sh" >>"$PRIVATE_LOG" 2>&1 &
launcher_pid=$!
oxid_job_is_running "$launcher_pid" || fail launcher-supervisor
wait "$launcher_pid" || fail android-launcher
launcher_pid=""
apk="$BUILD_SOURCE/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] && [ ! -L "$apk" ] || fail owned-apk
apk_sha256="$(run_deadline 30 shasum -a 256 "$apk")"; apk_sha256="${apk_sha256%% *}"
[[ "$apk_sha256" =~ ^[0-9a-f]{64}$ ]] || fail apk-digest
[ -n "$(adb_text shell pm path "$PACKAGE" 2>/dev/null)" ] || fail package-install
reverse_after="$(adb_device reverse --list 2>/dev/null | run_deadline 5 sort)"
for port in "${REVERSE_PORTS[@]}"; do run_deadline 5 awk -v route="tcp:$port" '$2 == route && $3 == route { found=1 } END { exit !found }' <<<"$reverse_after" || fail reverse-install; done

capability_absent() {
  adb_device shell "run-as $PACKAGE sh -c 'test ! -e files/portal-offer.capability && test ! -e files/.portal-offer.capability.tmp'" >/dev/null 2>&1
}
wait_capability_absent() { for ((_attempt = 0; _attempt < 100; _attempt++)); do capability_absent && return 0; run_deadline 2 sleep 0.1; done; return 1; }
stage_capability_file() {
  local source_kind="$1" source_path="$2" metadata
  local stage="run-as $PACKAGE sh -c 'umask 077; target=files/portal-offer.capability; candidate=files/.portal-offer.capability.tmp; rm -f \"\$candidate\" \"\$target\"; cat >\"\$candidate\"; test \"\$(wc -c <\"\$candidate\")\" -eq 64; grep -Eq \"^[0-9a-f]{64}\$\" \"\$candidate\"; chmod 600 \"\$candidate\"; mv \"\$candidate\" \"\$target\"'"
  if [ "$source_kind" = file ]; then run_deadline 10 cat "$source_path" | adb_device shell "$stage" >>"$PRIVATE_LOG" 2>&1
  else run_deadline 15 head -c 64 "$source_path" | adb_device shell "$stage" >>"$PRIVATE_LOG" 2>&1; fi
  metadata="$(adb_text shell "run-as $PACKAGE stat -c '%s %a' files/portal-offer.capability" 2>/dev/null)"
  [ "$metadata" = "64 600" ] && adb_device shell "run-as $PACKAGE test ! -e files/.portal-offer.capability.tmp" >/dev/null 2>&1
}
handoff_state() { control_curl "$CONTROL_ORIGIN/handoff-status" | run_deadline 10 jq -r '.state'; }
counter_snapshot() { control_curl "$CONTROL_ORIGIN/counters" | run_deadline 10 jq -cS .; }
open_webview() {
  local pid="$1" pages
  websocket_url=""
  remove_forward || return 1
  adb_device forward "tcp:$CDP_PORT" "localabstract:webview_devtools_remote_$pid" >/dev/null || return 1
  forward_active=1
  for ((_attempt = 0; _attempt < 120; _attempt++)); do
    pages="$(run_deadline 5 curl --noproxy '*' --fail --silent --show-error --max-time 2 "http://127.0.0.1:$CDP_PORT/json" 2>/dev/null || true)"
    websocket_url="$(run_deadline 5 jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' <<<"$pages" 2>/dev/null || true)"
    [ -n "$websocket_url" ] && return 0
    run_deadline 2 sleep 0.25
  done
  return 1
}
app_pid() { adb_text shell pidof "$PACKAGE" 2>/dev/null; }
run_scenario() {
  local mode="$1" pid control_capability result
  result="$PRIVATE_STATE/scenario-$mode.json"
  pid="$(app_pid)"; [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  open_webview "$pid" || return 1
  control_capability="$(run_deadline 10 jq -r '.controlCapability // empty' "$PORTAL_STATE/ready.json")"
  [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "$control_capability" | run_deadline 180 env OXID_PORTAL_CONTROL_ORIGIN="$CONTROL_ORIGIN" \
    node "$ROOT/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" >"$result" 2>>"$PRIVATE_LOG" || return 1
  control_capability=""
  run_deadline 10 jq -e --arg mode "$mode" '.mode == $mode and .passed == true and (.measurements | type == "object") and (.counterDelta | type == "object")' "$result" >/dev/null || return 1
  if run_deadline 5 rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|serial|\.ts\.net' "$result"; then return 1; fi
  run_deadline 5 chmod 600 "$result" || return 1
  remove_forward
}
set_proxy_normal() { printf 'normal' | control_curl -X POST --data-binary @- "$CONTROL_ORIGIN/proxy-mode" >/dev/null; }
arm_offer() {
  set_proxy_normal || return 1
  control_curl -X POST "$CONTROL_ORIGIN/arm-android-offer" >/dev/null 2>>"$PRIVATE_LOG" &
  arm_pid=$!
  oxid_job_is_running "$arm_pid" || return 1
  stage_capability_file fifo "$PORTAL_STATE/capability.fifo" || return 1
  wait "$arm_pid" || return 1
  arm_pid=""
  [ "$(handoff_state)" = ready ] || return 1
}
deliver_warm_offer() {
  arm_offer || return 1
  adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" >/dev/null 2>>"$PRIVATE_LOG" || return 1
}
assert_consumed() { [ "$(handoff_state)" = empty ] && wait_capability_absent; }

journey_status="running"
journey_deadline=$((SECONDS + 600))
[ "$(handoff_state)" = ready ] || fail cold-handoff-ready
stage_capability_file file "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-stage
run_deadline 5 rm -f -- "$PORTAL_STATE/portal-offer.capability" || fail cold-capability-remove
capability_staged_atomically=true
adb_device shell am force-stop "$PACKAGE" >/dev/null || fail cold-stop
adb_device shell am start -W -a android.intent.action.VIEW -d "$TRIGGER" "$PACKAGE" >/dev/null 2>>"$PRIVATE_LOG" || fail cold-intent
run_scenario cold-route || fail cold-route
assert_consumed || fail cold-consume
session_pid="$(app_pid)"; [[ "$session_pid" =~ ^[1-9][0-9]*$ ]] || fail cold-pid
run_scenario prepare-holder || fail prepare-holder
[ "$(app_pid)" = "$session_pid" ] || fail holder-process
adb_device exec-out run-as "$PACKAGE" cat files/oxid/private/did-records.json | \
  control_curl -H 'Content-Type: application/json' --data-binary @- "$CONTROL_ORIGIN/holder" >/dev/null || fail holder-sync

for mode in route-refuse malformed protocol-error protocol-timeout issue-error issue; do
  deliver_warm_offer || fail "$mode-arm"
  [ "$(app_pid)" = "$session_pid" ] || fail "$mode-process"
  run_scenario "$mode" || fail "$mode"
  assert_consumed || fail "$mode-consume"
done
capability_burned_before_network=true
one_shot_ready_empty=true

credential_header="$(adb_device shell run-as "$PACKAGE" od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$(adb_device shell run-as "$PACKAGE" wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r\n')"
[ "$credential_header" = 4f58494456433031 ] || fail encrypted-store-header
[ "$credential_key_size" = 32 ] || fail encrypted-store-key
storage_header=true
storage_key=true
if adb_device exec-out run-as "$PACKAGE" cat files/oxid/private/credentials.enc 2>/dev/null | \
  run_deadline 10 rg -a -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|John|Doe|AB1234567'; then
  fail encrypted-store-plaintext
fi
storage_denylist=true
old_pid="$(app_pid)"; [[ "$old_pid" =~ ^[1-9][0-9]*$ ]] || fail process-generation
adb_device shell am force-stop "$PACKAGE" >/dev/null || fail process-stop
for ((_attempt = 0; _attempt < 50; _attempt++)); do [ -z "$(app_pid)" ] && { process_absent=true; break; }; run_deadline 2 sleep 0.1; done
[ "$process_absent" = true ] || fail process-absence
adb_device shell am start -n "$PACKAGE/dev.dioxus.main.MainActivity" >/dev/null 2>>"$PRIVATE_LOG" || fail process-launch
for ((_attempt = 0; _attempt < 60; _attempt++)); do new_pid="$(app_pid)"; [ -n "$new_pid" ] && [ "$new_pid" != "$old_pid" ] && { different_generation=true; break; }; run_deadline 2 sleep 0.25; done
[ "$different_generation" = true ] || fail process-restart
no_data_reset=true
run_scenario restored || fail restored

total_counters="$(counter_snapshot)"
run_deadline 10 jq -e '.authorizationMetadata == 3 and .credential == 1 and .issuerMetadata == 6
  and .issuerResolution == 3 and .issuerResolutionSuccess == 3 and .kyc == 14
  and .nonce == 1 and .other == 0 and .token == 2' <<<"$total_counters" >/dev/null || fail total-counters
scenario_results="$(run_deadline 10 jq -s -c \
  '[.[] | {name:.mode,passed,counterDelta,measurements}]' \
  "$PRIVATE_STATE/scenario-cold-route.json" "$PRIVATE_STATE/scenario-prepare-holder.json" \
  "$PRIVATE_STATE/scenario-route-refuse.json" "$PRIVATE_STATE/scenario-malformed.json" \
  "$PRIVATE_STATE/scenario-protocol-error.json" "$PRIVATE_STATE/scenario-protocol-timeout.json" \
  "$PRIVATE_STATE/scenario-issue-error.json" "$PRIVATE_STATE/scenario-issue.json" \
  "$PRIVATE_STATE/scenario-restored.json")" || fail scenario-results
[ "$(jq 'length' <<<"$scenario_results")" = 9 ] || fail scenario-results
api_level="$(adb_text shell getprop ro.build.version.sdk)"
[[ "$api_level" =~ ^[0-9]+$ ]] || fail platform-api
abi="$(adb_text shell getprop ro.product.cpu.abi)"
case "$abi" in arm64-v8a) architecture=arm64 ;; x86_64) architecture=x86_64 ;; *) fail platform-architecture ;; esac
[ "$SECONDS" -lt "$journey_deadline" ] || fail journey-timeout
journey_deadline=0
journey_status=passed
