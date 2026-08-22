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

for port in 6300 8088 18093 9944 18090; do
  rg -q "${port}" scripts/test-android-portal-flow.sh || {
    echo "Android Portal harness is missing exact reverse port $port." >&2
    exit 1
  }
done

# The activation button changes its visible label to `Activating…` before the
# development custody task is complete. Waiting only for the old label to
# disappear races route navigation against that task and can cancel it when the
# Wallet page unmounts. Require the Android flow to wait for the stable aria
# control itself to leave the DOM before it creates a managed DID.
activation_complete_wait='!document.querySelector('\''button[aria-label="Activate protected Midnight account"]'\'') && Boolean(${button("Use my receive address")})'
if ! rg -qF "$activation_complete_wait" tests/mobile/android-portal-flow.mjs; then
  echo "Android Portal flow must wait for development custody activation to complete." >&2
  exit 1
fi

# A startup failure (fetch, worktree add, support spawn, ready wait, manifest
# check) must still remove whatever was already created. That only holds if
# portal_mobile_cleanup is trapped before any of those side effects run, so
# require the trap to be the very first statement in portal_mobile_start and
# to be installed exactly once.
start_body="$(awk '
  /^portal_mobile_start\(\) \{/ { capture=1; next }
  capture && /^}/ { exit }
  capture { print }
' scripts/e2e/portal-mobile-harness-lib.sh)"
first_statement="$(awk 'NF && $0 !~ /^[[:space:]]*#/ { print; exit }' <<<"$start_body")"
if [[ "$first_statement" != *"trap 'portal_mobile_cleanup' EXIT INT TERM"* ]]; then
  echo "portal_mobile_start must install its cleanup trap before its first side effect." >&2
  exit 1
fi
trap_installations="$(grep -c "trap 'portal_mobile_cleanup' EXIT INT TERM" scripts/e2e/portal-mobile-harness-lib.sh)"
if [ "$trap_installations" -ne 1 ]; then
  echo "portal_mobile_cleanup must be trapped exactly once, at the top of portal_mobile_start." >&2
  exit 1
fi

# The real single-use offer must never be a host `simctl openurl` argument
# (visible via `ps`/Activity Monitor for the host process's lifetime). Only a
# fixed, non-secret trigger constant may reach that call; the app fetches the
# real offer itself over a loopback GET, entirely inside the simulator.
if rg -n 'simctl.*openurl.*iosDevice, offer\]' scripts/e2e/portal-mobile-support.mjs; then
  echo "iOS delivery must not pass the real offer to simctl openurl argv." >&2
  exit 1
fi
ios_trigger="openid-credential-offer://standalone-portal-test-fetch"
for source_file in \
  scripts/e2e/portal-mobile-support.mjs \
  crates/adapters/identity-ingress/src/lib.rs; do
  rg -qF "$ios_trigger" "$source_file" || {
    echo "iOS non-secret loopback test trigger is missing or drifted in $source_file." >&2
    exit 1
  }
done
rg -q 'loopback-test-offer-trigger' \
  crates/adapters/identity-ingress/Cargo.toml \
  crates/composition/Cargo.toml || {
  echo "The loopback test-offer trigger feature must be wired from composition's mobile-portal feature." >&2
  exit 1
}

echo "Portal mobile harness syntax, sequence, compile-time markers, route exclusions, and cleanup-trap ordering passed."
