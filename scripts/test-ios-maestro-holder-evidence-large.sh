#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run the privacy-safe holder evidence lane at a larger iOS viewport.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
exec env \
  OXID_IOS_EVIDENCE_VIEWPORT="402-pt-class" \
  OXID_IOS_EVIDENCE_DEVICE_TYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro" \
  "$ROOT/scripts/test-ios-maestro-holder-evidence.sh"
