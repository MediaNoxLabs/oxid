#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

mode="${1:-}"
case "$mode" in
  show-offer-qr|status|prepare-scan|assert-qr-offer|assert-cancelled|assert-timeout|assert-unavailable|link-warm|link-cold)
    ;;
  *)
    echo "Usage: $0 {show-offer-qr|status|prepare-scan|assert-qr-offer|assert-cancelled|assert-timeout|assert-unavailable|link-warm|link-cold}" >&2
    exit 1
    ;;
esac

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
offer_uri='openid-credential-offer://?credential_offer=%7B%7D'

if [ "$mode" = "show-offer-qr" ]; then
  if ! command -v qrencode >/dev/null 2>&1; then
    echo "The qrencode command is required to display the deterministic offer." >&2
    exit 1
  fi
  qr_directory="$repository_root/target/physical-evidence"
  qr_path="$qr_directory/identity-offer.png"
  mkdir -p "$qr_directory"
  chmod 700 "$qr_directory"
  qrencode -l M -s 10 -m 4 -o "$qr_path" "$offer_uri"
  chmod 600 "$qr_path"
  if [ "$(uname -s)" = "Darwin" ]; then
    open "$qr_path"
  fi
  echo "Deterministic credential-offer QR written under ignored target/physical-evidence."
  exit 0
fi

for command_name in curl jq node rg; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
adb_command="$android_sdk/platform-tools/adb"
if [ ! -x "$adb_command" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi
if "$adb_command" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
  echo "Stop the Android emulator before collecting physical-device evidence." >&2
  exit 1
fi
if xcrun simctl list devices 2>/dev/null | grep -q '(Booted)'; then
  echo "Shut down the iOS simulator before collecting physical-device evidence." >&2
  exit 1
fi

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices -l | awk '$2 == "device" && $1 !~ /^emulator-/ {print $1; exit}')"
fi
if [ -z "$device" ] || [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu | tr -d '\r')" != "0" ]; then
  echo "An authorized physical Android device is required." >&2
  exit 1
fi
if ! "$adb_command" -s "$device" shell pm path io.medianox.oxid 2>/dev/null | rg -q '^package:'; then
  echo "Install the Oxid standalone-development build before running this harness." >&2
  exit 1
fi

model="$($adb_command -s "$device" shell getprop ro.product.model | tr -d '\r')"
android_version="$($adb_command -s "$device" shell getprop ro.build.version.release | tr -d '\r')"
api_level="$($adb_command -s "$device" shell getprop ro.build.version.sdk | tr -d '\r')"
if [ "$mode" = "status" ]; then
  echo "Physical device: $model; Android $android_version (API $api_level); application io.medianox.oxid."
  exit 0
fi

devtools_port=9228
cleanup() {
  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
}
trap cleanup EXIT

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
  echo "Oxid MainActivity did not resume on the physical device." >&2
  exit 1
}

start_oxid_if_needed() {
  if ! rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$(top_activity)"; then
    "$adb_command" -s "$device" shell am start \
      -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
  fi
  wait_for_oxid
}

run_webview_scenario() {
  local scenario="$1"
  local process_id=""
  local socket_list=""
  local page_list=""
  local websocket_url=""

  for _attempt in $(seq 1 40); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid 2>/dev/null | tr -d '\r' || true)"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
      break
    fi
    sleep 0.5
  done
  if [ -z "$process_id" ]; then
    echo "Oxid WebView process did not become available on the physical device." >&2
    exit 1
  fi

  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
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
  node "$repository_root/tests/mobile/android-identity-ingress.mjs" "$websocket_url" "$scenario"
  cleanup
}

case "$mode" in
  prepare-scan)
    start_oxid_if_needed
    run_webview_scenario prepare-scan
    ;;
  assert-qr-offer|assert-cancelled|assert-timeout|assert-unavailable)
    run_webview_scenario "$mode"
    ;;
  link-warm)
    start_oxid_if_needed
    "$adb_command" -s "$device" shell am start -W \
      -a android.intent.action.VIEW -d "$offer_uri" io.medianox.oxid >/dev/null
    wait_for_oxid
    run_webview_scenario assert-app-link
    ;;
  link-cold)
    "$adb_command" -s "$device" shell am force-stop io.medianox.oxid
    "$adb_command" -s "$device" shell am start -W \
      -a android.intent.action.VIEW -d "$offer_uri" io.medianox.oxid >/dev/null
    wait_for_oxid
    run_webview_scenario assert-app-link
    ;;
esac

echo "Physical Android identity-ingress mode '$mode' passed on $model (API $api_level)."
