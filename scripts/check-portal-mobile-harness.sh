#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

bash -n \
  scripts/e2e/portal-mobile-harness-lib.sh \
  scripts/run-ios-simulator.sh \
  scripts/run-android-emulator.sh \
  scripts/test-ios-portal-flow.sh \
  scripts/test-android-portal-flow.sh
node --check scripts/e2e/portal-mobile-support.mjs
node --check scripts/e2e/portal-mobile-holder-sync.mjs
node --check tests/mobile/android-portal-flow.mjs

if rg -n '10\.0\.2\.2|set -x|reverse --remove-all' \
  scripts/e2e/portal-mobile-* \
  scripts/test-ios-portal-flow.sh \
  scripts/test-android-portal-flow.sh \
  tests/mobile/android-portal-flow.mjs; then
  echo "Portal mobile harness contains a forbidden route, trace mode, or broad reverse cleanup." >&2
  exit 1
fi

portal_recipe="$({ awk '
  /^portal-mobile-smoke:/ { capture=1; next }
  capture && /^[^[:space:]#]/ { exit }
  capture { print }
' Justfile; } || true)"
ios_line="$(grep -n 'test-ios-portal-flow.sh' <<<"$portal_recipe" | cut -d: -f1)"
android_line="$(grep -n 'test-android-portal-flow.sh' <<<"$portal_recipe" | cut -d: -f1)"
if [ -z "$ios_line" ] || [ -z "$android_line" ] || [ "$ios_line" -ge "$android_line" ]; then
  echo "portal-mobile-smoke must run iOS before Android and never in parallel." >&2
  exit 1
fi

for marker in \
  OXID_STANDALONE_PORTAL_PROFILE \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256; do
  rg -q "$marker" apps/oxid scripts || {
    echo "Portal compile-time profile marker is missing: $marker" >&2
    exit 1
  }
done

for port in 6300 8088 9092 9944 18090; do
  rg -q "${port}" scripts/test-android-portal-flow.sh || {
    echo "Android Portal harness is missing exact reverse port $port." >&2
    exit 1
  }
done

echo "Portal mobile harness syntax, sequence, compile-time markers, and route exclusions passed."
