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

run_webview_wallet_flow flow

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
  .schemaVersion == 1
  and (.profiles | length) == 1
  and .profiles[0].displayName == "My wallet"
  and .profiles[0].id == .activeProfileId
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

echo "Android protected account, DUST/shielded sync, receive QR, transfer, and profile-restore smoke flow passed on $device."
