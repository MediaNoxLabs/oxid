#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run the checked-in local-only pilot against a previously booted simulated build.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
: "${OXID_IOS_DEVICE:?set OXID_IOS_DEVICE to the receipt-owned simulator UDID}"
# The app is built and installed by the existing reproducible iOS launcher.
OXID_IOS_DEVICE="$OXID_IOS_DEVICE" OXID_IOS_RESET_DATA=1 \
  OXID_STANDALONE_NETWORK_PROFILE=simulated OXID_UI_PROFILE=demo \
  ./scripts/run-ios-simulator.sh deploy
artifact_root="$root/target/mobile-visual-accessibility/ios/$OXID_IOS_DEVICE"
mkdir -p "$artifact_root"
exec nix run .#maestro -- test tests/maestro/ios-lunar-aegis.yaml \
  --udid "$OXID_IOS_DEVICE" --test-output-dir "$artifact_root"
