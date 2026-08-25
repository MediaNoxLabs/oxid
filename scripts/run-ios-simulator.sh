#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS simulator requires macOS and Xcode." >&2
  exit 1
fi

for command_name in nix rustup jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

if [ ! -x /usr/bin/xcrun ] || [ ! -x /usr/bin/open ] || [ ! -x /usr/bin/plutil ]; then
  echo "Xcode command-line tools are required." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
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

standalone_network_profile="${OXID_STANDALONE_NETWORK_PROFILE:-simulated}"
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
    mobile_features="$mobile_features,standalone-tailnet"
    ;;
  *)
    echo "OXID_STANDALONE_NETWORK_PROFILE must be 'simulated', 'local', or 'tailnet' for iOS Simulator." >&2
    exit 1
    ;;
esac

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
  tailnet-ios-simulator)
    if [ "$mobile_custody" != "development" ] || \
      [ "$standalone_network_profile" != "tailnet" ]; then
      echo "OXID_MOBILE_PORTAL_PROFILE=tailnet-ios-simulator requires the standalone-tailnet development profile." >&2
      exit 1
    fi
    portal_public_origin="${OXID_BUILD_PORTAL_PUBLIC_ORIGIN:-}"
    if ! [[ "$portal_public_origin" =~ ^https://([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.ts\.net:9443$ ]]; then
      echo "OXID_BUILD_PORTAL_PUBLIC_ORIGIN must be an exact HTTPS MagicDNS origin on port 9443." >&2
      exit 1
    fi
    public_host="${portal_public_origin#https://}"
    public_host="${public_host%:9443}"
    [ "${OXID_BUILD_MIDNIGHT_INDEXER_WS_URL:-}" = "wss://$public_host:8443/api/v4/graphql/ws" ] && \
      [ "${OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL:-}" = "https://$public_host:8443/api/v4/graphql" ] && \
      [ "${OXID_BUILD_MIDNIGHT_NODE_WS_URL:-}" = "wss://$public_host:10000" ] && \
      [ "${OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL:-}" = "https://$public_host" ] || {
      echo "The iOS Portal tailnet profile requires the existing exact Midnight tailnet URLs." >&2
      exit 1
    }
    portal_manifest_path="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH:-}"
    portal_manifest_sha256="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256:-}"
    if [[ "$portal_manifest_path" != /* ]] || [ ! -f "$portal_manifest_path" ] || \
      [ -L "$portal_manifest_path" ]; then
      echo "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH must name an absolute regular non-symlink file." >&2
      exit 1
    fi
    if ! [[ "$portal_manifest_sha256" =~ ^[0-9a-f]{64}$ ]] || \
      [ "$(shasum -a 256 "$portal_manifest_path" | awk '{print $1}')" != "$portal_manifest_sha256" ]; then
      echo "The Portal deployment manifest digest is invalid or does not match." >&2
      exit 1
    fi
    portal_authority_profile="tailnet-ios-simulator"
    mobile_features="$mobile_features,standalone-portal-tailnet-ios-simulator"
    ;;
  *)
    echo "OXID_MOBILE_PORTAL_PROFILE must be 'unavailable', 'local', or 'tailnet-ios-simulator'." >&2
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
    if [ "$mobile_custody" != "development" ] || \
      [ "$standalone_network_profile" != "simulated" ]; then
      echo "OXID_UI_PROFILE=demo requires the simulated development composition." >&2
      exit 1
    fi
    mobile_features="$mobile_features,ui-profile-demo"
    ;;
  *)
    echo "OXID_UI_PROFILE must be 'user', 'dev', or 'demo'." >&2
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

case "$(uname -m)" in
  arm64)
    rust_target="aarch64-apple-ios-sim"
    ;;
  x86_64)
    rust_target="x86_64-apple-ios"
    ;;
  *)
    echo "Unsupported macOS architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

rustup target add "$rust_target"
if [ "$portal_profile" != "unavailable" ]; then
  portal_profile_authority_directory="$(mktemp -d "${TMPDIR:-/tmp}/oxid-portal-profile-ios.XXXXXX")"
  chmod 700 "$portal_profile_authority_directory"
  portal_profile_authority_path="$portal_profile_authority_directory/authority.json"
  "$repository_root/scripts/e2e/write-portal-profile-authority.sh" \
    ios_simulator "$rust_target" "$portal_profile_authority_path" "$portal_authority_profile"
  portal_profile_authority_sha256="$(shasum -a 256 "$portal_profile_authority_path" | awk '{print $1}')"
fi
rust_toolchain_bin="$(dirname -- "$(rustup which cargo)")"
dioxus_output="$(nix build .#dioxus-cli --no-link --print-out-paths)"
dioxus_cli="$dioxus_output/bin/dx"
xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"

PATH="$rust_toolchain_bin:/usr/bin:$PATH" \
  DEVELOPER_DIR="$xcode_developer_dir" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$portal_manifest_path" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$portal_manifest_sha256" \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_PATH="$portal_profile_authority_path" \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_SHA256="$portal_profile_authority_sha256" \
  OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$portal_public_origin" \
  OXID_PRESENTATION_ARTIFACTS_DIR="$presentation_artifacts_dir" \
  env -u SDKROOT \
  "$dioxus_cli" build \
    --ios \
    --package oxid-app \
    --no-default-features \
    --features "$mobile_features" \
    --target "$rust_target" \
    --locked

app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
if [ ! -d "$app_bundle" ]; then
  echo "Dioxus did not create the expected app bundle: $app_bundle" >&2
  exit 1
fi
if [ "$mobile_presentation_proving" = "artifacts" ]; then
  packaged_bytes="$(find "$app_bundle" -type f -exec /usr/bin/stat -f '%z' {} + | awk '{ total += $1 } END { print total + 0 }')"
  echo "Authenticated Compact artifact measurement bundle: $packaged_bytes uncompressed bytes."
fi

device="${OXID_IOS_DEVICE:-}"
if [ -z "$device" ]; then
  device="$(
    /usr/bin/xcrun simctl list devices booted -j \
      | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty'
  )"
fi
if [ -z "$device" ]; then
  device="$(
    /usr/bin/xcrun simctl list devices available -j \
      | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty'
  )"
fi
if [ -z "$device" ]; then
  echo "No available iPhone simulator was found." >&2
  exit 1
fi

device_state="$(
  /usr/bin/xcrun simctl list devices -j \
    | jq -r --arg device "$device" 'first(.devices[][] | select(.udid == $device) | .state) // empty'
)"
if [ -z "$device_state" ]; then
  echo "OXID_IOS_DEVICE does not identify an installed simulator: $device" >&2
  exit 1
fi
if [ "$device_state" != "Booted" ]; then
  /usr/bin/xcrun simctl boot "$device"
fi

/usr/bin/open -a Simulator --args -CurrentDeviceUDID "$device"
/usr/bin/xcrun simctl bootstatus "$device" -b
if [ "${OXID_IOS_RESET_DATA:-0}" = "1" ]; then
  /usr/bin/xcrun simctl uninstall "$device" io.medianox.oxid >/dev/null 2>&1 || true
fi
/usr/bin/xcrun simctl install "$device" "$app_bundle"

bundle_identifier="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$app_bundle/Info.plist")"
/usr/bin/xcrun simctl terminate "$device" "$bundle_identifier" >/dev/null 2>&1 || true
/usr/bin/xcrun simctl launch "$device" "$bundle_identifier"

echo "Launched $bundle_identifier ($ui_profile profile, $mobile_custody custody, $standalone_network_profile network, $portal_profile Portal) on simulator $device."
