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

xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
host_user="$(id -un)"
app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
bundle_identifier="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$app_bundle/Info.plist")"
test_source="$repository_root/tests/mobile/ios/OxidUITests/ProfileFlowTests.swift"
test_names=()
while IFS= read -r test_name; do
  test_names+=("$test_name")
done < <(
  sed -nE \
    's/^[[:space:]]*func (test[A-Za-z0-9_]+)\(\)( throws)? \{.*/\1/p' \
    "$test_source"
)
if [ "${#test_names[@]}" -eq 0 ]; then
  echo "No ProfileFlowTests test methods were discovered in $test_source." >&2
  exit 1
fi

# Every scenario owns a clean installation. Several fixtures deliberately have
# stable replay identifiers, while onboarding deliberately requires no prior
# profile; sharing one app container makes those independent guarantees depend
# on XCTest's method order.
for test_name in "${test_names[@]}"; do
  echo "Running isolated iOS profile scenario: $test_name"
  /usr/bin/xcrun simctl terminate "$device" "$bundle_identifier" >/dev/null 2>&1 || true
  /usr/bin/xcrun simctl uninstall "$device" "$bundle_identifier" >/dev/null 2>&1 || true
  /usr/bin/xcrun simctl install "$device" "$app_bundle"

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
    -derivedDataPath "$repository_root/target/mobile-tests/ios-derived-data" \
    -only-testing:"OxidUITests/ProfileFlowTests/$test_name" \
    CODE_SIGNING_ALLOWED=NO
done
