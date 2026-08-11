#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS UI smoke test requires macOS and Xcode." >&2
  exit 1
fi

for command_name in nix jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done
if [ ! -x /usr/bin/xcodebuild ] || [ ! -x /usr/bin/xcrun ]; then
  echo "Xcode is required for the iOS UI smoke test." >&2
  exit 1
fi

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

OXID_IOS_DEVICE="$device" OXID_IOS_RESET_DATA=1 \
  "$repository_root/scripts/run-ios-simulator.sh"

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)"
generated_project_root="$repository_root/target/mobile-tests/ios"
mkdir -p "$generated_project_root"
OXID_REPOSITORY_ROOT="$repository_root" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$repository_root/tests/mobile/ios/project.yml" \
    --project "$generated_project_root"

/usr/bin/xcodebuild test \
  -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
  -scheme OxidUITests \
  -destination "platform=iOS Simulator,id=$device" \
  -derivedDataPath "$repository_root/target/mobile-tests/ios-derived-data" \
  CODE_SIGNING_ALLOWED=NO
