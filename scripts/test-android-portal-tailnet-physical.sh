#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Successful mocked Portal issuance only, on the one reviewed physical phone.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: test-android-portal-tailnet-physical.sh <absolute-stack-env> R5CX82NAS0P" >&2
  exit 2
fi
STACK_ENV_FILE="$1"
readonly expected_serial="R5CX82NAS0P"
serial="$2"
[[ "$STACK_ENV_FILE" = /* ]] && [ -f "$STACK_ENV_FILE" ] && [ ! -L "$STACK_ENV_FILE" ] || {
  echo "The stack environment must be an absolute regular non-symlink file." >&2
  exit 1
}
[ "$serial" = "$expected_serial" ] || {
  echo "This demo requires explicit serial R5CX82NAS0P." >&2
  exit 1
}
for command_name in curl jq node rg shasum tailscale; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Required command '$command_name' is missing." >&2
    exit 1
  }
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = Darwin ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
adb_command="$android_sdk/platform-tools/adb"
[ -x "$adb_command" ] || { echo "Android platform-tools are unavailable." >&2; exit 1; }
[ "$($adb_command -s "$serial" get-state 2>/dev/null || true)" = device ] && \
  [ "$($adb_command -s "$serial" shell getprop ro.kernel.qemu | tr -d '\r\n')" = 0 ] && \
  [[ "$serial" != emulator-* ]] || {
  echo "R5CX82NAS0P must be an online non-QEMU physical device." >&2
  exit 1
}
if "$adb_command" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
  echo "Stop every Android emulator before the physical demo." >&2
  exit 1
fi

export STACK_ENV_FILE
export OXID_ANDROID_DEVICE="$serial"
export OXID_MOBILE_PORTAL_PROFILE=tailnet-android-physical
export OXID_BUILD_PORTAL_PUBLIC_ORIGIN="https://yuriys-macbook-pro.taila4adff.ts.net:9443"
# shellcheck source=scripts/e2e/portal-mobile-harness-lib.sh
source "$repository_root/scripts/e2e/portal-mobile-harness-lib.sh"
portal_mobile_start android

device="$serial"
devtools_port=""
portal_mobile_android_forward_active=0
portal_mobile_platform_cleanup() {
  local status=0
  if [ "$portal_mobile_android_forward_active" = 1 ] && [[ "$devtools_port" =~ ^[0-9]+$ ]]; then
    "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || status=1
    portal_mobile_android_forward_active=0
    devtools_port=""
  fi
  "$adb_command" -s "$device" shell run-as io.medianox.oxid \
    rm -f files/portal-offer.capability files/.portal-offer.capability.tmp >/dev/null 2>&1 || true
  return "$status"
}

# The physical launcher validates this same serial and uses authenticated
# tailnet HTTPS. It neither starts an emulator nor creates adb reverse routes.
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
OXID_MOBILE_CUSTODY=development \
OXID_MOBILE_PORTAL_PROFILE=tailnet-android-physical \
  "$repository_root/scripts/run-android-tailnet.sh" \
  >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1

run_webview_scenario() {
  local mode="$1" process_id="" socket_list="" page_list="" websocket_url=""
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
  devtools_port="$($adb_command -s "$device" forward \
    "tcp:0" "localabstract:webview_devtools_remote_$process_id" | tr -d '\r\n')"
  [[ "$devtools_port" =~ ^[0-9]+$ ]] || { portal_mobile_fail adb-forward; return 1; }
  portal_mobile_android_forward_active=1
  for _attempt in $(seq 1 60); do
    page_list="$(curl --noproxy '*' --fail --silent --connect-timeout 2 --max-time 2 \
      "http://127.0.0.1:$devtools_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page" and .url == "https://dioxus.index.html/")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
    [ -n "$websocket_url" ] && break
    sleep 0.5
  done
  [ -n "$websocket_url" ] || { portal_mobile_fail webview-page; portal_mobile_platform_cleanup || true; return 1; }
  OXID_PORTAL_CONTROL_ORIGIN="$PORTAL_MOBILE_CONTROL_ORIGIN" \
    node "$repository_root/tests/mobile/android-portal-flow.mjs" "$websocket_url" "$mode" \
    >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 || {
      portal_mobile_platform_cleanup || true
      return 1
    }
  portal_mobile_platform_cleanup
}

sync_public_holder() {
  local public_store="$PORTAL_MOBILE_STATE_DIR/android-public-did.json"
  "$adb_command" -s "$device" exec-out run-as io.medianox.oxid \
    cat files/oxid/private/did-records.json >"$public_store"
  chmod 600 "$public_store"
  curl --noproxy '*' --fail --silent --show-error -H 'Content-Type: application/json' \
    --data-binary "@$public_store" "$PORTAL_MOBILE_CONTROL_ORIGIN/holder" >/dev/null
}

deliver_portal_trigger() {
  local portal_test_offer_trigger="openid-credential-offer://standalone-portal-test-fetch"
  local capability_stage_command
  curl --noproxy '*' --fail --silent --show-error \
    --connect-timeout "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
    --max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
    -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/arm-android-offer" >/dev/null
  # adb joins separate shell arguments without preserving their quoting. Pass
  # one remotely quoted command so every filesystem operation remains under
  # run-as instead of escaping to Android's outer shell and read-only `/`.
  capability_stage_command="run-as io.medianox.oxid sh -c 'umask 077; mkdir -p files; target=files/portal-offer.capability; candidate=files/.portal-offer.capability.tmp; rm -f \"\$candidate\" \"\$target\"; cat >\"\$candidate\"; [ \"\$(wc -c <\"\$candidate\")\" -eq 64 ] && mv \"\$candidate\" \"\$target\"'"
  head -c "$PORTAL_MOBILE_OFFER_CAPABILITY_BYTES" <&8 | \
    "$adb_command" -s "$device" shell "$capability_stage_command" \
      >/dev/null 2>>"$PORTAL_MOBILE_PRIVATE_LOG"
  "$adb_command" -s "$device" shell am start -W -a android.intent.action.VIEW \
    -d "$portal_test_offer_trigger" io.medianox.oxid >/dev/null
}

run_webview_scenario prepare-holder
sync_public_holder
deliver_portal_trigger
run_webview_scenario issue

credential_header="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r\n')"
[ "$credential_header" = 4f58494456433031 ] && [ "$credential_key_size" = 32 ] || {
  portal_mobile_fail encrypted-store
  exit 1
}

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
run_webview_scenario restored

counters="$(curl --noproxy '*' --fail --silent "$PORTAL_MOBILE_CONTROL_ORIGIN/counters")"
jq -e '.token == 1 and .nonce == 1 and .credential == 1 and .issuerResolutionSuccess >= 2' \
  >/dev/null <<<"$counters" || { portal_mobile_fail protocol-counts; exit 1; }
portal_mobile_finish || { portal_mobile_fail support-finish; exit 1; }

model="$($adb_command -s "$device" shell getprop ro.product.model | tr -d '\r\n')"
android_version="$($adb_command -s "$device" shell getprop ro.build.version.release | tr -d '\r\n')"
api_level="$($adb_command -s "$device" shell getprop ro.build.version.sdk | tr -d '\r\n')"
portal_mobile_assert_evidence_source || exit 1
evidence="${OXID_PORTAL_ANDROID_PHYSICAL_EVIDENCE_PATH:-$repository_root/target/portal-mobile-e2e/android-physical-tailnet/evidence.json}"
evidence_directory="$(dirname -- "$evidence")"
mkdir -p "$evidence_directory"
evidence_temp="$(umask 077 && mktemp "$evidence_directory/.evidence.json.tmp.XXXXXX")" || {
  portal_mobile_fail evidence-temp
  exit 1
}
PORTAL_MOBILE_EVIDENCE_TEMP="$evidence_temp"
chmod 600 "$evidence_temp"
evidence_document='{
  schema:"oxid-portal-mobile-evidence-v1",
  oxid:{head:$head},
  portal:{helperCommit:$helperCommit,helperTree:$helperTree,integrationCommit:$portalCommit,integrationTree:$portalTree,prHead:$prHead,profileSourceCommit:$profileSource,provenanceSha256:$provenance},
  platform:{kind:"android_physical_tailnet",model:$model,os:$os,apiLevel:$api,applicationId:"io.medianox.oxid",profile:"standalone-tailnet-development-portal-android-physical"},
  acceptance:{mockKycApproved:true,authenticatedTailnetHttps:true,oneItemStrictRouter:true,explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,processRestart:true,developmentCustodyReactivated:true,reverified:true,physicalDeviceVerified:true,noAdbReverse:true,secretFreeEvidence:true}
}'
evidence_sentinel='openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|R5CX82NAS0P|emulator-[0-9]+'
jq_args=(
  --arg head "$PORTAL_MOBILE_OXID_HEAD" --arg model "$model" --arg os "$android_version" --arg api "$api_level"
  --arg helperCommit "$PORTAL_HELPER_COMMIT" --arg helperTree "$PORTAL_HELPER_TREE"
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" --arg portalTree "$PORTAL_INTEGRATION_TREE"
  --arg prHead "$PORTAL_PR_HEAD" --arg profileSource "$PORTAL_PROFILE_SOURCE"
  --arg provenance "$PORTAL_PROVENANCE_SHA256"
)
jq -cn "${jq_args[@]}" "$evidence_document" >"$evidence_temp" || {
  portal_mobile_discard_evidence_temp "$evidence_temp" || true
  portal_mobile_fail evidence-generate
  exit 1
}
portal_mobile_finalize_evidence \
  "$evidence" "$evidence_temp" "$evidence_document" "$evidence_sentinel" \
  "${jq_args[@]}" || exit 1
printf 'Android physical Portal tailnet demo passed at %s on %s, Android %s (API %s); evidence=%s\n' \
  "$PORTAL_MOBILE_OXID_HEAD" "$model" "$android_version" "$api_level" "${evidence#"$repository_root/"}"
