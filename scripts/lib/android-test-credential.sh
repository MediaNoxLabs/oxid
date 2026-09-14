#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Shared disposable-emulator credential ceremony for Android smoke harnesses.
# Call prepare once, run authorize in the background before the WebView journey,
# and call cleanup from the owning harness trap.

oxid_android_test_credential_require_emulator() {
  local adb_command="$1"
  local device="$2"

  case "$device" in
    emulator-*) ;;
    *)
      echo "The Android credential smoke helper refuses non-emulator device '$device'." >&2
      return 1
      ;;
  esac
  if [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "1" ]; then
    echo "The Android credential smoke helper requires a disposable QEMU emulator." >&2
    return 1
  fi
}

oxid_android_test_credential_prepare() {
  local adb_command="$1"
  local device="$2"
  local recovery_script

  oxid_android_test_credential_require_emulator "$adb_command" "$device"
  if [ "$($adb_command -s "$device" shell locksettings get-disabled | tr -d '\r')" != "true" ]; then
    echo "The emulator already has a device credential; refusing to replace it." >&2
    return 1
  fi

  oxid_android_test_credential_pin="${OXID_ANDROID_TEST_PIN:-}"
  if [ -z "$oxid_android_test_credential_pin" ]; then
    oxid_android_test_credential_pin="$(od -An -N4 -tu4 /dev/urandom | awk '{ printf "%06d", ($1 % 900000) + 100000 }')"
  fi
  if ! [[ "$oxid_android_test_credential_pin" =~ ^[0-9]{6,12}$ ]]; then
    echo "OXID_ANDROID_TEST_PIN must contain 6 to 12 digits." >&2
    return 1
  fi

  oxid_android_test_credential_recovery_directory="$(
    umask 077
    mktemp -d "${TMPDIR:-/tmp}/oxid-android-test-credential.XXXXXX"
  )"
  chmod 700 "$oxid_android_test_credential_recovery_directory"
  recovery_script="$oxid_android_test_credential_recovery_directory/clear-owned-credential.sh"
  {
    printf '#!/usr/bin/env bash\nset -eu\n'
    printf '%q -s %q shell locksettings clear --old %q >/dev/null\n' \
      "$adb_command" "$device" "$oxid_android_test_credential_pin"
    printf 'rm -f -- %q\nrmdir -- %q\n' \
      "$recovery_script" "$oxid_android_test_credential_recovery_directory"
  } >"$recovery_script"
  chmod 700 "$recovery_script"
  oxid_android_test_credential_recovery_script="$recovery_script"

  if ! "$adb_command" -s "$device" shell locksettings set-pin \
    "$oxid_android_test_credential_pin" >/dev/null; then
    rm -f -- "$oxid_android_test_credential_recovery_script"
    rmdir -- "$oxid_android_test_credential_recovery_directory"
    oxid_android_test_credential_recovery_script=""
    oxid_android_test_credential_recovery_directory=""
    oxid_android_test_credential_pin=""
    return 1
  fi
  oxid_android_test_credential_owned=1
}

oxid_android_test_credential_prompt_focused() {
  local adb_command="$1"
  local device="$2"
  local focused
  focused="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
    | rg 'topResumedActivity|ResumedActivity' || true)"
  rg -q 'ConfirmDeviceCredential|ConfirmLockPassword|ConfirmLockPattern|Keyguard' <<<"$focused"
}

oxid_android_test_credential_resume_app() {
  local adb_command="$1"
  local device="$2"
  local resumed

  for _oxid_resume_attempt in $(seq 1 20); do
    resumed="$($adb_command -s "$device" shell dumpsys activity activities 2>/dev/null \
      | rg 'topResumedActivity|ResumedActivity' || true)"
    if rg -q 'io\.medianox\.oxid/dev\.dioxus\.main\.MainActivity' <<<"$resumed"; then
      return 0
    fi
    if oxid_android_test_credential_prompt_focused "$adb_command" "$device"; then
      sleep 0.2
      continue
    fi
    break
  done
  if oxid_android_test_credential_prompt_focused "$adb_command" "$device"; then
    echo "Android device-credential prompt did not close after authorization." >&2
    return 1
  fi
  "$adb_command" -s "$device" shell am start -W \
    -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
}

oxid_android_test_credential_authorize() {
  local adb_command="$1"
  local device="$2"

  for _oxid_attempt in $(seq 1 90); do
    if oxid_android_test_credential_prompt_focused "$adb_command" "$device"; then
      echo "Android device-credential prompt observed." >&2
      sleep 1
      "$adb_command" -s "$device" shell input text \
        "$oxid_android_test_credential_pin" >/dev/null
      "$adb_command" -s "$device" shell input keyevent ENTER >/dev/null
      for _oxid_settle_attempt in $(seq 1 50); do
        if ! oxid_android_test_credential_prompt_focused "$adb_command" "$device"; then
          oxid_android_test_credential_resume_app "$adb_command" "$device"
          return
        fi
        sleep 0.2
      done
      echo "Android device-credential prompt remained open after PIN submission." >&2
      return 1
    fi
    sleep 1
  done
  echo "Android device-credential prompt did not appear." >&2
  return 1
}

oxid_android_test_credential_cleanup() {
  local adb_command="$1"
  local device="$2"
  if [ "${oxid_android_test_credential_owned:-0}" -eq 1 ]; then
    if ! "$adb_command" -s "$device" shell locksettings clear \
      --old "$oxid_android_test_credential_pin" >/dev/null 2>&1; then
      echo "Failed to remove the disposable emulator PIN." >&2
      echo "Run the private recovery helper retained at: $oxid_android_test_credential_recovery_script" >&2
      echo "Credential ownership remains recorded; the credential and device identifier were not printed." >&2
      return 1
    fi
    oxid_android_test_credential_owned=0
    oxid_android_test_credential_pin=""
    rm -f -- "$oxid_android_test_credential_recovery_script"
    rmdir -- "$oxid_android_test_credential_recovery_directory"
    oxid_android_test_credential_recovery_script=""
    oxid_android_test_credential_recovery_directory=""
  fi
}
