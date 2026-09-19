#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=
# A freshly created recent iOS runtime can need more than the shared helper's
# 30-second default to finish first boot. This diagnostic owns the disposable
# simulator and keeps every simulator operation bounded by two minutes.
export OXID_IOS_OPERATION_TIMEOUT_SECONDS=120

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly ROOT
readonly RUN_ROOT="$ROOT/target/ios-wallet-lifecycle-simulator"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly RECEIPT="$PRIVATE_STATE/simulator-receipt.json"
readonly DIAGNOSTIC="$PRIVATE_STATE/lifecycle-diagnostic.json"
readonly FAILURE_SCREENSHOT="$PRIVATE_STATE/failure.png"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly APP_BUNDLE="$ROOT/target/dx/oxid-app/debug/ios/OxidApp.app"
readonly ARTIFACT_RECEIPT="$ROOT/target/dx/oxid-app/debug/ios/oxid-app-artifact-receipt.json"
readonly PACKAGE="io.medianox.oxid"

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"

simulator_owned=0
journey_passed=0

fail() {
  printf 'ios-wallet-lifecycle-simulator: FAIL phase=%s\n' "$1" >&2
  exit 1
}

cleanup() {
  local incoming=$?
  trap - EXIT INT TERM HUP
  set +e
  if [ "$incoming" -ne 0 ] && [ "$simulator_owned" -eq 1 ]; then
    oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" io screenshot \
      "$FAILURE_SCREENSHOT" >/dev/null 2>&1 || true
    chmod 600 "$FAILURE_SCREENSHOT" >/dev/null 2>&1 || true
  fi
  if [ "$simulator_owned" -eq 1 ]; then
    oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$RECEIPT" >/dev/null 2>&1 || incoming=1
    simulator_owned=0
  fi
  if [ "$incoming" -eq 0 ] && [ "$journey_passed" -eq 1 ]; then
    rm -rf -- "$PRIVATE_STATE"
  else
    printf 'ios-wallet-lifecycle-simulator: private diagnostics retained mode=0600\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT INT TERM HUP

[ "$(uname -s)" = Darwin ] || fail platform
[ -z "${OXID_IOS_DEVICE:-}" ] || fail ambient-device-selector
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail dirty-source
for command_name in git jq nix node rustup shasum timeout; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-tool-$command_name"
done
readonly TIMEOUT="$(command -v timeout)"
[ -x /usr/bin/xcodebuild ] && [ -x /usr/bin/xcrun ] && [ -x /usr/bin/plutil ] || fail xcode-tools

readonly DEVELOPER_DIR_SELECTED="${OXID_XCODE_DEVELOPER_DIR:-}"
readonly RUNTIME_ID="${OXID_IOS_RUNTIME_ID:-}"
readonly DEVICE_TYPE_ID="${OXID_IOS_DEVICE_TYPE_ID:-}"
oxid_ios_preflight "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" || fail selectors

[ ! -e "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || fail occupied-evidence
mkdir -m 700 "$RUN_ROOT" "$PRIVATE_STATE" || fail private-state

readonly HEAD="$(git -C "$ROOT" rev-parse HEAD)"
readonly TREE="$(git -C "$ROOT" rev-parse 'HEAD^{tree}')"
readonly DEVICE_NAME="oxid-lifecycle-${HEAD:0:12}"
DEVICE="$(oxid_ios_create_owned "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" "$DEVICE_NAME" "$RECEIPT")" \
  || fail simulator-create
readonly DEVICE
simulator_owned=1
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" boot >/dev/null || fail simulator-boot
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" bootstatus -b >/dev/null || fail simulator-ready

OXID_IOS_DEVICE="$DEVICE" OXID_IOS_RESET_DATA=1 \
OXID_MOBILE_CUSTODY=native \
OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR_SELECTED" \
  "$ROOT/scripts/run-ios-simulator.sh" build || fail app-build
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" install "$APP_BUNDLE" >/dev/null \
  || fail app-install

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)" || fail xcodegen-build
generated_project_root="$PRIVATE_STATE/xcode"
mkdir -m 700 "$generated_project_root" || fail xcode-project-state
OXID_REPOSITORY_ROOT="$ROOT" "$xcodegen_output/bin/xcodegen" generate \
  --spec "$ROOT/tests/mobile/ios/project.yml" --project "$generated_project_root" \
  >/dev/null || fail xcodegen

host_user="$(id -un)"
env -i \
  "DEVELOPER_DIR=$DEVELOPER_DIR_SELECTED" \
  "HOME=$HOME" \
  "LANG=${LANG:-en_US.UTF-8}" \
  "LOGNAME=$host_user" \
  "OXID_LIFECYCLE_DIAGNOSTIC_PATH=$DIAGNOSTIC" \
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
  "TMPDIR=${TMPDIR:-/tmp}" \
  "USER=$host_user" \
  "$TIMEOUT" -k 30s 1200s /usr/bin/xcodebuild test \
    -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
    -scheme OxidUITests \
    -destination "platform=iOS Simulator,id=$DEVICE" \
    -derivedDataPath "$PRIVATE_STATE/derived-data" \
    -only-testing:"OxidUITests/LifecycleRecoveryTests/testBackgroundAndColdRelaunchRecoverWithoutManualSync" \
    CODE_SIGNING_ALLOWED=NO || fail lifecycle-test

oxid_ios_receipt_mode_is_private "$DIAGNOSTIC" || fail diagnostic-mode
jq -e '
  (keys | sort) == (["backgroundForeground","manualFamilySync","processRelaunch","protectedInteraction","schema","staleObservation"] | sort)
  and .schema == "oxid-ios-wallet-lifecycle-diagnostic-v1"
  and .backgroundForeground == "recovered"
  and .processRelaunch == "recovered"
  and .protectedInteraction == "rearmed"
  and .manualFamilySync == "not_used"
  and .staleObservation == "not_visible"
' "$DIAGNOSTIC" >/dev/null || fail diagnostic-contract

artifact_sha="$(jq -er '.artifactSha256' "$ARTIFACT_RECEIPT")" || fail artifact-receipt
[ "$(git -C "$ROOT" rev-parse HEAD)" = "$HEAD" ] || fail head-changed
[ "$(git -C "$ROOT" rev-parse 'HEAD^{tree}')" = "$TREE" ] || fail tree-changed
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail source-changed
oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$RECEIPT" >/dev/null \
  || fail simulator-cleanup
simulator_owned=0
rm -rf -- "$PRIVATE_STATE" || fail private-cleanup
[ ! -e "$PRIVATE_STATE" ] || fail private-cleanup
jq -n \
  --arg head "$HEAD" --arg tree "$TREE" --arg artifact "$artifact_sha" \
  '{schema:"oxid-ios-wallet-lifecycle-evidence-v1",oxid:{head:$head,tree:$tree},
    platform:{kind:"ios_simulator"},artifactSha256:$artifact,
    outcomes:{backgroundForeground:"recovered",processRelaunch:"recovered",
      protectedInteraction:"rearmed",manualFamilySync:"not_used",staleObservation:"not_visible"},
    cleanup:{receiptOwnedSimulator:true,privateDiagnosticsRemoved:true}}' \
  >"$EVIDENCE" || fail evidence
chmod 644 "$EVIDENCE"
journey_passed=1
printf 'ios-wallet-lifecycle-simulator: PASS evidence=%s\n' "$EVIDENCE"
