#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/test-android-profile-flow.sh [--apk APK --receipt RECEIPT]

Without arguments, build and smoke the ordinary Android development target.
With --apk and --receipt, install and smoke that exact release-candidate APK
without rebuilding it. Both options are required together.
USAGE
}

prebuilt_apk=""
prebuilt_receipt=""
while (($# > 0)); do
  case "$1" in
    --apk)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      prebuilt_apk="$2"
      shift 2
      ;;
    --receipt)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      prebuilt_receipt="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done
if { [ -n "$prebuilt_apk" ] && [ -z "$prebuilt_receipt" ]; } || \
  { [ -z "$prebuilt_apk" ] && [ -n "$prebuilt_receipt" ]; }; then
  echo "--apk and --receipt must be supplied together." >&2
  exit 2
fi

for command_name in curl jq node rg od awk; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
if [ -n "$prebuilt_apk" ]; then
  case "$prebuilt_apk" in
    /*) ;;
    *) prebuilt_apk="$repository_root/$prebuilt_apk" ;;
  esac
  case "$prebuilt_receipt" in
    /*) ;;
    *) prebuilt_receipt="$repository_root/$prebuilt_receipt" ;;
  esac
fi

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
device=""
test_pin=""
credential_owned=0
app_state_owned=0
cleanup() {
  if [ -n "$device" ]; then
    "$adb_command" -s "$device" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
    if [ "$app_state_owned" -eq 1 ]; then
      "$adb_command" -s "$device" shell am force-stop io.medianox.oxid >/dev/null 2>&1 || true
      "$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null 2>&1 || true
    fi
    if [ "$credential_owned" -eq 1 ]; then
      "$adb_command" -s "$device" shell locksettings clear --old "$test_pin" >/dev/null 2>&1 || true
    fi
  fi
}
trap cleanup EXIT

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi

if [ -n "$device" ]; then
  if [ -n "$prebuilt_apk" ]; then
    OXID_ANDROID_DEVICE="$device" OXID_ANDROID_REQUIRE_EMULATOR=1 \
      OXID_ANDROID_PREBUILT_APK="$prebuilt_apk" \
      OXID_ANDROID_PREBUILT_RECEIPT="$prebuilt_receipt" \
      "$repository_root/scripts/run-android-emulator.sh" deploy
  else
    OXID_ANDROID_DEVICE="$device" OXID_ANDROID_REQUIRE_EMULATOR=1 OXID_ANDROID_JNI_RECOVERY_TEST=1 \
      "$repository_root/scripts/run-android-emulator.sh"
  fi
else
  if [ -n "$prebuilt_apk" ]; then
    OXID_ANDROID_REQUIRE_EMULATOR=1 OXID_ANDROID_PREBUILT_APK="$prebuilt_apk" \
      OXID_ANDROID_PREBUILT_RECEIPT="$prebuilt_receipt" \
      "$repository_root/scripts/run-android-emulator.sh" deploy
  else
    OXID_ANDROID_REQUIRE_EMULATOR=1 OXID_ANDROID_JNI_RECOVERY_TEST=1 \
      "$repository_root/scripts/run-android-emulator.sh"
  fi
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')"
fi
if [ -z "$device" ]; then
  echo "The Android smoke harness did not find an online device." >&2
  exit 1
fi
case "$device" in
  emulator-*) ;;
  *)
    echo "The profile smoke changes the device credential and runs only on a disposable emulator." >&2
    exit 1
    ;;
esac
if [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "1" ]; then
  echo "The selected Android target is not a disposable QEMU emulator." >&2
  exit 1
fi
if [ "$($adb_command -s "$device" shell locksettings get-disabled | tr -d '\r')" != "true" ]; then
  echo "The emulator already has a device credential; refusing to replace it." >&2
  exit 1
fi
test_pin="${OXID_ANDROID_TEST_PIN:-}"
if [ -z "$test_pin" ]; then
  test_pin="$(od -An -N4 -tu4 /dev/urandom | awk '{ printf "%06d", ($1 % 900000) + 100000 }')"
fi
if ! [[ "$test_pin" =~ ^[0-9]{6,12}$ ]]; then
  echo "OXID_ANDROID_TEST_PIN must contain 6 to 12 digits." >&2
  exit 1
fi
"$adb_command" -s "$device" shell locksettings set-pin "$test_pin" >/dev/null
credential_owned=1

echo "Resetting Android application data for the smoke flow."
"$adb_command" -s "$device" shell pm clear io.medianox.oxid >/dev/null
app_state_owned=1
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
    echo "Oxid WebView process did not become available." >&2
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

credential_prompt_focused() {
  local focused
  focused="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
    | rg 'topResumedActivity|ResumedActivity' || true)"
  rg -q 'ConfirmDeviceCredential|ConfirmLockPassword|ConfirmLockPattern|Keyguard' <<<"$focused"
}

resume_onboarding_after_authorization() {
  local resumed=""
  for _attempt in $(seq 1 20); do
    resumed="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg 'topResumedActivity|ResumedActivity' || true)"
    if rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$resumed"; then
      return 0
    fi
    if credential_prompt_focused; then
      sleep 0.2
      continue
    fi
    break
  done
  if credential_prompt_focused; then
    echo "Android device-credential prompt did not close after authorization." >&2
    return 1
  fi
  "$adb_command" -s "$device" shell am start -W \
    -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
  wait_for_main_activity
}

authorize_onboarding_prompt() {
  for _attempt in $(seq 1 90); do
    if credential_prompt_focused; then
      echo "Android device-credential prompt observed." >&2
      "$adb_command" -s "$device" shell input text "$test_pin" >/dev/null
      for _settle_attempt in $(seq 1 10); do
        if ! credential_prompt_focused; then
          resume_onboarding_after_authorization
          return
        fi
        sleep 0.2
      done
      "$adb_command" -s "$device" shell input keyevent ENTER >/dev/null
      resume_onboarding_after_authorization
      return
    fi
    sleep 1
  done
  echo "Android device-credential prompt did not appear." >&2
  return 1
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

background_to_android_home() {
  local resumed=""
  "$adb_command" -s "$device" shell am start -W \
    -a android.intent.action.MAIN \
    -c android.intent.category.HOME >/dev/null
  for _attempt in $(seq 1 50); do
    resumed="$("$adb_command" -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg 'topResumedActivity|ResumedActivity' || true)"
    if ! rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$resumed"; then
      return 0
    fi
    sleep 0.2
  done
  echo "Android did not background Oxid before the secret-mode lifecycle assertion." >&2
  return 1
}

dismiss_native_share_chooser() {
  local resumed=""
  for _attempt in $(seq 1 3); do
    resumed="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg 'topResumedActivity|ResumedActivity' || true)"
    if ! rg -q 'ResolverActivity|ChooserActivity|IntentResolverActivity' <<<"$resumed"; then
      wait_for_main_activity
      return
    fi
    "$adb_command" -s "$device" shell input keyevent BACK >/dev/null
    sleep 1
  done
  wait_for_main_activity
}

assert_screen_privacy_flag() {
  local expected="$1"
  local flag_line=""
  local flag_hex=""
  local flag_secure_set=0
  local window_state=""
  window_state="$($adb_command -s "$device" shell dumpsys window windows 2>/dev/null \
    | awk '
      /Window #[0-9]+ Window.*io\.medianox\.oxid\/dev\.dioxus\.main\.MainActivity/ {
        capture = 1
        lines = 0
      }
      capture {
        print
        lines += 1
        if (lines == 24) exit
      }
    ')"
  if [ -z "$window_state" ]; then
    echo "Android Oxid window was unavailable for screen-privacy inspection." >&2
    exit 1
  fi
  flag_line="$(rg '^[[:space:]]+fl=' <<<"$window_state" | head -1 || true)"
  if [ -z "$flag_line" ]; then
    echo "Android Oxid window flags were unavailable for screen-privacy inspection." >&2
    exit 1
  fi
  # AOSP emulator images expose symbolic names while recent Samsung Android 16
  # builds expose only a hexadecimal mask. Support both truthful dumpsys forms;
  # WindowManager.LayoutParams.FLAG_SECURE is bit 0x2000 in the numeric form.
  if rg -q '(^|[[:space:]])SECURE([[:space:]]|$)' <<<"$flag_line"; then
    flag_secure_set=1
  else
    flag_hex="$(rg -o 'fl=[0-9a-fA-F]+' <<<"$flag_line" | head -1 | cut -d= -f2 || true)"
    if [ -n "$flag_hex" ] && (( (0x$flag_hex & 0x2000) != 0 )); then
      flag_secure_set=1
    fi
  fi
  if [ "$expected" = "protected" ]; then
    if [ "$flag_secure_set" -ne 1 ]; then
      echo "Android secret mode did not set FLAG_SECURE." >&2
      exit 1
    fi
  elif [ "$flag_secure_set" -eq 1 ]; then
    echo "Android explicit reveal did not clear FLAG_SECURE." >&2
    exit 1
  fi
}

authorize_onboarding_prompt &
onboarding_authorizer=$!
run_webview_wallet_flow privacy-reveal
wait "$onboarding_authorizer"
assert_screen_privacy_flag unprotected
background_to_android_home
"$adb_command" -s "$device" shell am start -W \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
wait_for_main_activity
run_webview_wallet_flow privacy-rearmed
assert_screen_privacy_flag protected
run_webview_wallet_flow flow

chooser_state="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null || true)"
if ! rg -q 'ResolverActivity|ChooserActivity|IntentResolverActivity' <<<"$chooser_state"; then
  echo "Android public receive-address share did not open a native chooser." >&2
  exit 1
fi
dismiss_native_share_chooser
run_webview_wallet_flow close-receive

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

if [ -n "$prebuilt_apk" ]; then
  prebuilt_sha256="$(shasum -a 256 "$prebuilt_apk" | awk '{print $1}')"
  echo "Android exact-artifact profile smoke passed (sha256=$prebuilt_sha256)."
else
  echo "Android protected account, Digital Passport OpenID4VP proof gate/local reveal/disclosure preview/restore, DUST/shielded sync, receive QR/copy/share, cold/warm app links, transfer, and profile-restore smoke flow passed."
fi
