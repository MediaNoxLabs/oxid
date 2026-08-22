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

if rg -n '10\.0\.2\.2|set -x|(?:reverse|forward) --remove-all' \
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

for port in 6300 8088 18091 18093 9944 18090; do
  rg -q "${port}" scripts/test-android-portal-flow.sh || {
    echo "Android Portal harness is missing exact reverse port $port." >&2
    exit 1
  }
done

# Portal verification has no future-time slack. The disposable QEMU clock must
# be set from the host through Android's privileged alarm service, then retain
# the original strict ±2-second assertion; merely widening the assertion can
# admit a credential whose issuance time is still in the wallet's future.
if ! rg -qF 'shell cmd alarm set-time' scripts/test-android-portal-flow.sh ||
  ! rg -q 'clock_skew.*-lt -2.*clock_skew.*-gt 2' scripts/test-android-portal-flow.sh; then
  echo "Android Portal flow must synchronize QEMU time and enforce the strict skew bound." >&2
  exit 1
fi

# The activation button changes its visible label to `Activating…` before the
# development custody task is complete. Waiting only for the old label to
# disappear races route navigation against that task and can cancel it when the
# Wallet page unmounts. Both initial activation and restored reactivation must
# reuse the stable aria control predicate before navigating away.
activation_complete_wait='!document.querySelector('\''button[aria-label="Activate protected Midnight account"]'\'') && Boolean(${button("Use my receive address")})'
if [ "$(rg -cF "$activation_complete_wait" tests/mobile/android-portal-flow.mjs)" -ne 2 ]; then
  echo "Both Android custody waits must use the stable activation-control predicate." >&2
  exit 1
fi

# A startup failure (lock, fetch, worktree add, support spawn, bounded ready
# wait, manifest check) must still remove whatever was already created. EXIT is
# the single cleanup owner; INT/TERM must stop control flow with conventional
# statuses rather than run cleanup and then resume an interrupted statement.
start_body="$(awk '
  /^portal_mobile_start\(\) \{/ { capture=1; next }
  capture && /^}/ { exit }
  capture { print }
' scripts/e2e/portal-mobile-harness-lib.sh)"
first_statement="$(awk 'NF && $0 !~ /^[[:space:]]*#/ { print; exit }' <<<"$start_body")"
if [[ "$first_statement" != *"trap 'portal_mobile_cleanup' EXIT"* ]]; then
  echo "portal_mobile_start must install its EXIT cleanup trap before its first side effect." >&2
  exit 1
fi
for signal_trap in "trap 'exit 130' INT" "trap 'exit 143' TERM"; do
  if [ "$(grep -cF "$signal_trap" scripts/e2e/portal-mobile-harness-lib.sh)" -ne 1 ]; then
    echo "Portal mobile signal handler is missing or duplicated: $signal_trap" >&2
    exit 1
  fi
done
if [ "$(grep -cF "trap 'portal_mobile_cleanup' EXIT" scripts/e2e/portal-mobile-harness-lib.sh)" -ne 1 ]; then
  echo "portal_mobile_cleanup must have exactly one early EXIT trap." >&2
  exit 1
fi

for bounded_startup_marker in \
  'exec 9<>"$ready_fifo"' \
  'read -r -t "$PORTAL_MOBILE_READY_TIMEOUT_SECONDS" -u 9' \
  'portal_mobile_wait_bounded' \
  '--max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS"'; do
  rg -qF -- "$bounded_startup_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Portal support lifecycle bound is missing: $bounded_startup_marker" >&2
    exit 1
  }
done
if rg -n '^[[:space:]]*wait "\$PORTAL_MOBILE_(SUPPORT|HOLDER_SYNC)_PID"' \
  scripts/e2e/portal-mobile-harness-lib.sh scripts/test-ios-portal-flow.sh; then
  echo "Portal support processes must never use an unbounded direct wait." >&2
  exit 1
fi
if ! rg -qF 'rm -rf "$PORTAL_MOBILE_STATE_DIR"' scripts/e2e/portal-mobile-harness-lib.sh ||
  rg -qF 'private failure artifacts=' scripts/e2e/portal-mobile-harness-lib.sh; then
  echo "Portal private runtime must be removed on every exit." >&2
  exit 1
fi

# Every synchronous support command is bounded, cleanup failures become the
# child status observed by the shell, and iOS delivery uses xcode-select's
# selected developer directory rather than a machine-specific Xcode path.
if [ "$(rg -c 'timeout: (timeoutMs|HOST_COMMAND_TIMEOUT_MS)' scripts/e2e/portal-mobile-support.mjs)" -ne 3 ] ||
  [ "$(rg -c 'killSignal: "SIGKILL"' scripts/e2e/portal-mobile-support.mjs)" -ne 3 ] ||
  ! rg -qF 'named compose project was not empty after compose-down' scripts/e2e/portal-mobile-support.mjs ||
  ! rg -qF 'process.exitCode = 1' scripts/e2e/portal-mobile-support.mjs ||
  ! rg -qF 'DEVELOPER_DIR: xcodeDeveloperDirectory' scripts/e2e/portal-mobile-support.mjs ||
  rg -qF '/Applications/Xcode.app/Contents/Developer' scripts/e2e/portal-mobile-support.mjs; then
  echo "Portal support command bounds, cleanup propagation, or selected Xcode wiring regressed." >&2
  exit 1
fi

# The real single-use offer must never enter host/device argv, OS URL/intent
# state, logs, evidence, or a retained staging file. Both mobile OS paths may
# deliver only the same fixed, non-secret trigger; the app's named worker
# retrieves the offer over bounded loopback HTTP.
if rg -n 'simctl.*openurl.*iosDevice, offer\]' scripts/e2e/portal-mobile-support.mjs; then
  echo "iOS delivery must not pass the real offer to simctl openurl argv." >&2
  exit 1
fi
if rg -n 'remote_offer_file|oxid-portal-offer|/data/local/tmp/.*offer|cat >.*offer|value=.*cat|PORTAL_MOBILE_CONTROL_ORIGIN/offer' \
  scripts/test-android-portal-flow.sh; then
  echo "Android delivery must not fetch, stage, or expand the real offer." >&2
  exit 1
fi
mobile_trigger="openid-credential-offer://standalone-portal-test-fetch"
for source_file in \
  scripts/e2e/portal-mobile-support.mjs \
  scripts/test-android-portal-flow.sh \
  crates/adapters/identity-ingress/src/lib.rs; do
  rg -qF "$mobile_trigger" "$source_file" || {
    echo "The non-secret mobile loopback trigger is missing or drifted in $source_file." >&2
    exit 1
  }
done
rg -qF -- '-d "$portal_test_offer_trigger"' scripts/test-android-portal-flow.sh || {
  echo "Android must deliver only its fixed Portal trigger to am start -d." >&2
  exit 1
}
if [ "$(rg -c 'loopback-test-offer-trigger' crates/composition/Cargo.toml)" -ne 1 ]; then
  echo "Composition must wire the loopback trigger exactly once." >&2
  exit 1
fi
for worker_bound in \
  oxid-portal-offer-fetch \
  connect_timeout \
  CONTROL_TIMEOUT \
  MAX_RESPONSE_BYTES; do
  rg -qF "$worker_bound" crates/adapters/identity-ingress/src/lib.rs || {
    echo "Portal trigger worker bound is missing: $worker_bound" >&2
    exit 1
  }
done

# Evidence is bound to the startup-clean Oxid revision, never a later HEAD.
for platform_script in scripts/test-ios-portal-flow.sh scripts/test-android-portal-flow.sh; do
  rg -qF 'portal_mobile_assert_evidence_source || exit 1' "$platform_script" &&
    rg -qF -- '--arg head "$PORTAL_MOBILE_OXID_HEAD"' "$platform_script" || {
    echo "Portal evidence source pin is missing in $platform_script." >&2
    exit 1
  }
done
if [ "$(rg -cF 'portal_mobile_assert_evidence_source' scripts/e2e/portal-mobile-harness-lib.sh)" -ne 1 ]; then
  echo "Portal evidence must fail closed through the shared source check." >&2
  exit 1
fi

# CDP uses a dynamically allocated, exactly owned forward. Both opening the
# socket and every command are bounded, and terminal WebSocket events reject
# all pending commands so top-level await cannot remain unsettled.
for cdp_marker in \
  '"tcp:0" "localabstract:webview_devtools_remote_$process_id"' \
  'forward --remove "tcp:$devtools_port"' \
  'CDP_OPEN_TIMEOUT_MS' \
  'CDP_COMMAND_TIMEOUT_MS' \
  'rejectPending(new Error("CDP connection closed"))' \
  'rejectPending(new Error("CDP connection failed"))'; do
  rg -qF "$cdp_marker" scripts/test-android-portal-flow.sh tests/mobile/android-portal-flow.mjs || {
    echo "Bounded exact CDP ownership marker is missing: $cdp_marker" >&2
    exit 1
  }
done

# Android scalar values strip both line-ending characters, and epoch values are
# checked before shell arithmetic. Timeout tests must observe the disabled,
# accessible loading control before they accept the terminal error.
if rg -n "tr -d '\\\\r'|tr -d '\\\\n'" scripts/test-android-portal-flow.sh ||
  ! rg -qF '"$emulator_epoch" =~ ^[0-9]+$' scripts/test-android-portal-flow.sh; then
  echo "Android scalar normalization or epoch validation regressed." >&2
  exit 1
fi
for busy_marker in \
  'accessible disabled offer-check busy state' \
  'application.buttons["Checking offer…"]' \
  'The in-progress offer check must be disabled'; do
  rg -qF "$busy_marker" tests/mobile/android-portal-flow.mjs tests/mobile/ios/OxidUITests/PortalFlowTests.swift || {
    echo "Portal timeout busy-state assertion is missing: $busy_marker" >&2
    exit 1
  }
done

for workflow in .github/workflows/ci.yml .github/workflows/quality.yml .github/workflows/scan.yml; do
  rg -q '^    branches: \[develop, integration, main\]$' "$workflow" || {
    echo "Hosted PR checks do not include integration in $workflow." >&2
    exit 1
  }
done
for lock_marker in 'mkdir "$PORTAL_MOBILE_LOCK_DIR"' 'mv "$PORTAL_MOBILE_LOCK_DIR" "$stale_lock"' 'owner-pid'; do
  rg -qF "$lock_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Atomic stale-safe Portal mobile lock marker is missing: $lock_marker" >&2
    exit 1
  }
done

echo "Portal mobile harness syntax, sequence, lifecycle bounds, exact CDP ownership, evidence pinning, secret-free delivery, and hosted PR filters passed."
