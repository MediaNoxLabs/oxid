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
devtools_port=9227
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
if [[ -z "$device" || "$device" != emulator-* ]]; then
  echo "The focused identity-ingress test is destructive and requires an Android emulator." >&2
  exit 1
fi

echo "Resetting only Oxid application data on Android emulator $device."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null

run_webview_scenario() {
  local mode="$1"
  local process_id=""
  local websocket_url=""
  local page_list=""
  local socket_list=""

  for _attempt in $(seq 1 40); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r' || true)"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
      break
    fi
    sleep 0.5
  done
  if [ -z "$process_id" ]; then
    echo "Oxid WebView process did not become available on $device." >&2
    exit 1
  fi

  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" forward \
    "tcp:$devtools_port" "localabstract:webview_devtools_remote_$process_id" >/dev/null
  for _attempt in $(seq 1 40); do
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
  node "$repository_root/tests/mobile/android-identity-ingress.mjs" "$websocket_url" "$mode"
  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null
}

top_activity() {
  "$adb_command" -s "$device" shell dumpsys activity activities 2>/dev/null \
    | rg 'topResumedActivity|ResumedActivity' \
    | head -n 1 \
    || true
}

wait_for_oxid() {
  for _attempt in $(seq 1 50); do
    if rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$(top_activity)"; then
      return 0
    fi
    sleep 0.2
  done
  echo "Oxid MainActivity did not resume on $device." >&2
  exit 1
}

run_webview_scenario prepare-scan
sleep 3
if rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$(top_activity)"; then
  run_webview_scenario assert-unavailable
  echo "Google Code Scanner is unavailable on this emulator; cancel/timeout remain physical evidence."
else
  "$adb_command" -s "$device" shell input keyevent BACK >/dev/null
  wait_for_oxid
  run_webview_scenario assert-cancelled

  run_webview_scenario prepare-scan
  sleep 65
  "$adb_command" -s "$device" shell input keyevent BACK >/dev/null
  wait_for_oxid
  run_webview_scenario assert-timeout
fi

# Google Code Scanner owns its activity and exposes no programmatic dismissal
# after Oxid closes the timed-out logical generation. Preserve application data
# but establish a clean host process before independently proving warm and cold
# OS-link delivery.
"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
wait_for_oxid

credential_offer_uri='openid-credential-offer://?credential_offer=%7B%7D'
"$adb_command" -s "$device" shell am start -W \
  -a android.intent.action.VIEW \
  -d "$credential_offer_uri" \
  io.medianox.oxid >/dev/null
run_webview_scenario assert-app-link

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start -W \
  -a android.intent.action.VIEW \
  -d "$credential_offer_uri" \
  io.medianox.oxid >/dev/null
run_webview_scenario assert-app-link

echo "Android QR fail-closed and warm/cold custom-scheme ingress passed on emulator $device."
