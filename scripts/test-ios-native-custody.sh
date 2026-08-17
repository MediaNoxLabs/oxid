#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS native-custody test requires macOS and Xcode." >&2
  exit 1
fi
for command_name in nix jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
device="${OXID_IOS_DEVICE:-}"
if [ -z "$device" ]; then
  device="$(
    /usr/bin/xcrun simctl list devices booted -j \
      | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty'
  )"
fi
if [ -z "$device" ]; then
  echo "Boot an iPhone simulator or set OXID_IOS_DEVICE." >&2
  exit 1
fi

OXID_MOBILE_CUSTODY=native OXID_IOS_DEVICE="$device" OXID_IOS_RESET_DATA=1 \
  "$repository_root/scripts/run-ios-simulator.sh"

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)"
generated_project_root="$repository_root/target/mobile-tests/ios-native-custody"
mkdir -p "$generated_project_root"
OXID_REPOSITORY_ROOT="$repository_root" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$repository_root/tests/mobile/ios/project.yml" \
    --project "$generated_project_root"

xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
host_user="$(id -un)"
env -i \
  "DEVELOPER_DIR=$xcode_developer_dir" \
  "HOME=$HOME" \
  "LANG=${LANG:-en_US.UTF-8}" \
  "LOGNAME=$host_user" \
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
  "TMPDIR=${TMPDIR:-/tmp}" \
  "USER=$host_user" \
  /usr/bin/xcodebuild test \
  -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
  -scheme OxidUITests \
  -destination "platform=iOS Simulator,id=$device" \
  -derivedDataPath "$repository_root/target/mobile-tests/ios-native-custody-derived-data" \
  -only-testing:OxidUITests/NativeCustodyTests/testNativeCompositionUsesDeviceCustodyOrFailsClosed \
  CODE_SIGNING_ALLOWED=NO

echo "iOS native-custody capability/fail-closed smoke passed on $device."
