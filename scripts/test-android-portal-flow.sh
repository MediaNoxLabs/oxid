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
devtools_port=9231

# Fixed, non-secret OS trigger shared with iOS. The app recognizes only this
# literal and retrieves the real offer over its bounded loopback worker. The
# real offer therefore never enters adb/device argv, Android intent state, a
# host/device staging file, logs, or retained evidence.
portal_test_offer_trigger="openid-credential-offer://standalone-portal-test-fetch"

# A long-running QEMU can lag the host by several seconds. The strict
# credential policy intentionally has no future-time slack, so cold-reboot an
# already-running disposable emulator instead of weakening verification.
existing_device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
if [ -n "$existing_device" ]; then
  if [ "$($adb_command -s "$existing_device" shell getprop ro.kernel.qemu 2>/dev/null | tr -d '
')" != "1" ]; then
    portal_mobile_fail qemu
    exit 1
  fi
  "$adb_command" -s "$existing_device" reboot
  "$adb_command" -s "$existing_device" wait-for-device
  for _attempt in $(seq 1 120); do
    if [ "$($adb_command -s "$existing_device" shell getprop sys.boot_completed 2>/dev/null | tr -d '
')" = "1" ]; then
      break
    fi
    sleep 1
  done
  [ "$($adb_command -s "$existing_device" shell getprop sys.boot_completed 2>/dev/null | tr -d '
')" = "1" ] || {
    portal_mobile_fail emulator-reboot
    exit 1
  }
  export OXID_ANDROID_DEVICE="$existing_device"
fi

OXID_MOBILE_CUSTODY=development \
OXID_STANDALONE_NETWORK_PROFILE=local \
OXID_MOBILE_PORTAL_PROFILE=local \
  "$repository_root/scripts/run-android-emulator.sh"

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi
if [[ -z "$device" || "$device" != emulator-* ]] || \
  [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "1" ]; then
  portal_mobile_fail qemu
  exit 1
fi
sync_epoch="$(date -u +%s)"
if ! "$adb_command" -s "$device" shell cmd alarm set-time "$((sync_epoch * 1000))"; then
  portal_mobile_fail emulator-clock-sync
  exit 1
fi
host_epoch="$(date -u +%s)"
emulator_epoch="$($adb_command -s "$device" shell date -u +%s | tr -d '
')"
clock_skew=$((host_epoch - emulator_epoch))
# Exact Final credential verification has no future-time slack. Synchronize
# this disposable QEMU through Android's clock service, then keep the strict
# bound instead of admitting an issuer timestamp that is still in the future.
if [ "$clock_skew" -lt -2 ] || [ "$clock_skew" -gt 2 ]; then
  portal_mobile_fail emulator-clock-skew
  exit 1
fi
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
  for _attempt in $(seq 1 60); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r' || true)"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then break; fi
    sleep 0.5
  done
  [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list" || {
    portal_mobile_fail webview-process
    return 1
  }
  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" forward \
    "tcp:$devtools_port" "localabstract:webview_devtools_remote_$process_id" >/dev/null
  for _attempt in $(seq 1 60); do
    page_list="$(curl --noproxy '*' --fail --silent "http://127.0.0.1:$devtools_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
    [ -n "$websocket_url" ] && break
    sleep 0.5
  done
  [ -n "$websocket_url" ] || { portal_mobile_fail webview-page; return 1; }
  OXID_PORTAL_CONTROL_ORIGIN="$PORTAL_MOBILE_CONTROL_ORIGIN" \
    node "$repository_root/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode"
  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null
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
run_webview_scenario protocol-error
curl --noproxy '*' --fail --silent -X POST --data-binary normal \
  "$PORTAL_MOBILE_CONTROL_ORIGIN/proxy-mode" >/dev/null

deliver_portal_trigger
run_webview_scenario issue

credential_header="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r')"
[ "$credential_header" = "4f58494456433031" ] && [ "$credential_key_size" = "32" ] || {
  portal_mobile_fail encrypted-store
  exit 1
}

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
deliver_portal_trigger
run_webview_scenario cold-route
run_webview_scenario restored

counters="$(curl --noproxy '*' --fail --silent "$PORTAL_MOBILE_CONTROL_ORIGIN/counters")"
jq -e '.token == 1 and .nonce == 1 and .credential == 1' >/dev/null <<<"$counters" || {
  portal_mobile_fail protocol-counts
  exit 1
}
portal_mobile_finish || { portal_mobile_fail support-finish; exit 1; }

model="$($adb_command -s "$device" shell getprop ro.product.model | tr -d '\r')"
android_version="$($adb_command -s "$device" shell getprop ro.build.version.release | tr -d '\r')"
api_level="$($adb_command -s "$device" shell getprop ro.build.version.sdk | tr -d '\r')"
evidence="$repository_root/target/portal-mobile-e2e/android/evidence.json"
mkdir -p "$(dirname -- "$evidence")"
jq -cn \
  --arg head "$(git rev-parse HEAD)" \
  --arg model "$model" \
  --arg os "$android_version" \
  --arg api "$api_level" \
  --argjson clockSkew "$clock_skew" \
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" \
  --arg portalTree "$PORTAL_INTEGRATION_TREE" \
  --arg prHead "$PORTAL_PR_HEAD" \
  --arg profileSource "$PORTAL_PROFILE_SOURCE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" \
  '{
    schema:"oxid-portal-mobile-evidence-v1",
    oxid:{head:$head},
    portal:{integrationCommit:$portalCommit,integrationTree:$portalTree,prHead:$prHead,profileSourceCommit:$profileSource,provenanceSha256:$provenance},
    platform:{kind:"android_qemu_emulator",model:$model,os:$os,apiLevel:$api,clockSkewSeconds:$clockSkew,applicationId:"io.medianox.oxid",profile:"standalone-local-development-portal",adbReversePorts:[6300,8088,9944,18090,18091,18093]},
    acceptance:{mockKycApproved:true,warmColdCustomScheme:true,oneItemStrictRouter:true,explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,processRestart:true,developmentCustodyReactivated:true,reverified:true,malformedDenied:true,unavailableDenied:true,timeoutDenied:true,qemuVerified:true,clockSynchronized:true,noEmulatorAlias:true,secretFreeEvidence:true}
  }' >"$evidence"
if rg -qi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|emulator-[0-9]+' "$evidence"; then
  portal_mobile_fail evidence-schema
  exit 1
fi
printf 'Android Portal emulator smoke passed at %s on %s, Android %s (API %s), app io.medianox.oxid; evidence=%s\n' \
  "$(git rev-parse HEAD)" "$model" "$android_version" "$api_level" "${evidence#"$repository_root/"}"
