#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in nix rustup java; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

mobile_custody="${OXID_MOBILE_CUSTODY:-development}"
case "$mobile_custody" in
  development)
    mobile_features="mobile,standalone-development"
    ;;
  native)
    mobile_features="mobile,standalone-native-custody"
    ;;
  *)
    echo "OXID_MOBILE_CUSTODY must be 'development' or 'native'." >&2
    exit 1
    ;;
esac

ui_profile="${OXID_UI_PROFILE:-user}"
case "$ui_profile" in
  user)
    ;;
  dev)
    mobile_features="$mobile_features,ui-profile-dev"
    ;;
  *)
    echo "OXID_UI_PROFILE must be 'user' or 'dev'." >&2
    exit 1
    ;;
esac

android_jni_recovery_test="${OXID_ANDROID_JNI_RECOVERY_TEST:-0}"
case "$android_jni_recovery_test" in
  0)
    ;;
  1)
    mobile_features="$mobile_features,android-jni-exception-recovery-test"
    ;;
  *)
    echo "OXID_ANDROID_JNI_RECOVERY_TEST must be '0' or '1'." >&2
    exit 1
    ;;
esac

mobile_presentation_proving="${OXID_MOBILE_PRESENTATION_PROVING:-unavailable}"
presentation_artifacts_dir=""
case "$mobile_presentation_proving" in
  unavailable)
    ;;
  artifacts)
    if [ "$mobile_custody" != "native" ]; then
      echo "OXID_MOBILE_PRESENTATION_PROVING=artifacts requires OXID_MOBILE_CUSTODY=native." >&2
      exit 1
    fi
    mobile_features="$mobile_features,standalone-native-proving-artifacts"
    presentation_artifacts_dir="$(
      nix build .#presentation-compact-artifacts --no-link --print-out-paths
    )"
    ;;
  *)
    echo "OXID_MOBILE_PRESENTATION_PROVING must be 'unavailable' or 'artifacts'." >&2
    exit 1
    ;;
esac

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
if [ -z "$android_sdk" ] || [ ! -d "$android_sdk" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi

adb_command="$android_sdk/platform-tools/adb"
emulator_command="$android_sdk/emulator/emulator"
if [ ! -x "$adb_command" ]; then
  echo "Android platform-tools are missing from $android_sdk." >&2
  exit 1
fi

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')"
fi

if [ -z "$device" ]; then
  if [ ! -x "$emulator_command" ]; then
    echo "No Android device is connected and the SDK emulator is unavailable." >&2
    exit 1
  fi
  avd="${OXID_ANDROID_AVD:-$($emulator_command -list-avds | sed -n '1p')}"
  if [ -z "$avd" ]; then
    echo "No Android device or configured AVD was found." >&2
    exit 1
  fi

  "$emulator_command" -avd "$avd" -no-snapshot-save >/dev/null 2>&1 &
  for _attempt in $(seq 1 120); do
    device="$($adb_command devices | awk 'NR > 1 && $2 == "device" { print $1; exit }')"
    if [ -n "$device" ]; then
      break
    fi
    sleep 1
  done
fi

if [ -z "$device" ] || [ "$($adb_command -s "$device" get-state 2>/dev/null || true)" != "device" ]; then
  echo "Android device '$device' is not online." >&2
  exit 1
fi

for _attempt in $(seq 1 120); do
  if [ "$($adb_command -s "$device" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
    break
  fi
  sleep 1
done
if [ "$($adb_command -s "$device" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; then
  echo "Android device '$device' did not finish booting." >&2
  exit 1
fi

case "$($adb_command -s "$device" shell getprop ro.product.cpu.abi | tr -d '\r')" in
  arm64-v8a)
    rust_target="aarch64-linux-android"
    ;;
  x86_64)
    rust_target="x86_64-linux-android"
    ;;
  *)
    echo "The connected Android ABI is not supported by this smoke script." >&2
    exit 1
    ;;
esac

android_ndk="${ANDROID_NDK_HOME:-}"
if [ -z "$android_ndk" ] && [ -d "$android_sdk/ndk" ]; then
  android_ndk="$(find "$android_sdk/ndk" -mindepth 1 -maxdepth 1 -type d | sort | tail -1)"
fi
if [ -z "$android_ndk" ] && [ -d "$android_sdk/ndk-bundle" ]; then
  android_ndk="$android_sdk/ndk-bundle"
fi
if [ -z "$android_ndk" ] || [ ! -d "$android_ndk" ]; then
  echo "Install an Android NDK or set ANDROID_NDK_HOME." >&2
  exit 1
fi

rustup target add "$rust_target"
rust_toolchain_bin="$(dirname -- "$(rustup which cargo)")"
dioxus_output="$(nix build .#dioxus-cli --no-link --print-out-paths)"
dioxus_cli="$dioxus_output/bin/dx"

ANDROID_HOME="$android_sdk" \
ANDROID_SDK_ROOT="$android_sdk" \
ANDROID_NDK_HOME="$android_ndk" \
OXID_PRESENTATION_ARTIFACTS_DIR="$presentation_artifacts_dir" \
PATH="$rust_toolchain_bin:$android_sdk/platform-tools:/usr/bin:$PATH" \
  "$dioxus_cli" build \
    --android \
    --package oxid-app \
    --no-default-features \
    --features "$mobile_features" \
    --target "$rust_target" \
    --locked

apk="$repository_root/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
if [ ! -f "$apk" ]; then
  echo "Dioxus did not create the expected APK: $apk" >&2
  exit 1
fi
if [ "$mobile_presentation_proving" = "artifacts" ]; then
  packaged_bytes="$(wc -c < "$apk" | tr -d ' ')"
  echo "Authenticated Compact artifact measurement APK: $packaged_bytes bytes."
fi

"$adb_command" -s "$device" install -r "$apk"
"$adb_command" -s "$device" shell am force-stop io.medianox.oxid
"$adb_command" -s "$device" shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2
if [ -z "$($adb_command -s "$device" shell pidof io.medianox.oxid | tr -d '\r')" ]; then
  echo "Oxid did not remain running on Android device '$device'." >&2
  exit 1
fi

echo "Launched io.medianox.oxid ($ui_profile profile, $mobile_custody custody) on Android device $device."
