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
devtools_port=9229

cleanup() {
  if [ -n "${device:-}" ]; then
    "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

"$repository_root/scripts/standalone-up.sh" local

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi
if [ -n "$device" ]; then
  OXID_ANDROID_DEVICE="$device" OXID_STANDALONE_NETWORK_PROFILE=local \
    "$repository_root/scripts/run-android-emulator.sh"
else
  OXID_STANDALONE_NETWORK_PROFILE=local \
    "$repository_root/scripts/run-android-emulator.sh"
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi

if [[ -z "$device" || "$device" != emulator-* ]] || \
  [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "1" ]; then
  echo "The localhost standalone smoke test requires an Android emulator." >&2
  exit 1
fi

reverse_list="$($adb_command -s "$device" reverse --list)"
for local_port in 8088 9944 6300; do
  if ! awk -v route="tcp:$local_port" '$2 == route && $3 == route { found = 1 } END { exit !found }' \
    <<<"$reverse_list"; then
    echo "Android emulator reverse route tcp:$local_port is missing." >&2
    exit 1
  fi
done

echo "Resetting only Oxid application data on Android emulator $device."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null

process_id=""
socket_list=""
for _attempt in $(seq 1 60); do
  process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r' || true)"
  socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
  if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
    break
  fi
  sleep 0.5
done
if [ -z "$process_id" ] || ! rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
  echo "Oxid WebView process did not become available on Android emulator '$device'." >&2
  exit 1
fi

"$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
"$adb_command" -s "$device" forward \
  "tcp:$devtools_port" "localabstract:webview_devtools_remote_$process_id" >/dev/null

websocket_url=""
for _attempt in $(seq 1 60); do
  page_list="$(curl --noproxy '*' --fail --silent "http://127.0.0.1:$devtools_port/json" || true)"
  websocket_url="$(jq -r 'first(.[] | select(.type == "page")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
  if [ -n "$websocket_url" ]; then
    break
  fi
  sleep 0.5
done
if [ -z "$websocket_url" ]; then
  echo "Oxid Android WebView did not expose a debuggable page." >&2
  exit 1
fi

node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" live-account
cleanup

"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
model="$($adb_command -s "$device" shell getprop ro.product.model | tr -d '\r')"
android_version="$($adb_command -s "$device" shell getprop ro.build.version.release | tr -d '\r')"
api_level="$($adb_command -s "$device" shell getprop ro.build.version.sdk | tr -d '\r')"
echo "Android localhost standalone live-account smoke passed at $(git rev-parse HEAD) on $model, Android $android_version (API $api_level), application io.medianox.oxid."
