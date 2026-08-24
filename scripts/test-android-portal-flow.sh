#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in curl jq node rg shasum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Required command '$command_name' is missing." >&2
    exit 1
  }
done
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
# shellcheck source=scripts/e2e/portal-mobile-harness-lib.sh
source "$repository_root/scripts/e2e/portal-mobile-harness-lib.sh"
portal_mobile_start android

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
[ -x "$android_sdk/platform-tools/adb" ] || { portal_mobile_fail android-sdk; exit 1; }
adb_command="$android_sdk/platform-tools/adb"
devtools_port=""
portal_mobile_android_forward_active=0
adb_gate_output=""

run_adb_gate_operation() {
  local operation="$1" timeout_seconds="$2"
  shift 2
  adb_gate_output="$PORTAL_MOBILE_STATE_DIR/adb-$operation.out"
  portal_mobile_run_captured_bounded \
    "$adb_gate_output" "$timeout_seconds" "$PORTAL_MOBILE_ADB_KILL_GRACE_SECONDS" \
    "$adb_command" "$@"
}

# Own only the dynamically allocated CDP forward. The shared EXIT cleanup calls
# this hook on every Android exit, including failures and signals.
portal_mobile_platform_cleanup() {
  local remove_pid remove_status=0
  if [ "$portal_mobile_android_forward_active" = 1 ] && \
    [[ "$devtools_port" =~ ^[0-9]+$ ]]; then
    "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" \
      >/dev/null 2>&1 &
    remove_pid=$!
    portal_mobile_wait_bounded "$remove_pid" "$PORTAL_MOBILE_TERM_GRACE_SECONDS" \
      >/dev/null 2>&1 || remove_status=1
    portal_mobile_android_forward_active=0
    devtools_port=""
  fi
  if [ -n "${device:-}" ]; then
    "$adb_command" -s "$device" shell run-as io.medianox.oxid \
      rm -f files/portal-offer.capability files/.portal-offer.capability.tmp >/dev/null 2>&1 &
    remove_pid=$!
    portal_mobile_wait_bounded "$remove_pid" "$PORTAL_MOBILE_TERM_GRACE_SECONDS" \
      >/dev/null 2>&1 || remove_status=1
  fi
  return "$remove_status"
}

# Fixed, non-secret OS trigger shared with iOS. The app recognizes only this
# literal and retrieves the real offer over its bounded loopback worker. The
# real offer therefore never enters adb/device argv, Android intent state,
# logs, or retained evidence; the capability crosses stdin into one app-private
# file that the app unlinks before its authenticated request.
portal_test_offer_trigger="openid-credential-offer://standalone-portal-test-fetch"

# A long-running QEMU can lag the host by several seconds. The strict
# credential policy intentionally has no future-time slack, so cold-reboot an
# already-running disposable emulator instead of weakening verification.
if ! run_adb_gate_operation devices "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" devices; then
  portal_mobile_fail adb-devices
  exit 1
fi
existing_device="$(awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }' "$adb_gate_output" | tr -d '\r\n')"
if [ -n "$existing_device" ]; then
  if ! run_adb_gate_operation qemu-before-reboot \
    "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" \
    -s "$existing_device" shell getprop ro.kernel.qemu; then
    portal_mobile_fail qemu
    exit 1
  fi
  [ "$(tr -d '\r\n' <"$adb_gate_output")" = "1" ] || {
    portal_mobile_fail qemu
    exit 1
  }

  # One real elapsed-time budget covers reboot, reconnect, and every readiness
  # query. Individual adb transports get a shorter bound capped by the shared
  # deadline, so one hang cannot multiply an attempt counter.
  adb_boot_deadline=$((SECONDS + PORTAL_MOBILE_ADB_BOOT_DEADLINE_SECONDS))
  if ! run_adb_gate_operation reboot "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" \
    -s "$existing_device" reboot; then
    portal_mobile_fail emulator-reboot
    exit 1
  fi
  adb_boot_remaining=$((adb_boot_deadline - SECONDS))
  if [ "$adb_boot_remaining" -le 0 ] || \
    ! run_adb_gate_operation wait-for-device "$adb_boot_remaining" \
      -s "$existing_device" wait-for-device; then
    portal_mobile_fail emulator-reconnect
    exit 1
  fi

  boot_completed=""
  while [ "$SECONDS" -lt "$adb_boot_deadline" ]; do
    adb_boot_remaining=$((adb_boot_deadline - SECONDS))
    adb_operation_timeout=$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS
    if [ "$adb_operation_timeout" -gt "$adb_boot_remaining" ]; then
      adb_operation_timeout=$adb_boot_remaining
    fi
    if [ "$adb_operation_timeout" -le 0 ] || \
      ! run_adb_gate_operation boot-completed "$adb_operation_timeout" \
        -s "$existing_device" shell getprop sys.boot_completed; then
      portal_mobile_fail emulator-reboot
      exit 1
    fi
    boot_completed="$(tr -d '\r\n' <"$adb_gate_output")"
    [ "$boot_completed" != "1" ] || break
    adb_boot_remaining=$((adb_boot_deadline - SECONDS))
    [ "$adb_boot_remaining" -gt 0 ] || break
    sleep 1
  done
  [ "$boot_completed" = "1" ] || {
    portal_mobile_fail emulator-reboot
    exit 1
  }
  export OXID_ANDROID_DEVICE="$existing_device"
fi

OXID_MOBILE_CUSTODY=development \
OXID_STANDALONE_NETWORK_PROFILE=local \
OXID_MOBILE_PORTAL_PROFILE=local \
  "$repository_root/scripts/run-android-emulator.sh" \
  >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  if ! run_adb_gate_operation devices-after-launch \
    "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" devices; then
    portal_mobile_fail adb-devices
    exit 1
  fi
  device="$(awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }' "$adb_gate_output" | tr -d '\r\n')"
fi
if [[ -z "$device" || "$device" != emulator-* ]]; then
  portal_mobile_fail qemu
  exit 1
fi
if ! run_adb_gate_operation qemu-after-launch \
  "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" \
  -s "$device" shell getprop ro.kernel.qemu || \
  [ "$(tr -d '\r\n' <"$adb_gate_output")" != "1" ]; then
  portal_mobile_fail qemu
  exit 1
fi
synchronize_android_clock() {
  local sync_epoch host_epoch emulator_epoch
  sync_epoch="$(date -u +%s)"
  [[ "$sync_epoch" =~ ^[0-9]+$ ]] || { portal_mobile_fail host-epoch; return 1; }
  # QEMU can lose a second under UI/crypto load. Start at the already reviewed
  # strict bound; Portal permits a 60-second holder-proof future skew while
  # Oxid continues to reject issuer artifacts from its own future.
  if ! run_adb_gate_operation clock-set-time \
    "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" \
    -s "$device" shell cmd alarm set-time "$(((sync_epoch + 2) * 1000))"; then
    portal_mobile_fail emulator-clock-sync
    return 1
  fi
  host_epoch="$(date -u +%s)"
  if ! run_adb_gate_operation clock-read-time \
    "$PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS" \
    -s "$device" shell date -u +%s; then
    portal_mobile_fail emulator-epoch
    return 1
  fi
  emulator_epoch="$(tr -d '\r\n' <"$adb_gate_output")"
  if ! [[ "$host_epoch" =~ ^[0-9]+$ && "$emulator_epoch" =~ ^[0-9]+$ ]]; then
    portal_mobile_fail emulator-epoch
    return 1
  fi
  clock_skew=$((host_epoch - emulator_epoch))
  if [ "$clock_skew" -lt -2 ] || [ "$clock_skew" -gt 2 ]; then
    portal_mobile_fail emulator-clock-skew
    return 1
  fi
}
synchronize_android_clock
reverse_list="$($adb_command -s "$device" reverse --list)"
for local_port in 8088 9944 6300 18090 18091 18093; do
  if ! awk -v route="tcp:$local_port" '$2 == route && $3 == route { found = 1 } END { exit !found }' <<<"$reverse_list"; then
    portal_mobile_fail "adb-reverse-$local_port"
    exit 1
  fi
done
if rg -q '10\.0\.2\.2' scripts/run-android-emulator.sh scripts/test-android-portal-flow.sh "$PORTAL_MOBILE_MANIFEST_PATH"; then
  portal_mobile_fail forbidden-emulator-alias
  exit 1
fi

"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null

run_webview_scenario() {
  local mode="$1" process_id="" websocket_url="" page_list="" socket_list=""
  local forward_output="$PORTAL_MOBILE_STATE_DIR/android-cdp-forward-port" forward_pid forward_status=0
  for _attempt in $(seq 1 60); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r\n' || true)"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then break; fi
    sleep 0.5
  done
  [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list" || {
    portal_mobile_fail webview-process
    return 1
  }
  : >"$forward_output"
  chmod 600 "$forward_output"
  "$adb_command" -s "$device" forward \
    "tcp:0" "localabstract:webview_devtools_remote_$process_id" >"$forward_output" 2>&1 &
  forward_pid=$!
  portal_mobile_wait_bounded "$forward_pid" "$PORTAL_MOBILE_TERM_GRACE_SECONDS" || forward_status=$?
  devtools_port="$(tr -d '\r\n' <"$forward_output")"
  if [[ "$devtools_port" =~ ^[0-9]+$ ]] && \
    [ "$devtools_port" -ge 1 ] && [ "$devtools_port" -le 65535 ]; then
    portal_mobile_android_forward_active=1
  fi
  if [ "$forward_status" != 0 ] || [ "$portal_mobile_android_forward_active" != 1 ]; then
    portal_mobile_platform_cleanup || true
    portal_mobile_fail adb-forward
    return 1
  fi
  for _attempt in $(seq 1 60); do
    page_list="$(curl --noproxy '*' --fail --silent \
      --connect-timeout 2 --max-time 2 "http://127.0.0.1:$devtools_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
    [ -n "$websocket_url" ] && break
    sleep 0.5
  done
  if [ -z "$websocket_url" ]; then
    portal_mobile_fail webview-page
    portal_mobile_platform_cleanup || true
    return 1
  fi
  if ! OXID_PORTAL_CONTROL_ORIGIN="$PORTAL_MOBILE_CONTROL_ORIGIN" \
    node "$repository_root/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode"; then
    portal_mobile_platform_cleanup || true
    return 1
  fi
  portal_mobile_platform_cleanup
}

sync_public_holder() {
  local public_store="$PORTAL_MOBILE_STATE_DIR/android-public-did.json"
  "$adb_command" -s "$device" exec-out run-as io.medianox.oxid \
    cat files/oxid/private/did-records.json >"$public_store"
  chmod 600 "$public_store"
  curl --noproxy '*' --fail --silent --show-error \
    -H 'Content-Type: application/json' \
    --data-binary "@$public_store" \
    "$PORTAL_MOBILE_CONTROL_ORIGIN/holder" >/dev/null
}

deliver_portal_trigger() {
  curl --noproxy '*' --fail --silent --show-error \
    --connect-timeout "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
    --max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
    -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/arm-android-offer" >/dev/null
  if ! head -c "$PORTAL_MOBILE_OFFER_CAPABILITY_BYTES" <&8 | \
    "$adb_command" -s "$device" shell run-as io.medianox.oxid sh -c \
      'umask 077; mkdir -p files; target=files/portal-offer.capability; candidate=files/.portal-offer.capability.tmp; rm -f "$candidate" "$target"; cat >"$candidate"; [ "$(wc -c <"$candidate")" -eq 64 ] && mv "$candidate" "$target"' \
      >/dev/null 2>>"$PORTAL_MOBILE_PRIVATE_LOG"; then
    portal_mobile_fail offer-capability-provision
    return 1
  fi
  "$adb_command" -s "$device" shell am start -W \
    -a android.intent.action.VIEW \
    -d "$portal_test_offer_trigger" \
    io.medianox.oxid >/dev/null
}

deliver_malformed_offer() {
  "$adb_command" -s "$device" shell am start -W \
    -a android.intent.action.VIEW \
    -d 'openid-credential-offer://?credential_offer=%7B%7D' \
    io.medianox.oxid >/dev/null
}

run_webview_scenario prepare-holder
sync_public_holder

deliver_portal_trigger
run_webview_scenario route-refuse

deliver_malformed_offer
run_webview_scenario malformed

curl --noproxy '*' --fail --silent -X POST --data-binary unavailable \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null
deliver_portal_trigger
run_webview_scenario protocol-error
curl --noproxy '*' --fail --silent -X POST --data-binary normal \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null

curl --noproxy '*' --fail --silent -X POST --data-binary timeout \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null
deliver_portal_trigger
run_webview_scenario protocol-timeout
curl --noproxy '*' --fail --silent -X POST --data-binary normal \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null

# Earlier negative-path UI work can make QEMU fall behind the host issuer.
# Reapply the exact disposable-emulator clock sync immediately before the
# positive strict issuance, while its issuer artifacts are current.
synchronize_android_clock
deliver_portal_trigger
run_webview_scenario issue

# Cleanup uncertainty deliberately locks this prepared review until the
# following force-stop/cold-route process boundary.
deliver_portal_trigger
run_webview_scenario issue-error

curl --noproxy '*' --fail --silent -X POST --data-binary normal \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null

credential_header="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r\n')"
[ "$credential_header" = "4f58494456433031" ] && [ "$credential_key_size" = "32" ] || {
  portal_mobile_fail encrypted-store
  exit 1
}

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
deliver_portal_trigger
run_webview_scenario cold-route
run_webview_scenario restored

counters="$(curl --noproxy '*' --fail --silent "$PORTAL_MOBILE_CONTROL_ORIGIN/counters")"
jq -e '.token == 2 and .nonce == 1 and .credential == 1' >/dev/null <<<"$counters" || {
  portal_mobile_fail protocol-counts
  exit 1
}
portal_mobile_finish || { portal_mobile_fail support-finish; exit 1; }

model="$($adb_command -s "$device" shell getprop ro.product.model | tr -d '\r\n')"
android_version="$($adb_command -s "$device" shell getprop ro.build.version.release | tr -d '\r\n')"
api_level="$($adb_command -s "$device" shell getprop ro.build.version.sdk | tr -d '\r\n')"
portal_mobile_assert_evidence_source || exit 1
evidence="${OXID_PORTAL_ANDROID_EVIDENCE_PATH:-$repository_root/target/portal-mobile-e2e/android/evidence.json}"
evidence_directory="$(dirname -- "$evidence")"
mkdir -p "$evidence_directory"
if ! evidence_temp="$(umask 077 && mktemp "$evidence_directory/.evidence.json.tmp.XXXXXX")"; then
  portal_mobile_fail evidence-temp
  exit 1
fi
PORTAL_MOBILE_EVIDENCE_TEMP="$evidence_temp"
chmod 600 "$evidence_temp" || { portal_mobile_fail evidence-temp; exit 1; }
evidence_document='{
  schema:"oxid-portal-mobile-evidence-v1",
  oxid:{head:$head},
  portal:{integrationCommit:$portalCommit,integrationTree:$portalTree,prHead:$prHead,profileSourceCommit:$profileSource,provenanceSha256:$provenance},
  platform:{kind:"android_qemu_emulator",model:$model,os:$os,apiLevel:$api,clockSkewSeconds:$clockSkew,applicationId:"io.medianox.oxid",profile:"standalone-local-development-portal",adbReversePorts:[6300,8088,9944,18090,18091,18093]},
  acceptance:{mockKycApproved:true,warmColdCustomScheme:true,oneItemStrictRouter:true,explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,processRestart:true,developmentCustodyReactivated:true,reverified:true,malformedDenied:true,unavailableDenied:true,timeoutDenied:true,qemuVerified:true,clockSynchronized:true,noEmulatorAlias:true,secretFreeEvidence:true}
}'
evidence_sentinel='openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|emulator-[0-9]+'
if ! jq -cn \
  --arg head "$PORTAL_MOBILE_OXID_HEAD" \
  --arg model "$model" \
  --arg os "$android_version" \
  --arg api "$api_level" \
  --argjson clockSkew "$clock_skew" \
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" \
  --arg portalTree "$PORTAL_INTEGRATION_TREE" \
  --arg prHead "$PORTAL_PR_HEAD" \
  --arg profileSource "$PORTAL_PROFILE_SOURCE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" \
  "$evidence_document" >"$evidence_temp"; then
  portal_mobile_discard_evidence_temp "$evidence_temp" || true
  portal_mobile_fail evidence-generate
  exit 1
fi
portal_mobile_finalize_evidence \
  "$evidence" "$evidence_temp" "$evidence_document" "$evidence_sentinel" \
  --arg head "$PORTAL_MOBILE_OXID_HEAD" \
  --arg model "$model" \
  --arg os "$android_version" \
  --arg api "$api_level" \
  --argjson clockSkew "$clock_skew" \
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" \
  --arg portalTree "$PORTAL_INTEGRATION_TREE" \
  --arg prHead "$PORTAL_PR_HEAD" \
  --arg profileSource "$PORTAL_PROFILE_SOURCE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" || exit 1
printf 'Android Portal emulator smoke passed at %s on %s, Android %s (API %s), app io.medianox.oxid; evidence=%s\n' \
  "$PORTAL_MOBILE_OXID_HEAD" "$model" "$android_version" "$api_level" "${evidence#"$repository_root/"}"
