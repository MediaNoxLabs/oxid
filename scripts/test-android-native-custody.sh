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
adb_command="$android_sdk/platform-tools/adb"
if [ ! -x "$adb_command" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi

device="${OXID_ANDROID_DEVICE:-$($adb_command devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')}"
case "$device" in
  emulator-*) ;;
  *)
    echo "Native custody automation changes the device PIN and therefore runs only on a disposable emulator." >&2
    exit 1
    ;;
esac

if [ "$($adb_command -s "$device" shell locksettings get-disabled | tr -d '\r')" != "true" ]; then
  echo "The emulator already has a device credential; refusing to replace it." >&2
  exit 1
fi

test_pin="${OXID_ANDROID_TEST_PIN:-246810}"
if ! [[ "$test_pin" =~ ^[0-9]{6,12}$ ]]; then
  echo "OXID_ANDROID_TEST_PIN must contain 6 to 12 digits." >&2
  exit 1
fi

devtools_port=9224
cleanup() {
  "$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
  "$adb_command" -s "$device" shell locksettings clear --old "$test_pin" >/dev/null 2>&1 || true
  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
}
trap cleanup EXIT

"$adb_command" -s "$device" shell locksettings set-pin "$test_pin" >/dev/null
OXID_MOBILE_CUSTODY=native OXID_ANDROID_DEVICE="$device" \
  "$repository_root/scripts/run-android-emulator.sh"
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2

credential_prompt_focused() {
  local focused
  focused="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
    | rg 'topResumedActivity|ResumedActivity' || true)"
  rg -q 'ConfirmDeviceCredential|ConfirmLockPassword|ConfirmLockPattern|Keyguard' <<<"$focused"
}

authorize_prompt() {
  for _attempt in $(seq 1 90); do
    if credential_prompt_focused; then
      echo "Android device-credential prompt observed on $device." >&2
      "$adb_command" -s "$device" shell input text "$test_pin" >/dev/null
      for _settle_attempt in $(seq 1 10); do
        if ! credential_prompt_focused; then
          return 0
        fi
        sleep 0.2
      done
      "$adb_command" -s "$device" shell input keyevent ENTER >/dev/null
      return 0
    fi
    sleep 1
  done
  echo "Android device-credential prompt did not appear." >&2
  return 1
}

run_webview_mode() {
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
    echo "Oxid WebView process did not become available." >&2
    return 1
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
    return 1
  fi
  local node_status=0
  node "$repository_root/tests/mobile/android-wallet-flow.mjs" "$websocket_url" "$mode" \
    || node_status=$?
  "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null
  return "$node_status"
}

authorize_prompt &
first_authorizer=$!
first_authorization="$(run_webview_mode native-authorize)"
wait "$first_authorizer"
if [ "$(jq -r '.securityAction' <<<"$first_authorization")" != "Initialize wallet" ]; then
  echo "Android native custody did not request first-use initialization." >&2
  exit 1
fi

sealed_name=""
for _attempt in $(seq 1 30); do
  sealed_name="$($adb_command -s "$device" shell run-as io.medianox.oxid \
    ls no_backup/oxid-custody-v1 2>/dev/null | tr -d '\r' || true)"
  if [[ "$sealed_name" =~ ^[0-9a-f]{64}\.json$ ]]; then
    break
  fi
  sleep 1
done
if ! [[ "$sealed_name" =~ ^[0-9a-f]{64}\.json$ ]]; then
  echo "Android native custody did not create one digest-named no-backup record." >&2
  exit 1
fi
profile_document="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  cat files/oxid/wallet-profiles.json 2>/dev/null || true)"
active_profile_id="$(jq -r '.activeProfileId // empty' <<<"$profile_document")"
expected_sealed_name="$(printf '%s' "$active_profile_id" | shasum -a 256 | awk '{print $1}').json"
if [ -z "$active_profile_id" ] || [ "$sealed_name" != "$expected_sealed_name" ]; then
  echo "Android native custody record is not bound to the active public profile." >&2
  exit 1
fi
sealed_record="$($adb_command -s "$device" exec-out run-as io.medianox.oxid \
  cat "no_backup/oxid-custody-v1/$sealed_name" 2>/dev/null || true)"
if ! jq -e '.version == 1 and (.protection == "operating_system" or .protection == "hardware_backed") and (.iv | length > 0) and (.ciphertext | length > 0)' \
  >/dev/null <<<"$sealed_record" || rg -q 'root_seed|secret|profile_' <<<"$sealed_record"; then
  echo "Android native custody did not retain only an opaque no-backup ciphertext record." >&2
  exit 1
fi
first_result="$(run_webview_mode native-custody)"
post_activation_record="$($adb_command -s "$device" exec-out run-as io.medianox.oxid \
  cat "no_backup/oxid-custody-v1/$sealed_name" 2>/dev/null || true)"
post_activation_record_hash="$(printf '%s' "$post_activation_record" | shasum -a 256 | awk '{print $1}')"

old_process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')"
"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
for _attempt in $(seq 1 30); do
  if [ -z "$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')" ]; then
    break
  fi
  sleep 1
done
if [ -n "$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')" ]; then
  echo "Android did not stop the native custody process before the restart check." >&2
  exit 1
fi
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2
new_process_id="$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')"
if [ -z "$old_process_id" ] || [ -z "$new_process_id" ] || [ "$old_process_id" = "$new_process_id" ]; then
  echo "Android did not establish a distinct native custody process after restart." >&2
  exit 1
fi
authorize_prompt &
second_authorizer=$!
second_authorization="$(run_webview_mode native-authorize)"
wait "$second_authorizer"
second_security_action="$(jq -r '.securityAction // empty' <<<"$second_authorization")"
if [ "$second_security_action" != "Unlock wallet" ]; then
  case "$second_security_action" in
    "") reported_action="missing" ;;
    "Initialize wallet") reported_action="initialize" ;;
    "already unlocked") reported_action="already-unlocked" ;;
    *) reported_action="unexpected" ;;
  esac
  echo "Android native custody did not require restart reauthorization (action: $reported_action)." >&2
  exit 1
fi
second_result="$(run_webview_mode native-restored)"
restored_profile_document="$($adb_command -s "$device" shell run-as io.medianox.oxid \
  cat files/oxid/wallet-profiles.json 2>/dev/null || true)"
restored_profile_id="$(jq -r '.activeProfileId // empty' <<<"$restored_profile_document")"
restored_record="$($adb_command -s "$device" exec-out run-as io.medianox.oxid \
  cat "no_backup/oxid-custody-v1/$sealed_name" 2>/dev/null || true)"
restored_record_hash="$(printf '%s' "$restored_record" | shasum -a 256 | awk '{print $1}')"

first_address="$(jq -r '.receiveAddress' <<<"$first_result")"
second_address="$(jq -r '.receiveAddress' <<<"$second_result")"
if [ -z "$first_address" ] || [ "$first_address" != "$second_address" ]; then
  profile_continuity=false
  sealed_record_continuity=false
  address_continuity=false
  [ -n "$active_profile_id" ] && [ "$active_profile_id" = "$restored_profile_id" ] && profile_continuity=true
  [ -n "$post_activation_record_hash" ] && [ "$post_activation_record_hash" = "$restored_record_hash" ] && sealed_record_continuity=true
  [ -n "$first_address" ] && [ "$first_address" = "$second_address" ] && address_continuity=true
  echo "Android native custody did not restore the same protected Midnight root (profile continuity: $profile_continuity; sealed record continuity: $sealed_record_continuity; address continuity: $address_continuity)." >&2
  exit 1
fi

echo "Android Keystore user-presence, no-backup ciphertext, lock, restart, and protected-root restoration passed on $device."
