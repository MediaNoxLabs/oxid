#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Final disposable-emulator semantic sweep; retain no Android visual artifacts.
set -euo pipefail
umask 077

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly ROOT
readonly HEAD="$(git -C "$ROOT" rev-parse HEAD)"
readonly STARTED_AT="$(date +%s)"
: "${OXID_ANDROID_DEVICE:?set OXID_ANDROID_DEVICE to a disposable emulator-* serial}"
: "${OXID_ANDROID_DISPOSABLE:?set OXID_ANDROID_DISPOSABLE=1 only for an operator-owned disposable emulator}"
[ "$OXID_ANDROID_DISPOSABLE" = 1 ] || { echo "refusing emulator without OXID_ANDROID_DISPOSABLE=1" >&2; exit 2; }
case "$OXID_ANDROID_DEVICE" in emulator-*) ;; *) echo "refusing non-emulator device" >&2; exit 2;; esac
[[ "$OXID_ANDROID_DEVICE" =~ ^emulator-[0-9]+$ ]] || { echo "refusing malformed emulator serial" >&2; exit 2; }
readonly RUN_ROOT="$ROOT/target/mobile-visual-accessibility/android-run-${HEAD:0:12}-${STARTED_AT}"
readonly PRIVATE_ROOT="$RUN_ROOT/private"
readonly artifact_root="$ROOT/target/mobile-visual-accessibility/android/$OXID_ANDROID_DEVICE"
readonly OUTCOMES="$PRIVATE_ROOT/scenario-outcomes.jsonl"

cleanup() {
  local status=$? final_status finished duration outcomes screenshot_count debug_count
  local root_ready=false raw_artifacts_removed=false private_diagnostics_removed=false
  trap - EXIT INT TERM HUP
  set +e
  final_status="$status"
  mkdir -p "$RUN_ROOT" && root_ready=true
  outcomes='[]'
  [ ! -f "$OUTCOMES" ] || outcomes="$(jq -s . "$OUTCOMES" 2>/dev/null)"
  rm -rf -- "$artifact_root"
  if [ ! -e "$artifact_root" ] && [ ! -L "$artifact_root" ]; then
    raw_artifacts_removed=true
    screenshot_count=0
    debug_count=0
  else
    screenshot_count="$(find "$artifact_root" -type f \( -name '*.png' -o -name '*.jpg' \) 2>/dev/null | wc -l | tr -d ' ')"
    debug_count="$(find "$artifact_root" -type f 2>/dev/null | wc -l | tr -d ' ')"
  fi
  rm -rf -- "$PRIVATE_ROOT"
  if [ ! -e "$PRIVATE_ROOT" ] && [ ! -L "$PRIVATE_ROOT" ]; then private_diagnostics_removed=true; fi
  if [ "$root_ready" != true ] || [ "$raw_artifacts_removed" != true ] || [ "$private_diagnostics_removed" != true ]; then
    final_status=1
  fi
  finished="$(date +%s)"
  duration=$((finished - STARTED_AT))
  if ! jq -n --arg head "$HEAD" --arg device "$OXID_ANDROID_DEVICE" --argjson duration "$duration" \
    --argjson passed "$( [ "$final_status" -eq 0 ] && printf true || printf false )" \
    --argjson scenario_outcomes "$outcomes" --argjson screenshots "$screenshot_count" \
    --argjson debug "$debug_count" --argjson rawRemoved "$raw_artifacts_removed" \
    --argjson privateRemoved "$private_diagnostics_removed" \
    '{schema:"oxid-android-maestro-semantic-evidence-v1",oxid:{head:$head},platform:{kind:"android_emulator",serial:$device,operatorDeclaredDisposable:true},outcome:{passed:$passed,durationSeconds:$duration,scenarios:$scenario_outcomes},artifacts:{screenshotsRetained:$screenshots,debugArtifactsRetained:$debug},cleanup:{rawArtifactsRemoved:$rawRemoved,privateDiagnosticsRemoved:$privateRemoved}}' >"$RUN_ROOT/receipt.json"; then
    final_status=1
  elif ! chmod 644 "$RUN_ROOT/receipt.json"; then
    final_status=1
  fi
  if [ "$final_status" -eq 0 ]; then
    printf 'android-maestro-semantic-evidence: PASS receipt=%s\n' "$RUN_ROOT/receipt.json"
  fi
  exit "$final_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

[ -z "$(git -C "$ROOT" status --porcelain)" ] || { echo "android-maestro-semantic-evidence: dirty-source" >&2; exit 1; }
mkdir -p "$RUN_ROOT"
mkdir -m 700 "$PRIVATE_ROOT"
scenarios=()
while IFS= read -r scenario; do
  scenarios+=("$scenario")
done < <(jq -r '.scenarios[] | select(.authority == "maestro" and (.platforms | index("android"))) | .id' "$ROOT/tests/maestro/inventory.json")
[ "${#scenarios[@]}" -gt 0 ] || { echo "android-maestro-semantic-evidence: empty scenario inventory" >&2; exit 1; }
for scenario in "${scenarios[@]}"; do
  composition="$(jq -r --arg id "$scenario" '.scenarios[] | select(.id == $id) | .composition' "$ROOT/tests/maestro/inventory.json")"
  if "$ROOT/scripts/run-maestro-android.sh" --composition "$composition" --flow "$scenario"; then
    jq -cn --arg id "$scenario" '{id:$id,outcome:"passed"}' >>"$OUTCOMES"
  else
    jq -cn --arg id "$scenario" '{id:$id,outcome:"failed"}' >>"$OUTCOMES"
    echo "android-maestro-semantic-evidence: FAIL scenario=$scenario" >&2
    exit 1
  fi
  rm -rf -- "$artifact_root"
done
