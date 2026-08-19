#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in curl jq nix node rg rustup; do
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
emulator_command="$android_sdk/emulator/emulator"

device="${OXID_ANDROID_DEVICE:-}"
started_emulator=0
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $1 ~ /^emulator-/ && $2 == "device" { print $1; exit }')"
fi
if [ -z "$device" ]; then
  if [ ! -x "$emulator_command" ]; then
    echo "The Android complete-backup smoke test requires an Android emulator." >&2
    exit 1
  fi
  avd="${OXID_ANDROID_AVD:-$($emulator_command -list-avds | sed -n '1p')}"
  if [ -z "$avd" ]; then
    echo "No configured Android AVD was found." >&2
    exit 1
  fi
  "$emulator_command" -avd "$avd" -no-snapshot-save >/dev/null 2>&1 &
  started_emulator=1
  for _attempt in $(seq 1 120); do
    device="$($adb_command devices | awk 'NR > 1 && $1 ~ /^emulator-/ && $2 == "device" { print $1; exit }')"
    if [ -n "$device" ]; then
      break
    fi
    sleep 1
  done
fi
case "$device" in
  emulator-*) ;;
  *)
    echo "The Android complete-backup smoke test refuses physical device '$device'." >&2
    exit 1
    ;;
esac

wait_for_boot() {
  "$adb_command" -s "$device" wait-for-device
  for _attempt in $(seq 1 180); do
    if [ "$($adb_command -s "$device" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
      return 0
    fi
    sleep 1
  done
  echo "Android emulator '$device' did not finish booting." >&2
  return 1
}
wait_for_boot

backup_directory="OxidBackupSmoke-$$"
remote_directory="/sdcard/Download/$backup_directory"
remote_backup="$remote_directory/oxid-wallet.oxidbak"
remote_ui_dump="/sdcard/oxid-backup-window-$$.xml"
devtools_port=9224
flow_pid=""
websocket_url=""

cleanup() {
  local exit_status=$?
  if [ -n "$flow_pid" ] && kill -0 "$flow_pid" >/dev/null 2>&1; then
    kill "$flow_pid" >/dev/null 2>&1 || true
    wait "$flow_pid" >/dev/null 2>&1 || true
  fi
  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" shell rm -f "$remote_ui_dump" >/dev/null 2>&1 || true
  if [ "$exit_status" -ne 0 ] && [ "${OXID_ANDROID_KEEP_FAILED_BACKUP_STATE:-0}" = "1" ]; then
    echo "Keeping failed Android backup state in $remote_directory on $device." >&2
    return
  fi
  case "$backup_directory" in
    OxidBackupSmoke-[0-9]*) ;;
    *)
      echo "Refusing cleanup for unexpected Android backup directory '$backup_directory'." >&2
      return
      ;;
  esac
  "$adb_command" -s "$device" shell rm -f "$remote_backup" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" shell rmdir "$remote_directory" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
  if [ "$started_emulator" -eq 1 ]; then
    "$adb_command" -s "$device" emu kill >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if "$adb_command" -s "$device" shell test -e "$remote_directory"; then
  echo "Refusing to reuse existing Android backup directory '$remote_directory'." >&2
  exit 1
fi
"$adb_command" -s "$device" shell mkdir "$remote_directory"

OXID_ANDROID_DEVICE="$device" OXID_MOBILE_CUSTODY=development \
  "$repository_root/scripts/run-android-emulator.sh"

echo "Resetting Oxid application data on Android emulator $device for complete backup export."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2

prepare_webview_wallet_flow() {
  local process_id=""
  local page_list=""
  local socket_list=""
  websocket_url=""

  for _attempt in $(seq 1 60); do
    process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')"
    socket_list="$($adb_command -s "$device" shell cat /proc/net/unix 2>/dev/null || true)"
    if [ -n "$process_id" ] && rg -q "@webview_devtools_remote_${process_id}$" <<<"$socket_list"; then
      break
    fi
    sleep 1
  done
  if [ -z "$process_id" ]; then
    echo "Oxid WebView process did not become available on Android emulator '$device'." >&2
    return 1
  fi

  "$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" forward \
    "tcp:$devtools_port" "localabstract:webview_devtools_remote_$process_id" >/dev/null
  for _attempt in $(seq 1 60); do
    page_list="$(curl --noproxy '*' --fail --silent "http://127.0.0.1:$devtools_port/json" || true)"
    websocket_url="$(jq -r 'first(.[] | select(.type == "page")) | .webSocketDebuggerUrl // empty' <<<"$page_list")"
    if [ -n "$websocket_url" ]; then
      break
    fi
    sleep 1
  done
  if [ -z "$websocket_url" ]; then
    echo "Oxid Android WebView did not expose a debuggable page." >&2
    return 1
  fi

}

wait_for_documents_ui() {
  for _attempt in $(seq 1 180); do
    if "$adb_command" -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg -q 'topResumedActivity=.*com\.google\.android\.documentsui'; then
      return 0
    fi
    if [ -n "$flow_pid" ] && ! kill -0 "$flow_pid" >/dev/null 2>&1; then
      wait "$flow_pid"
      return 1
    fi
    sleep 0.25
  done
  echo "Android DocumentsUI did not become the foreground document picker." >&2
  return 1
}

dump_ui_nodes() {
  "$adb_command" -s "$device" shell uiautomator dump "$remote_ui_dump" >/dev/null
  "$adb_command" -s "$device" shell cat "$remote_ui_dump" \
    | awk 'BEGIN { RS = ">" } NF { print $0 ">" }'
}

ui_node() {
  local fragment="$1"
  local nodes=""
  local node=""
  for _attempt in $(seq 1 80); do
    nodes="$(dump_ui_nodes 2>/dev/null || true)"
    node="$(rg -F -m1 "$fragment" <<<"$nodes" || true)"
    if [ -n "$node" ]; then
      printf '%s\n' "$node"
      return 0
    fi
    sleep 0.25
  done
  echo "Android DocumentsUI did not expose '$fragment'." >&2
  return 1
}

ui_has_fragment() {
  local fragment="$1"
  local nodes=""
  nodes="$(dump_ui_nodes 2>/dev/null || true)"
  rg -F -q "$fragment" <<<"$nodes"
}

tap_ui_fragment() {
  local fragment="$1"
  local node=""
  node="$(ui_node "$fragment")"
  if [[ "$node" =~ bounds=\"\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\]\" ]]; then
    local x=$(( (BASH_REMATCH[1] + BASH_REMATCH[3]) / 2 ))
    local y=$(( (BASH_REMATCH[2] + BASH_REMATCH[4]) / 2 ))
    "$adb_command" -s "$device" shell input tap "$x" "$y" >/dev/null
    return 0
  fi
  echo "Android DocumentsUI element '$fragment' had no bounded tap target." >&2
  return 1
}

open_backup_directory() {
  if ui_has_fragment "text=\"$backup_directory\" resource-id=\"com.google.android.documentsui:id/breadcrumb_text\""; then
    return 0
  fi
  if ui_has_fragment "text=\"$backup_directory\""; then
    tap_ui_fragment "text=\"$backup_directory\""
  else
    tap_ui_fragment 'content-desc="Show roots"'
    tap_ui_fragment 'text="Downloads" resource-id="android:id/title"'
    tap_ui_fragment "text=\"$backup_directory\""
  fi
  ui_node "text=\"$backup_directory\" resource-id=\"com.google.android.documentsui:id/breadcrumb_text\"" >/dev/null
}

prepare_webview_wallet_flow
node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" backup-export &
flow_pid=$!
wait_for_documents_ui
open_backup_directory
tap_ui_fragment 'text="SAVE" resource-id="android:id/button1"'
wait "$flow_pid"
flow_pid=""
"$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true

backup_size="$($adb_command -s "$device" shell stat -c %s "$remote_backup" 2>/dev/null | tr -d '\r')"
if [[ ! "$backup_size" =~ ^[0-9]+$ ]] || [ "$backup_size" -le 32 ]; then
  echo "Android complete backup was not written as a non-empty document." >&2
  exit 1
fi

apk="$repository_root/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
if [ ! -f "$apk" ]; then
  echo "Android backup smoke could not find the built Oxid APK: $apk" >&2
  exit 1
fi

"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" uninstall io.medianox.oxid >/dev/null
"$adb_command" -s "$device" reboot
wait_for_boot
"$adb_command" -s "$device" install "$apk" >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2

prepare_webview_wallet_flow
node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" backup-recover &
flow_pid=$!
wait_for_documents_ui
open_backup_directory
tap_ui_fragment 'text="oxid-wallet.oxidbak"'
wait "$flow_pid"
flow_pid=""
"$adb_command" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true

profile_document="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  cat files/oxid/wallet-profiles.json 2>/dev/null || true)"
if ! jq -e '
  .schemaVersion == 3
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
  and (.completeBackupReceipts | length) == 0
' >/dev/null <<<"$profile_document"; then
  echo "Android complete recovery did not restore the exact profile/account association." >&2
  exit 1
fi

credential_header="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  od -An -tx1 -N8 files/oxid/private/credentials.enc 2>/dev/null | tr -d ' \r\n')"
credential_key_size="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  wc -c files/oxid/private/credentials.key 2>/dev/null | awk '{print $1}' | tr -d '\r')"
if [ "$credential_header" != "4f58494456433031" ] || [ "$credential_key_size" != "32" ]; then
  echo "Android complete recovery did not restore the encrypted credential inventory." >&2
  exit 1
fi

echo "Android complete-wallet native document export, uninstall, reboot, reinstall, import, and recovery passed on emulator $device."
