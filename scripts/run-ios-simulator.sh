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
rust_toolchain_bin="$(dirname -- "$(rustup which cargo)")"
dioxus_output="$(nix build .#dioxus-cli --no-link --print-out-paths)"
dioxus_cli="$dioxus_output/bin/dx"
xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
simulator_sdk_root="$(
  env -u SDKROOT DEVELOPER_DIR="$xcode_developer_dir" \
    /usr/bin/xcrun --sdk iphonesimulator --show-sdk-path
)"

PATH="$rust_toolchain_bin:/usr/bin:$PATH" \
  DEVELOPER_DIR="$xcode_developer_dir" \
  SDKROOT="$simulator_sdk_root" \
  "$dioxus_cli" build \
    --ios \
    --package oxid-app \
    --no-default-features \
    --features mobile,standalone-development \
    --target "$rust_target" \
    --locked

app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
if [ ! -d "$app_bundle" ]; then
  echo "Dioxus did not create the expected app bundle: $app_bundle" >&2
  exit 1
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

echo "Launched $bundle_identifier on simulator $device."
