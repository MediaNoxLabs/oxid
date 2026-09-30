#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run every safe iOS Maestro inventory flow in one receipt-owned simulator lane.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly ROOT
readonly HEAD="$(git -C "$ROOT" rev-parse HEAD)"
readonly STARTED_AT="$(date +%s)"
readonly RUN_ROOT="$ROOT/target/mobile-visual-accessibility/ios-run-${HEAD:0:12}-${STARTED_AT}"
readonly PRIVATE_ROOT="$RUN_ROOT/private"
readonly RECEIPT="$PRIVATE_ROOT/simulator-receipt.json"
readonly METRICS="$RUN_ROOT/receipt.json"
readonly OUTCOMES="$PRIVATE_ROOT/scenario-outcomes.jsonl"
readonly DEVICE_TYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation"

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"

DEVICE=""
owned=0
cleanup_ok=false
raw_artifacts_removed=false
runtime_version="unknown"

collect_public_artifacts() {
  local raw_root latest log_file screenshot source
  [ -n "$DEVICE" ] || return 0
  raw_root="$ROOT/target/mobile-visual-accessibility/ios/$DEVICE"
  if [ ! -d "$raw_root" ]; then raw_artifacts_removed=true; return 0; fi
  latest="$(find "$raw_root" -name manifest.json -type f -print 2>/dev/null | sort | tail -1)"
  if [ -n "$latest" ]; then
    latest="$(dirname "$latest")"
    mkdir -p "$RUN_ROOT/screenshots"
    while IFS= read -r source; do
      screenshot="$(basename "$source")"
      cp -- "$source" "$RUN_ROOT/screenshots/$screenshot"
    done < <(find "$raw_root" -type f \( -name 'lunar-aegis-ios-*.png' -o -name 'developer-profile-banner-*.png' \) -print 2>/dev/null | sort)
    log_file="$latest/logs/maestro.log"
    [ ! -f "$log_file" ] || tail -n 200 "$log_file" >"$RUN_ROOT/maestro-tail.log"
  fi
  rm -rf -- "$raw_root"
  raw_artifacts_removed=true
}

cleanup() {
  local status=$? finished_at duration artifact_bytes screenshot_count artifact_file artifact_size outcomes
  trap - EXIT INT TERM HUP
  set +e
  collect_public_artifacts
  if [ "$owned" = 1 ]; then oxid_ios_delete_owned "$DEVELOPER_DIR" "$RECEIPT" >/dev/null && cleanup_ok=true; else cleanup_ok=true; fi
  finished_at="$(date +%s)"
  duration=$((finished_at - STARTED_AT))
  artifact_bytes=0
  while IFS= read -r artifact_file; do
    artifact_size="$(wc -c <"$artifact_file" | tr -d ' ')"
    artifact_bytes=$((artifact_bytes + artifact_size))
  done < <(find "$RUN_ROOT" -type f -not -path "$PRIVATE_ROOT/*" -print 2>/dev/null)
  screenshot_count="$(find "$RUN_ROOT" -type f \( -name '*.png' -o -name '*.jpg' \) -not -path "$PRIVATE_ROOT/*" | wc -l | tr -d ' ')"
  outcomes='[]'
  [ ! -f "$OUTCOMES" ] || outcomes="$(jq -s . "$OUTCOMES")"
  mkdir -p "$RUN_ROOT"
  jq -n --arg head "$HEAD" --arg device "$DEVICE" --arg runtime "$runtime_version" \
    --argjson started "$STARTED_AT" --argjson finished "$finished_at" --argjson duration "$duration" \
    --argjson bytes "$artifact_bytes" --argjson screenshots "$screenshot_count" \
    --argjson passed "$( [ "$status" -eq 0 ] && printf true || printf false )" \
    --argjson cleaned "$cleanup_ok" --argjson rawRemoved "$raw_artifacts_removed" --argjson scenario_outcomes "$outcomes" \
    '{schema:"oxid-ios-maestro-evidence-v2",oxid:{head:$head},platform:{kind:"ios_simulator",viewport:"375-pt-class",deviceType:"iPhone SE (3rd generation)",udid:$device,runtime:$runtime},outcome:{passed:$passed,startedAtUnix:$started,finishedAtUnix:$finished,durationSeconds:$duration,scenarios:$scenario_outcomes},artifacts:{publicBytes:$bytes,screenshotCount:$screenshots,boundedLog:"maestro-tail.log"},cleanup:{receiptOwnedSimulator:true,privateDiagnosticsRemoved:$cleaned,rawArtifactsRemoved:$rawRemoved}}' >"$METRICS"
  chmod 644 "$METRICS"
  [ "$cleanup_ok" = true ] && rm -rf -- "$PRIVATE_ROOT"
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
runtime_version="$(
  oxid_ios_xcrun "$DEVELOPER_DIR" simctl list runtimes -j |
    jq -er --arg runtime "$RUNTIME_ID" \
      'first(.runtimes[] | select(.identifier == $runtime and .isAvailable == true) | .version)'
)" || fail runtime
mkdir -p "$RUN_ROOT" || fail evidence-root
mkdir -m 700 "$PRIVATE_ROOT" || fail private-root
DEVICE="$(oxid_ios_create_owned "$DEVELOPER_DIR" "$RUNTIME_ID" "$DEVICE_TYPE" "oxid-maestro-${HEAD:0:12}" "$RECEIPT")" || fail simulator-create
owned=1
chmod 600 "$RECEIPT" || fail receipt-mode
oxid_ios_owned_simctl "$DEVELOPER_DIR" "$RECEIPT" boot >/dev/null || fail simulator-boot
oxid_ios_owned_simctl "$DEVELOPER_DIR" "$RECEIPT" bootstatus -b >/dev/null || fail simulator-ready

scenarios=()
while IFS= read -r scenario; do
  scenarios+=("$scenario")
done < <(jq -r '
  .scenarios[] |
  select(.authority == "maestro" and (.platforms | index("ios")) and .id != "canonical-holder-evidence") |
  [if .composition == "dev" then 0 else 1 end, .id] | @tsv
' "$ROOT/tests/maestro/inventory.json" | LC_ALL=C sort | cut -f2)
scenarios+=(canonical-holder-evidence)
for scenario in "${scenarios[@]}"; do
  if OXID_IOS_DEVICE="$DEVICE" OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR" "$ROOT/scripts/run-maestro-ios.sh" --composition "$(jq -r --arg id "$scenario" '.scenarios[] | select(.id == $id) | .composition' "$ROOT/tests/maestro/inventory.json")" --flow "$scenario"; then
    jq -cn --arg id "$scenario" '{id:$id,outcome:"passed"}' >>"$OUTCOMES"
  else
    jq -cn --arg id "$scenario" '{id:$id,outcome:"failed"}' >>"$OUTCOMES"
    fail "maestro-$scenario"
  fi
done
oxid_ios_delete_owned "$DEVELOPER_DIR" "$RECEIPT" >/dev/null || fail simulator-cleanup
owned=0
cleanup_ok=true
printf 'ios-maestro-holder-evidence: PASS receipt=%s\n' "$METRICS"
