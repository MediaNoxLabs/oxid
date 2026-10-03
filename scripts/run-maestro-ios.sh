#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run one closed inventory flow against one explicit receipt-owned simulator.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
umask 077
lane_lock="$root/target/mobile-visual-accessibility/ios/.maestro-lane.lock"

if [ "${1:-}" = "--cleanup-stale-lane" ]; then
  [ "$#" -eq 2 ] || { echo "usage: $0 --cleanup-stale-lane <owner-token>" >&2; exit 2; }
  node scripts/lib/maestro-ios-lane.mjs cleanup --lock "$lane_lock" --token "$2"
  printf 'factory-metrics phase=maestro-ios-lane result=cleaned\n'
  exit 0
fi

: "${OXID_IOS_DEVICE:?set OXID_IOS_DEVICE to the receipt-owned simulator UDID}"
if ! [[ "$OXID_IOS_DEVICE" =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$ ]]; then
  echo "OXID_IOS_DEVICE must be an explicit simulator UDID" >&2
  exit 2
fi

usage() {
  echo "usage: $0 --composition demo|dev --flow <inventory-id>" >&2
  exit 2
}
[ "$#" -eq 4 ] || usage
[ "$1" = "--composition" ] || usage
composition="$2"
[ "$3" = "--flow" ] || usage
flow_id="$4"
case "$composition" in demo|dev) ;; *) usage ;; esac
inventory="tests/maestro/inventory.json"
flow="$(
  jq -r --arg id "$flow_id" --arg composition "$composition" '
    first(.scenarios[] | select(
      .id == $id and .composition == $composition and .authority == "maestro"
      and (.platforms | index("ios"))
    ) | .flow) // empty
  ' "$inventory"
)"
if ! [[ "$flow" =~ ^flows/[a-z0-9-]+\.yaml$ ]] || [ ! -f "tests/maestro/$flow" ]; then
  echo "unknown, unsupported, or lower-layer-only inventory flow: $flow_id" >&2
  exit 2
fi
flow="tests/maestro/$flow"
artifact_root="$root/target/mobile-visual-accessibility/ios/$OXID_IOS_DEVICE"
debug_root="$artifact_root/debug"
mkdir -p "$debug_root"

# One global local simulator lane prevents concurrent flows from reusing a receipt.
set +e
lane_result="$(node scripts/lib/maestro-ios-lane.mjs acquire \
  --lock "$lane_lock" --pid "$$" --host "$(hostname)" \
  --started-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --worktree "$root" --flow "$flow_id")"
lane_status=$?
set -e
if [ "$lane_status" -ne 0 ]; then
  outcome="$(printf '%s' "$lane_result" | jq -r '.outcome // "failed"')"
  token="$(printf '%s' "$lane_result" | jq -r '.owner.token // empty')"
  printf 'factory-metrics phase=maestro-ios-lane result=%s\n' "$outcome" >&2
  echo "Maestro iOS lane is unavailable: $lane_result" >&2
  if [ -n "$token" ]; then
    echo "after verifying the owner, clean this exact lease with: $0 --cleanup-stale-lane $token" >&2
  fi
  exit 75
fi
lane_token="$(printf '%s' "$lane_result" | jq -r '.owner.token')"
lane_outcome="$(printf '%s' "$lane_result" | jq -r '.outcome')"
printf 'factory-metrics phase=maestro-ios-lane result=%s\n' "$lane_outcome"
cleanup() {
  local status=$?
  trap - EXIT
  if [ "$status" -ne 0 ]; then
    rm -rf -- "$artifact_root"
  fi
  node scripts/lib/maestro-ios-lane.mjs release --lock "$lane_lock" --token "$lane_token" || true
  exit "$status"
}
trap cleanup EXIT

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
  OXID_UI_PROFILE="$composition"
)
run_phase build "${launcher_environment[@]}" ./scripts/run-ios-simulator.sh ensure
run_phase deploy "${launcher_environment[@]}" OXID_IOS_RESET_DATA=1 \
  ./scripts/run-ios-simulator.sh deploy
run_phase maestro nix run .#maestro -- test "$flow" \
  --udid "$OXID_IOS_DEVICE" --test-output-dir "$artifact_root" \
  --debug-output "$debug_root"
