#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS complete-backup smoke test requires macOS and Xcode." >&2
  exit 1
fi

for command_name in nix jq rustup; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done
if [ ! -x /usr/bin/xcodebuild ] || [ ! -x /usr/bin/xcrun ]; then
  echo "Xcode is required for the iOS complete-backup smoke test." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

template="$({
  /usr/bin/xcrun simctl list devices available -j \
    | jq -r '
        .devices
        | to_entries
        | map(select(.key | startswith("com.apple.CoreSimulator.SimRuntime.iOS-")))
        | sort_by(.key)
        | reverse
        | map(. as $runtime
            | $runtime.value[]
            | select(.isAvailable and (.name | startswith("iPhone")))
            | [$runtime.key, .deviceTypeIdentifier]
          )
        | first
        | @tsv
      '
} 2>/dev/null)"
if [ -z "$template" ] || [ "$template" = "null" ]; then
  echo "No available iPhone simulator runtime and device type were found." >&2
  exit 1
fi
IFS=$'\t' read -r runtime_identifier device_type_identifier <<<"$template"

device_name="Oxid Complete Backup Smoke $$"
device="$(/usr/bin/xcrun simctl create "$device_name" "$device_type_identifier" "$runtime_identifier")"
cleanup() {
  local exit_status=$?
  if [ "$exit_status" -ne 0 ] && [ "${OXID_IOS_KEEP_FAILED_SIMULATOR:-0}" = "1" ]; then
    echo "Keeping failed disposable simulator $device ($device_name) for diagnostics." >&2
    return
  fi
  /usr/bin/xcrun simctl shutdown "$device" >/dev/null 2>&1 || true
  /usr/bin/xcrun simctl delete "$device" >/dev/null 2>&1 || true
}
trap cleanup EXIT

/usr/bin/xcrun simctl boot "$device"
/usr/bin/xcrun simctl bootstatus "$device" -b
OXID_MOBILE_CUSTODY=development OXID_IOS_DEVICE="$device" OXID_IOS_RESET_DATA=1 \
  "$repository_root/scripts/run-ios-simulator.sh"

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)"
generated_project_root="$repository_root/target/mobile-tests/ios-backup"
mkdir -p "$generated_project_root"
OXID_REPOSITORY_ROOT="$repository_root" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$repository_root/tests/mobile/ios/project.yml" \
    --project "$generated_project_root"

xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
host_user="$(id -un)"
run_test() {
  local test_name="$1"
  env -i \
    "DEVELOPER_DIR=$xcode_developer_dir" \
    "HOME=$HOME" \
    "LANG=${LANG:-en_US.UTF-8}" \
    "LOGNAME=$host_user" \
    "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
    "TMPDIR=${TMPDIR:-/tmp}" \
    "USER=$host_user" \
    /usr/bin/xcodebuild test \
    -quiet \
    -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
    -scheme OxidUITests \
    -destination "platform=iOS Simulator,id=$device" \
    -derivedDataPath "$repository_root/target/mobile-tests/ios-backup-derived-data" \
    -only-testing:"OxidUITests/BackupFlowTests/$test_name" \
    CODE_SIGNING_ALLOWED=NO
}

run_test testExportsCompleteWalletBackupThroughDocumentPicker

app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
/usr/bin/xcrun simctl terminate "$device" io.medianox.oxid >/dev/null 2>&1 || true
/usr/bin/xcrun simctl uninstall "$device" io.medianox.oxid
/usr/bin/xcrun simctl keychain "$device" reset
/usr/bin/xcrun simctl shutdown "$device"
/usr/bin/xcrun simctl boot "$device"
/usr/bin/xcrun simctl bootstatus "$device" -b
/usr/bin/xcrun simctl install "$device" "$app_bundle"

run_test testRecoversCompleteWalletBackupThroughDocumentPicker

echo "iOS complete-wallet native document export, app reset, import, and recovery passed on disposable simulator $device."
