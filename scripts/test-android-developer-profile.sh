#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in curl jq node od rg; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
# shellcheck source=scripts/lib/android-test-credential.sh
source "$repository_root/scripts/lib/android-test-credential.sh"

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
if [ -z "$android_sdk" ] || [ ! -x "$android_sdk/platform-tools/adb" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi
adb_command="$android_sdk/platform-tools/adb"
devtools_port=9225
authorization_pid=""
app_state_owned=0
cleanup() {
  if [ -n "$authorization_pid" ] && kill -0 "$authorization_pid" >/dev/null 2>&1; then
    kill "$authorization_pid" >/dev/null 2>&1 || true
    wait "$authorization_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "${device:-}" ]; then
    "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  fi
  if [ "${app_state_owned:-0}" -eq 1 ]; then
    "$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
  fi
  if [ -n "${device:-}" ]; then
    oxid_android_test_credential_cleanup "$adb_command" "$device"
  fi
}
trap cleanup EXIT

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi
if [ -n "$device" ]; then
  oxid_android_test_credential_require_emulator "$adb_command" "$device"
fi

if [ -n "$device" ]; then
  OXID_ANDROID_DEVICE="$device" OXID_ANDROID_REQUIRE_EMULATOR=1 OXID_UI_PROFILE=dev \
    "$repository_root/scripts/run-android-emulator.sh"
else
  OXID_ANDROID_REQUIRE_EMULATOR=1 OXID_UI_PROFILE=dev \
    "$repository_root/scripts/run-android-emulator.sh"
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi
if [ -z "$device" ]; then
  echo "The Android developer-profile harness did not find an online device." >&2
  exit 1
fi
oxid_android_test_credential_prepare "$adb_command" "$device"

echo "Resetting Oxid application data on the admitted Android emulator for the developer-profile smoke."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
app_state_owned=1
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2

process_id=""
websocket_url=""
page_list=""
socket_list=""
for _attempt in $(seq 1 30); do
  process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')"
  socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
  if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
    break
  fi
  sleep 1
done
if [ -z "$process_id" ]; then
  echo "Oxid WebView process did not become available on the admitted Android emulator." >&2
  exit 1
fi

"$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
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

oxid_android_test_credential_authorize "$adb_command" "$device" &
authorization_pid=$!
node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" developer
wait "$authorization_pid"
authorization_pid=""

echo "Android standalone developer-profile manifest smoke passed on the admitted emulator."
