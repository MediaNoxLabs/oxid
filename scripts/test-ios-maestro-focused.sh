#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run one inventory-owned Maestro flow in one disposable iOS Simulator.
set -euo pipefail
export LC_ALL=C
export CDPATH=
# A freshly created current runtime can spend more than two minutes in first
# boot. Keep each simulator operation bounded without consuming the lane's
# separate thirty-minute end-to-end budget.
export OXID_IOS_OPERATION_TIMEOUT_SECONDS="${OXID_IOS_OPERATION_TIMEOUT_SECONDS:-300}"

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly ROOT
readonly INVENTORY="$ROOT/tests/maestro/inventory.json"

usage() {
  printf 'usage: %s --composition demo|dev --flow <inventory-id>\n' "$0" >&2
  exit 2
}

[ "$#" -eq 4 ] || usage
[ "$1" = --composition ] || usage
readonly COMPOSITION="$2"
[ "$3" = --flow ] || usage
readonly FLOW_ID="$4"
case "$COMPOSITION" in demo|dev) ;; *) usage ;; esac
[[ "$FLOW_ID" =~ ^[a-z0-9][a-z0-9-]{0,79}$ ]] || usage

# Resolve one closed inventory member before touching Xcode or host state.
readonly FLOW_PATH="$({
  jq -er --arg id "$FLOW_ID" --arg composition "$COMPOSITION" '
    [.scenarios[] | select(
      .id == $id and .composition == $composition and .authority == "maestro"
      and (.platforms | index("ios"))
    )]
    | if length == 1 then first.flow else error("not one supported scenario") end
  ' "$INVENTORY"
})" || usage
[[ "$FLOW_PATH" =~ ^flows/[a-z0-9-]+\.yaml$ ]] || usage
[ -f "$ROOT/tests/maestro/$FLOW_PATH" ] || usage

fail() {
  printf 'ios-maestro-focused: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ "$(uname -s)" = Darwin ] || fail platform
[ -z "${OXID_IOS_DEVICE:-}" ] || fail ambient-device-selector
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail dirty-source
for command_name in git jq nix node rustup shasum timeout; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-tool-$command_name"
done

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"

readonly HEAD="$(git -C "$ROOT" rev-parse HEAD)"
readonly TREE="$(git -C "$ROOT" rev-parse 'HEAD^{tree}')"
readonly STARTED_AT="$(date +%s)"
readonly RUN_ROOT="$ROOT/target/mobile-visual-accessibility/focused-ios/run-${HEAD:0:12}-${STARTED_AT}-${FLOW_ID}"
readonly PRIVATE_ROOT="$RUN_ROOT/private"
readonly SIMULATOR_RECEIPT="$PRIVATE_ROOT/simulator-receipt.json"
readonly PRIVATE_LOG="$PRIVATE_ROOT/maestro.log"
readonly METRICS_RECEIPT="$RUN_ROOT/receipt.json"

DEVELOPER_DIR_SELECTED=""
RUNTIME_ID=""
DEVICE_TYPE_ID=""
DEVICE=""
simulator_owned=0
scenario_passed=0
cleanup_ok=false
raw_removed=false
private_removed=false

cleanup() {
  local incoming=$? finished_at duration outcome
  trap - EXIT INT TERM HUP
  set +e
  if [ -n "$DEVICE" ]; then
    rm -rf -- "$ROOT/target/mobile-visual-accessibility/ios/$DEVICE"
    [ ! -e "$ROOT/target/mobile-visual-accessibility/ios/$DEVICE" ] && raw_removed=true
  else
    raw_removed=true
  fi
  if [ "$simulator_owned" -eq 1 ]; then
    if oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$SIMULATOR_RECEIPT" >/dev/null 2>&1; then
      simulator_owned=0
      cleanup_ok=true
    fi
  else
    cleanup_ok=true
  fi
  rm -rf -- "$PRIVATE_ROOT"
  [ ! -e "$PRIVATE_ROOT" ] && private_removed=true
  if [ "$cleanup_ok" != true ] || [ "$raw_removed" != true ] || [ "$private_removed" != true ]; then
    incoming=1
  fi
  finished_at="$(date +%s)"
  duration=$((finished_at - STARTED_AT))
  if [ "$incoming" -eq 0 ] && [ "$scenario_passed" -eq 1 ]; then outcome=passed; else outcome=failed; fi
  mkdir -p "$RUN_ROOT"
  jq -n --arg head "$HEAD" --arg tree "$TREE" --arg scenario "$FLOW_ID" \
    --arg composition "$COMPOSITION" --arg outcome "$outcome" \
    --argjson duration "$duration" --argjson cleaned "$cleanup_ok" \
    --argjson rawRemoved "$raw_removed" --argjson privateRemoved "$private_removed" \
    '{schema:"oxid-ios-maestro-focused-diagnostic-v1",oxid:{head:$head,tree:$tree},
      authority:"diagnostic-only",releaseEvidence:false,scenario:{id:$scenario,composition:$composition},
      outcome:{result:$outcome,durationSeconds:$duration},
      cleanup:{receiptOwnedSimulator:true,simulatorRemoved:$cleaned,
        rawArtifactsRemoved:$rawRemoved,privateDiagnosticsRemoved:$privateRemoved}}' >"$METRICS_RECEIPT"
  chmod 644 "$METRICS_RECEIPT"
  printf 'factory-metrics phase=ios-maestro-focused scenario=%s result=%s duration_ms=%s cleanup=%s\n' \
    "$FLOW_ID" "$outcome" "$((duration * 1000))" "$cleanup_ok"
  exit "$incoming"
}

on_signal() { exit 130; }
[ ! -e "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || fail occupied-run-root
mkdir -m 700 -p "$RUN_ROOT" "$PRIVATE_ROOT" || fail private-root
trap cleanup EXIT
trap on_signal INT TERM HUP

DEVELOPER_DIR_SELECTED="$(oxid_ios_discover_developer_directory "${OXID_XCODE_DEVELOPER_DIR:-}")" \
  || fail developer-directory
selector_pair="$(oxid_ios_resolve_selectors "$DEVELOPER_DIR_SELECTED" \
  "${OXID_IOS_RUNTIME_ID:-}" "${OXID_IOS_DEVICE_TYPE_ID:-}")" || fail selectors
IFS=$'\t' read -r RUNTIME_ID DEVICE_TYPE_ID <<<"$selector_pair"
oxid_ios_preflight "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" || fail selectors

DEVICE="$(oxid_ios_create_owned "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" "$DEVICE_TYPE_ID" \
  "oxid-focused-${HEAD:0:12}-${FLOW_ID:0:30}" "$SIMULATOR_RECEIPT")" || fail simulator-create
simulator_owned=1
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$SIMULATOR_RECEIPT" boot >/dev/null \
  || fail simulator-boot
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$SIMULATOR_RECEIPT" bootstatus -b >/dev/null \
  || fail simulator-ready

if ! OXID_IOS_DEVICE="$DEVICE" OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR_SELECTED" \
  "$ROOT/scripts/run-maestro-ios.sh" --composition "$COMPOSITION" --flow "$FLOW_ID" \
  >"$PRIVATE_LOG" 2>&1; then
  fail maestro
fi

[ "$(git -C "$ROOT" rev-parse HEAD)" = "$HEAD" ] || fail head-changed
[ "$(git -C "$ROOT" rev-parse 'HEAD^{tree}')" = "$TREE" ] || fail tree-changed
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail source-changed
scenario_passed=1
