#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for command_name in nix rustup java node; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
origin_policy="$repository_root/scripts/e2e/tailnet-origin-policy.mjs"
cd "$repository_root"

portal_profile_authority_directory=""
cleanup_portal_profile_authority() {
  if [ -n "$portal_profile_authority_directory" ]; then
    rm -rf -- "$portal_profile_authority_directory"
  fi
}
trap cleanup_portal_profile_authority EXIT

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
  demo)
    if [ "$mobile_custody" != "development" ]; then
      echo "OXID_UI_PROFILE=demo requires OXID_MOBILE_CUSTODY=development." >&2
      exit 1
    fi
    mobile_features="$mobile_features,ui-profile-demo"
    ;;
  *)
    echo "OXID_UI_PROFILE must be 'user', 'dev', or 'demo'." >&2
    exit 1
    ;;
esac

standalone_network_profile="${OXID_STANDALONE_NETWORK_PROFILE:-simulated}"
requested_portal_profile="${OXID_MOBILE_PORTAL_PROFILE:-unavailable}"
case "$standalone_network_profile" in
  simulated)
    ;;
  local)
    if [ "$mobile_custody" != "development" ]; then
      echo "OXID_STANDALONE_NETWORK_PROFILE=local requires development custody." >&2
      exit 1
    fi
    mobile_features="$mobile_features,standalone-local"
    ;;
  tailnet)
    if [ "$mobile_custody" != "development" ]; then
      echo "OXID_STANDALONE_NETWORK_PROFILE=tailnet requires development custody." >&2
      exit 1
    fi
    for build_value in \
      OXID_BUILD_MIDNIGHT_INDEXER_WS_URL \
      OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL \
      OXID_BUILD_MIDNIGHT_NODE_WS_URL \
      OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL; do
      if [ -z "${!build_value:-}" ]; then
        echo "$build_value is required for the tailnet build profile." >&2
        exit 1
      fi
    done
    if [ "$requested_portal_profile" != "tailnet-android" ]; then
      mobile_features="$mobile_features,standalone-tailnet"
    fi
    ;;
  *)
    echo "OXID_STANDALONE_NETWORK_PROFILE must be 'simulated', 'local', or 'tailnet'." >&2
    exit 1
    ;;
esac

if [ "$ui_profile" = "demo" ] && [ "$standalone_network_profile" != "simulated" ]; then
  echo "OXID_UI_PROFILE=demo requires the simulated development composition." >&2
  exit 1
fi

portal_profile="${OXID_MOBILE_PORTAL_PROFILE:-unavailable}"
portal_manifest_path=""
portal_manifest_sha256=""
portal_profile_authority_path=""
portal_profile_authority_sha256=""
portal_public_origin=""
portal_authority_profile="local"
case "$portal_profile" in
  unavailable)
    ;;
  local)
    if [ "$mobile_custody" != "development" ] || \
      [ "$standalone_network_profile" != "local" ]; then
      echo "OXID_MOBILE_PORTAL_PROFILE=local requires the standalone-local development profile." >&2
      exit 1
    fi
    portal_manifest_path="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH:-}"
    portal_manifest_sha256="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256:-}"
    if [[ "$portal_manifest_path" != /* ]] || [ ! -f "$portal_manifest_path" ] || \
      [ -L "$portal_manifest_path" ]; then
      echo "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH must name an absolute regular non-symlink file." >&2
      exit 1
    fi
    if ! [[ "$portal_manifest_sha256" =~ ^[0-9a-f]{64}$ ]]; then
      echo "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256 must be lowercase SHA-256." >&2
      exit 1
    fi
    actual_manifest_sha256="$(shasum -a 256 "$portal_manifest_path" | awk '{print $1}')"
    if [ "$actual_manifest_sha256" != "$portal_manifest_sha256" ]; then
      echo "The Portal deployment manifest digest does not match." >&2
      exit 1
    fi
    mobile_features="$mobile_features,standalone-portal"
    ;;
  tailnet-android)
    if [ "$mobile_custody" != "development" ] || \
      [ "$standalone_network_profile" != "tailnet" ]; then
      echo "OXID_MOBILE_PORTAL_PROFILE=tailnet-android requires the standalone tailnet development profile." >&2
      exit 1
    fi
    portal_public_origin="${OXID_BUILD_PORTAL_PUBLIC_ORIGIN:-}"
    if ! OXID_TAILNET_ORIGIN_POLICY_INPUT="$portal_public_origin" \
      node "$origin_policy" --origin-env; then
      echo "The Android Portal profile requires a canonical HTTPS MagicDNS origin with an explicit listener." >&2
      exit 1
    fi
    public_host="${portal_public_origin#https://}"
    public_port="${public_host##*:}"
    public_host="${public_host%:*}"
    if [ "$public_port" -lt 1024 ] || [ "$public_port" -gt 65535 ] || [[ "$public_port" =~ ^(8443|10000)$ ]]; then
      echo "The Android Portal listener conflicts with a protected standalone route." >&2
      exit 1
    fi
    [ "${OXID_BUILD_MIDNIGHT_INDEXER_WS_URL:-}" = "wss://$public_host:8443/api/v4/graphql/ws" ] && \
      [ "${OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL:-}" = "https://$public_host:8443/api/v4/graphql" ] && \
      [ "${OXID_BUILD_MIDNIGHT_NODE_WS_URL:-}" = "wss://$public_host:10000" ] && \
      [ "${OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL:-}" = "https://$public_host" ] || {
      echo "The Android physical Portal profile requires the exact authenticated Midnight tailnet URLs." >&2
      exit 1
    }
    portal_manifest_path="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH:-}"
    portal_manifest_sha256="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256:-}"
    if [[ "$portal_manifest_path" != /* ]] || [ ! -f "$portal_manifest_path" ] || \
      [ -L "$portal_manifest_path" ] || ! [[ "$portal_manifest_sha256" =~ ^[0-9a-f]{64}$ ]] || \
      [ "$(shasum -a 256 "$portal_manifest_path" | awk '{print $1}')" != "$portal_manifest_sha256" ]; then
      echo "The Portal deployment manifest path or digest is invalid." >&2
      exit 1
    fi
    portal_authority_profile="tailnet-android"
    mobile_features="$mobile_features,standalone-portal-tailnet"
    ;;
  *)
    echo "OXID_MOBILE_PORTAL_PROFILE must be 'unavailable', 'local', or 'tailnet-android'." >&2
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

adb_call() {
  if [ -n "$adb_timeout_seconds" ]; then
    timeout -k 5s "${adb_timeout_seconds}s" "$adb_command" "$@"
  else
    "$adb_command" "$@"
  fi
}

first_online_device() {
  if [ "$standalone_network_profile" = "local" ]; then
    adb_call devices | awk 'NR > 1 && $2 == "device" && $1 ~ /^emulator-/ { print $1; exit }'
  else
    adb_call devices | awk 'NR > 1 && $2 == "device" { print $1; exit }'
  fi
}

avd_definition_exists() {
  local candidate="$1"
  local avd_root
  if [ -n "${ANDROID_AVD_HOME:-}" ] && [ -f "$ANDROID_AVD_HOME/$candidate.ini" ]; then
    return 0
  fi
  if [ -n "${ANDROID_SDK_HOME:-}" ] && [ -f "$ANDROID_SDK_HOME/avd/$candidate.ini" ]; then
    return 0
  fi
  for avd_root in "$HOME/.android/avd"; do
    if [ -f "$avd_root/$candidate.ini" ]; then
      return 0
    fi
  done
  return 1
}

first_configured_avd() {
  local candidate
  while IFS= read -r candidate; do
    if [ -n "$candidate" ] && avd_definition_exists "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done < <("$emulator_command" -list-avds)
  return 1
}

device="${OXID_ANDROID_DEVICE:-}"
adb_timeout_seconds="${OXID_ANDROID_ADB_TIMEOUT_SECONDS:-}"
if [ -n "$adb_timeout_seconds" ]; then
  if ! [[ "$adb_timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "OXID_ANDROID_ADB_TIMEOUT_SECONDS must be a positive integer." >&2
    exit 1
  fi
  if ! command -v timeout >/dev/null 2>&1; then
    echo "OXID_ANDROID_ADB_TIMEOUT_SECONDS requires coreutils timeout." >&2
    exit 1
  fi
fi
adb_device() {
  if [ -n "$adb_timeout_seconds" ]; then
    timeout -k 5s "${adb_timeout_seconds}s" env ANDROID_SERIAL="$device" "$adb_command" "$@"
  else
    ANDROID_SERIAL="$device" "$adb_command" "$@"
  fi
}
if [ -z "$device" ]; then
  device="$(first_online_device)"
fi

if [ -z "$device" ]; then
  if [ ! -x "$emulator_command" ]; then
    echo "No Android device is connected and the SDK emulator is unavailable." >&2
    exit 1
  fi
  avd="${OXID_ANDROID_AVD:-}"
  if [ -z "$avd" ]; then
    avd="$(first_configured_avd || true)"
  fi
  if [ -z "$avd" ]; then
    echo "No Android device or configured AVD was found." >&2
    exit 1
  fi
  if ! avd_definition_exists "$avd"; then
    echo "Android AVD '$avd' has no configuration file in a reviewed AVD directory." >&2
    exit 1
  fi

  emulator_log="$repository_root/target/mobile-tests/android-emulator-launch.log"
  mkdir -p "$(dirname -- "$emulator_log")"
  : >"$emulator_log"
  nohup "$emulator_command" -avd "$avd" -no-snapshot-save \
    </dev/null >"$emulator_log" 2>&1 &
  emulator_process=$!
  for _attempt in $(seq 1 120); do
    device="$(first_online_device)"
    if [ -n "$device" ]; then
      break
    fi
    if ! kill -0 "$emulator_process" 2>/dev/null; then
      echo "Android AVD '$avd' exited before becoming available." >&2
      sed -n '1,120p' "$emulator_log" >&2
      exit 1
    fi
    sleep 1
  done
fi

if [ -z "$device" ] || [ "$(adb_device get-state 2>/dev/null || true)" != "device" ]; then
  echo "The selected Android device is not online." >&2
  exit 1
fi

for _attempt in $(seq 1 120); do
  if [ "$(adb_device shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
    break
  fi
  sleep 1
done
if [ "$(adb_device shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; then
  echo "The selected Android device did not finish booting." >&2
  exit 1
fi

if [ "$portal_profile" = "tailnet-android" ] && \
  { [[ "$device" = emulator-* ]] || \
    [ "$(adb_device shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "0" ]; }; then
  echo "The Android physical Portal profile requires a non-QEMU device." >&2
  exit 1
fi

if [ "$standalone_network_profile" = "local" ]; then
  if [[ "$device" != emulator-* ]] || \
    [ "$(adb_device shell getprop ro.kernel.qemu 2>/dev/null | tr -d '\r')" != "1" ]; then
    echo "The local standalone profile requires an Android emulator; use the tailnet profile for a physical phone." >&2
    exit 1
  fi
  reverse_ports=(8088 9944 6300)
  if [ "$portal_profile" = "local" ]; then
    # 18091 is owned only by the virtual-mobile single-use offer harness; the
    # physical suite keeps its unpublished control API on 18095.
    reverse_ports+=(18090 18091 18093)
  fi
  reverse_owner_receipt_matches_ports() {
    local receipt="${OXID_ANDROID_REVERSE_OWNER_RECEIPT:-}" mode
    [ -n "$receipt" ] && [[ "$receipt" = /* ]] && [ -f "$receipt" ] && [ ! -L "$receipt" ] || return 1
    if mode="$(stat -c '%a' "$receipt" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' "$receipt")"; fi
    [ "$mode" = 600 ] || return 1
    awk -v ports="${reverse_ports[*]}" '
      BEGIN {
        split(ports, values, " ")
        for (i in values) expected[values[i]] = 1
      }
      NR == 1 { if ($0 != "android-adb-reverse-owner-v1") invalid = 1; next }
      {
        if ($0 !~ /^[1-9][0-9]{0,4}$/ || !($0 in expected) || ++seen[$0] != 1) invalid = 1
      }
      END {
        for (port in expected) if (seen[port] != 1) invalid = 1
        exit invalid ? 1 : 0
      }
    ' "$receipt"
  }
  if [ -n "${OXID_ANDROID_REVERSE_OWNER_RECEIPT:-}" ]; then
    if ! reverse_owner_receipt_matches_ports; then
      echo "The Android reverse-owner receipt is not an exact private mapping receipt." >&2
      exit 1
    fi
  else
    for local_port in "${reverse_ports[@]}"; do
      adb_device reverse "tcp:$local_port" "tcp:$local_port"
    done
  fi
  reverse_list="$(adb_device reverse --list)"
  for local_port in "${reverse_ports[@]}"; do
    if ! awk -v serial="$device" -v route="tcp:$local_port" \
      '$1 == serial && $2 == route && $3 == route { found = 1 } END { exit !found }' <<<"$reverse_list"; then
      echo "Android emulator reverse route tcp:$local_port was not installed." >&2
      exit 1
    fi
  done
fi

case "$(adb_device shell getprop ro.product.cpu.abi | tr -d '\r')" in
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
if [ "$portal_profile" != "unavailable" ]; then
  portal_profile_authority_directory="$(mktemp -d "${TMPDIR:-/tmp}/oxid-portal-profile-android.XXXXXX")"
  chmod 700 "$portal_profile_authority_directory"
  portal_profile_authority_path="$portal_profile_authority_directory/authority.json"
  authority_platform="android_qemu"
  if [ "$portal_profile" = "tailnet-android" ]; then
    authority_platform="android_physical"
  fi
  "$repository_root/scripts/e2e/write-portal-profile-authority.sh" \
    "$authority_platform" "$rust_target" "$portal_profile_authority_path" "$portal_authority_profile"
  portal_profile_authority_sha256="$(shasum -a 256 "$portal_profile_authority_path" | awk '{print $1}')"
fi
rust_toolchain_bin="$(dirname -- "$(rustup which cargo)")"
dioxus_output="$(nix build .#dioxus-cli --no-link --print-out-paths)"
dioxus_cli="$dioxus_output/bin/dx"

ANDROID_HOME="$android_sdk" \
ANDROID_SDK_ROOT="$android_sdk" \
ANDROID_NDK_HOME="$android_ndk" \
OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$portal_manifest_path" \
OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$portal_manifest_sha256" \
OXID_BUILD_PORTAL_PROFILE_AUTHORITY_PATH="$portal_profile_authority_path" \
OXID_BUILD_PORTAL_PROFILE_AUTHORITY_SHA256="$portal_profile_authority_sha256" \
OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$portal_public_origin" \
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

adb_device install -r "$apk"
adb_device shell am force-stop io.medianox.oxid
adb_device shell am start \
  -n io.medianox.oxid/dev.dioxus.main.MainActivity >/dev/null
sleep 2
if [ -z "$(adb_device shell pidof io.medianox.oxid | tr -d '\r')" ]; then
  echo "Oxid did not remain running on Android." >&2
  exit 1
fi

echo "Launched io.medianox.oxid ($ui_profile profile, $mobile_custody custody, $standalone_network_profile network, $portal_profile Portal) on Android."
