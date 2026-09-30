#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run the checked-in local-only pilot against one explicit simulator.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
: "${OXID_IOS_DEVICE:?set OXID_IOS_DEVICE to the receipt-owned simulator UDID}"
artifact_root="$root/target/mobile-visual-accessibility/ios/$OXID_IOS_DEVICE"
debug_root="$artifact_root/debug"
mkdir -p "$debug_root"

run_phase() {
  local phase="$1"
  shift
  local started="$SECONDS"
  if "$@"; then
    printf 'factory-metrics phase=%s result=passed duration_ms=%s\n' \
      "$phase" "$(( (SECONDS - started) * 1000 ))"
  else
    local status=$?
    printf 'factory-metrics phase=%s result=failed duration_ms=%s\n' \
      "$phase" "$(( (SECONDS - started) * 1000 ))" >&2
    return "$status"
  fi
}

launcher_environment=(
  env
  OXID_IOS_DEVICE="$OXID_IOS_DEVICE"
  OXID_STANDALONE_NETWORK_PROFILE=simulated
  OXID_UI_PROFILE=demo
)
run_phase build "${launcher_environment[@]}" ./scripts/run-ios-simulator.sh ensure
run_phase deploy "${launcher_environment[@]}" OXID_IOS_RESET_DATA=1 \
  ./scripts/run-ios-simulator.sh deploy
run_phase maestro nix run .#maestro -- test tests/maestro/ios-lunar-aegis.yaml \
  --udid "$OXID_IOS_DEVICE" --test-output-dir "$artifact_root" \
  --debug-output "$debug_root"
