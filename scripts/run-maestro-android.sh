#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run only against the existing repository-owned emulator helper; never a phone.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
: "${OXID_ANDROID_DEVICE:?set OXID_ANDROID_DEVICE to an emulator-* serial}"
case "$OXID_ANDROID_DEVICE" in emulator-[0-9]*) ;; *) echo "refusing non-emulator device" >&2; exit 2;; esac
[[ "$OXID_ANDROID_DEVICE" =~ ^emulator-[0-9]+$ ]] || { echo "refusing malformed emulator serial" >&2; exit 2; }
umask 077

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
      and (.platforms | index("android"))
    ) | .flow) // empty
  ' "$inventory"
)"
if ! [[ "$flow" =~ ^flows/[a-z0-9-]+\.yaml$ ]] || [ ! -f "tests/maestro/$flow" ]; then
  echo "unknown, unsupported, or lower-layer-only inventory flow: $flow_id" >&2
  exit 2
fi
flow="tests/maestro/$flow"
artifact_root="$root/target/mobile-visual-accessibility/android/$OXID_ANDROID_DEVICE"
debug_root="$artifact_root/debug"
mkdir -p "$debug_root"

cleanup() {
  local status=$?
  trap - EXIT
  if [ "$status" -ne 0 ]; then
    rm -rf -- "$artifact_root"
  fi
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
  OXID_ANDROID_DEVICE="$OXID_ANDROID_DEVICE"
  OXID_ANDROID_REQUIRE_EMULATOR=1
  OXID_STANDALONE_NETWORK_PROFILE=simulated
  OXID_MOBILE_CUSTODY=development
  OXID_UI_PROFILE="$composition"
)
run_phase build "${launcher_environment[@]}" ./scripts/run-android-emulator.sh ensure
run_phase deploy "${launcher_environment[@]}" ./scripts/run-android-emulator.sh deploy
run_phase maestro nix run .#maestro -- test "$flow" \
  --device "$OXID_ANDROID_DEVICE" --test-output-dir "$artifact_root" \
  --debug-output "$debug_root"
