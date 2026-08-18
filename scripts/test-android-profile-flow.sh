#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in curl jq node rg; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
if [ -z "$android_sdk" ] || [ ! -x "$android_sdk/platform-tools/adb" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi
adb_command="$android_sdk/platform-tools/adb"
devtools_port=9223
trap '"$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true' EXIT

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')"
fi

if [ -n "$device" ]; then
  OXID_ANDROID_DEVICE="$device" "$repository_root/scripts/run-android-emulator.sh"
else
  "$repository_root/scripts/run-android-emulator.sh"
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')"
fi
if [ -z "$device" ]; then
  echo "The Android smoke harness did not find an online device." >&2
  exit 1
fi

echo "Resetting Oxid application data on Android device $device for the smoke flow."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2

run_webview_wallet_flow() {
  local mode="$1"
  local process_id=""
  local websocket_url=""
  local page_list=""
  local socket_list=""

  for _attempt in $(seq 1 30); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
      break
    fi
    sleep 1
  done
  if [ -z "$process_id" ]; then
    echo "Oxid WebView process did not become available on Android device '$device'." >&2
    exit 1
  fi

  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" forward \
    "tcp:$devtools_port" "localabstract:webview_devtools_remote_$process_id" >/dev/null
  for _attempt in $(seq 1 30); do
    page_list="$(curl --noproxy '*' --fail --silent "http://127.0.0.1:$devtools_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
    if [ -n "$websocket_url" ]; then
      break
    fi
    sleep 1
  done
  if [ -z "$websocket_url" ]; then
    echo "Oxid Android WebView did not expose a debuggable page." >&2
    exit 1
  fi

  node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" "$mode"
  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null
}

wait_for_main_activity() {
  local resumed=""
  for _attempt in $(seq 1 50); do
    resumed="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg 'topResumedActivity|ResumedActivity' || true)"
    if rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$resumed"; then
      return 0
    fi
    sleep 0.2
  done
  echo "Oxid MainActivity did not resume after the native share chooser closed." >&2
  return 1
}

run_webview_wallet_flow flow

chooser_state="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null || true)"
if ! rg -q 'ResolverActivity|ChooserActivity|IntentResolverActivity' <<<"$chooser_state"; then
  echo "Android public receive-address share did not open a native chooser." >&2
  exit 1
fi
"$adb_command" -s "$device" shell input keyevent BACK >/dev/null
wait_for_main_activity

credential_offer_uri='openid-credential-offer://?credential_offer=%7B%7D'
"$adb_command" -s "$device" shell am start -W \
  -a android.intent.action.VIEW \
  -d "$credential_offer_uri" \
  io.medianox.oxid >/dev/null
sleep 1
run_webview_wallet_flow app-link

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start -W \
  -a android.intent.action.VIEW \
  -d "$credential_offer_uri" \
  io.medianox.oxid >/dev/null
sleep 2
run_webview_wallet_flow app-link

profile_document=""
for _attempt in $(seq 1 15); do
  profile_document="$($adb_command -s "$device" shell run-as io.medianox.oxid \
    cat files/oxid/wallet-profiles.json 2>/dev/null || true)"
  if [ -n "$profile_document" ]; then
    break
  fi
  sleep 1
done

if ! jq -e '
  .schemaVersion == 2
  and (.profiles | length) == 1
  and .profiles[0].displayName == "My wallet"
  and .profiles[0].id == .activeProfileId
  and (.accountAssociations | length) == 1
  and .accountAssociations[0].profileId == .activeProfileId
  and .accountAssociations[0].selectedNetworkId == "undeployed"
  and (.accountAssociations[0].accounts | length) == 1
  and .accountAssociations[0].accounts[0].networkId == "undeployed"
  and .accountAssociations[0].accounts[0].accountIndex == 0
  and .accountAssociations[0].accounts[0].addressIndex == 0
' >/dev/null <<<"$profile_document"; then
  echo "Android profile creation did not produce the expected durable public metadata." >&2
  exit 1
fi
active_profile_id="$(jq -r '.activeProfileId' <<<"$profile_document")"

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2
run_webview_wallet_flow restored

restored_document="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  cat files/oxid/wallet-profiles.json)"
if [ "$(jq -r '.activeProfileId' <<<"$restored_document")" != "$active_profile_id" ]; then
  echo "Android did not preserve the active profile across process restart." >&2
  exit 1
fi
if [ -z "$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')" ]; then
  echo "Oxid did not remain running after Android profile restoration." >&2
  exit 1
fi

credential_header="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r')"
if [ "$credential_header" != "4f58494456433031" ] || [ "$credential_key_size" != "32" ]; then
  echo "Android credential inventory was not restored from the protected standalone store." >&2
  exit 1
fi

echo "Android protected account, Digital Passport OpenID4VP proof gate/local reveal/disclosure preview/restore, DUST/shielded sync, receive QR/copy/share, cold/warm app links, transfer, and profile-restore smoke flow passed on $device."
