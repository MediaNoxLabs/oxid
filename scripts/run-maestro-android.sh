#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run only against the existing repository-owned emulator helper; never a phone.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
: "${OXID_ANDROID_DEVICE:?set OXID_ANDROID_DEVICE to an emulator-* serial}"
case "$OXID_ANDROID_DEVICE" in emulator-*) ;; *) echo "refusing non-emulator device" >&2; exit 2;; esac
OXID_ANDROID_DEVICE="$OXID_ANDROID_DEVICE" OXID_ANDROID_REQUIRE_EMULATOR=1 \
  OXID_STANDALONE_NETWORK_PROFILE=simulated OXID_MOBILE_CUSTODY=development \
  OXID_UI_PROFILE=demo ./scripts/run-android-emulator.sh deploy
artifact_root="$root/target/mobile-visual-accessibility/android/$OXID_ANDROID_DEVICE"
mkdir -p "$artifact_root"
exec nix run .#maestro -- test tests/maestro/android-lunar-aegis.yaml \
  --device "$OXID_ANDROID_DEVICE" --test-output-dir "$artifact_root"
