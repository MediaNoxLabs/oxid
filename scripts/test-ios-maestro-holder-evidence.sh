#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run one receipt-owned, privacy-safe iOS Maestro holder journey at 375 points.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly ROOT
readonly HEAD="$(git -C "$ROOT" rev-parse HEAD)"
readonly STARTED_AT="$(date +%s)"
readonly RUN_ROOT="$ROOT/target/mobile-visual-accessibility/ios-run-${HEAD:0:12}-${STARTED_AT}"
readonly PRIVATE_ROOT="$RUN_ROOT/private"
readonly RECEIPT="$PRIVATE_ROOT/simulator-receipt.json"
readonly METRICS="$RUN_ROOT/receipt.json"
readonly DEVICE_TYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation"

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"

DEVICE=""
owned=0
cleanup_ok=false
raw_artifacts_removed=false

collect_public_artifacts() {
  local raw_root latest log_file screenshot source
  [ -n "$DEVICE" ] || return 0
  raw_root="$ROOT/target/mobile-visual-accessibility/ios/$DEVICE"
  [ -d "$raw_root" ] || return 0
  latest="$(find "$raw_root" -name manifest.json -type f -print 2>/dev/null | sort | tail -1)"
  if [ -n "$latest" ]; then
    latest="$(dirname "$latest")"
    mkdir -p "$RUN_ROOT/screenshots"
    while IFS= read -r source; do
      screenshot="$(basename "$source")"
      cp -- "$source" "$RUN_ROOT/screenshots/$screenshot"
    done < <(find "$latest/takeScreenshot" -type f -name 'lunar-aegis-ios-*.png' -print 2>/dev/null | sort)
    log_file="$latest/logs/maestro.log"
    if [ -f "$log_file" ]; then
      tail -n 200 "$log_file" >"$RUN_ROOT/maestro-tail.log"
    fi
  fi
  rm -rf -- "$raw_root"
  raw_artifacts_removed=true
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  collect_public_artifacts
  if [ "$owned" = 1 ]; then
    oxid_ios_delete_owned "$DEVELOPER_DIR" "$RECEIPT" >/dev/null && cleanup_ok=true
  else
    cleanup_ok=true
  fi
  if [ "$cleanup_ok" = true ]; then
    rm -rf -- "$PRIVATE_ROOT"
  fi
  local finished_at duration artifact_bytes screenshot_count artifact_file artifact_size
  finished_at="$(date +%s)"
  duration=$((finished_at - STARTED_AT))
  artifact_bytes=0
  while IFS= read -r artifact_file; do
    artifact_size="$(wc -c <"$artifact_file" | tr -d ' ')"
    artifact_bytes=$((artifact_bytes + artifact_size))
  done < <(find "$RUN_ROOT" -type f -not -path "$PRIVATE_ROOT/*" -print 2>/dev/null)
  screenshot_count="$(find "$RUN_ROOT" -type f \( -name '*.png' -o -name '*.jpg' \) -not -path "$PRIVATE_ROOT/*" | wc -l | tr -d ' ')"
  mkdir -p "$RUN_ROOT"
  jq -n \
    --arg head "$HEAD" --arg device "$DEVICE" --argjson started "$STARTED_AT" \
    --argjson finished "$finished_at" --argjson duration "$duration" \
    --argjson bytes "$artifact_bytes" --argjson screenshots "$screenshot_count" \
    --argjson passed "$([ "$status" -eq 0 ] && printf true || printf false)" \
    --argjson cleaned "$cleanup_ok" --argjson rawRemoved "$raw_artifacts_removed" \
    '{schema:"oxid-ios-maestro-holder-evidence-v1",oxid:{head:$head},platform:{kind:"ios_simulator",viewport:"375-pt-class",deviceType:"iPhone SE (3rd generation)",udid:$device},outcome:{passed:$passed,startedAtUnix:$started,finishedAtUnix:$finished,durationSeconds:$duration},artifacts:{publicBytes:$bytes,screenshotCount:$screenshots,boundedLog:"maestro-tail.log"},cleanup:{receiptOwnedSimulator:true,privateDiagnosticsRemoved:$cleaned,rawArtifactsRemoved:$rawRemoved}}' \
    >"$METRICS"
  chmod 644 "$METRICS"
  exit "$status"
}
trap cleanup EXIT INT TERM HUP

fail() { printf 'ios-maestro-holder-evidence: FAIL %s\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail platform
[ -z "${OXID_IOS_DEVICE:-}" ] || fail ambient-device-selector
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail dirty-source
for command in jq nix node rustup timeout; do command -v "$command" >/dev/null 2>&1 || fail "missing-$command"; done

DEVELOPER_DIR="${OXID_XCODE_DEVELOPER_DIR:-$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)}"
RUNTIME_ID="${OXID_IOS_RUNTIME_ID:-com.apple.CoreSimulator.SimRuntime.iOS-17-5}"
oxid_ios_preflight "$DEVELOPER_DIR" "$RUNTIME_ID" "$DEVICE_TYPE" || fail selectors

mkdir -p "$RUN_ROOT" || fail evidence-root
mkdir -m 700 "$PRIVATE_ROOT" || fail private-root
DEVICE="$(oxid_ios_create_owned "$DEVELOPER_DIR" "$RUNTIME_ID" "$DEVICE_TYPE" "oxid-maestro-${HEAD:0:12}" "$RECEIPT")" || fail simulator-create
chmod 600 "$RECEIPT" || fail receipt-mode
owned=1
oxid_ios_owned_simctl "$DEVELOPER_DIR" "$RECEIPT" boot >/dev/null || fail simulator-boot
oxid_ios_owned_simctl "$DEVELOPER_DIR" "$RECEIPT" bootstatus -b >/dev/null || fail simulator-ready

OXID_IOS_DEVICE="$DEVICE" OXID_IOS_RESET_DATA=1 OXID_STANDALONE_NETWORK_PROFILE=simulated OXID_UI_PROFILE=demo \
  OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR" "$ROOT/scripts/run-ios-simulator.sh" build || fail app-build
OXID_IOS_DEVICE="$DEVICE" OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR" "$ROOT/scripts/run-maestro-ios.sh" || fail maestro

oxid_ios_delete_owned "$DEVELOPER_DIR" "$RECEIPT" >/dev/null || fail simulator-cleanup
owned=0
cleanup_ok=true
printf 'ios-maestro-holder-evidence: PASS receipt=%s\n' "$METRICS"
