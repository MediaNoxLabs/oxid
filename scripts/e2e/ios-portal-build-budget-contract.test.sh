#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
readonly RUNNER="$ROOT/scripts/test-ios-portal-exact-sequence-simulator.sh"
readonly SUPERVISOR="$ROOT/scripts/e2e/ios-xcode-supervisor.mjs"
readonly CLANG_WRAPPER="$ROOT/scripts/e2e/xcode-swift-only-clang-wrapper.sh"
readonly JUSTFILE="$ROOT/Justfile"

fail() {
  printf 'ios-portal-build-budget-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -f "$RUNNER" ] && [ ! -L "$RUNNER" ] || fail runner

grep -qF 'readonly DIAGNOSTIC_ROOT="$ROOT/target/ios-portal-diagnostic"' "$RUNNER" || fail diagnostic-root
grep -qF '  --diagnostic-phase)' "$RUNNER" || fail diagnostic-operation
for phase in cold-route prepare-holder route-refuse malformed protocol-error protocol-timeout issue-error issue restored; do
  grep -qF "  $phase)" "$RUNNER" || fail "diagnostic-phase-$phase"
done
grep -qF 'fail diagnostic-phase' "$RUNNER" || fail diagnostic-phase-reject
grep -qF '&& [ "$OPERATION" = run ]; then write_evidence || cleanup_ok=false' "$RUNNER" || fail diagnostic-evidence
grep -qF 'oxid-ios-portal-diagnostic-cache-v1' "$RUNNER" || fail diagnostic-cache-schema
grep -qF 'diagnostic_artifact_valid() {' "$RUNNER" || fail diagnostic-cache-validation
grep -qF 'prepare_diagnostic_app() {' "$RUNNER" || fail diagnostic-cache-reuse
grep -qF 'oxid-ios-portal-focused-diagnostic-v1' "$RUNNER" || fail diagnostic-result-schema
grep -qF 'acceptanceEvidence:false,canonicalReceiptTouched:false' "$RUNNER" || fail diagnostic-non-acceptance
diagnostic_cold_route="$(sed -n '/^run_diagnostic_cold_route() {$/,/^}$/p' "$RUNNER")"
grep -qF 'deliver_offer || return 1' <<<"$diagnostic_cold_route" || fail diagnostic-cold-route-delivery
if grep -qF ' terminate ' <<<"$diagnostic_cold_route"; then fail diagnostic-cold-route-stop; fi
grep -qF 'if [ "$DIAGNOSTIC_PHASE" != cold-route ]; then run_diagnostic_cold_route || return 1; fi' "$RUNNER" \
  || fail diagnostic-profile-prerequisite
grep -qF 'prepare_diagnostic_holder || return 1' "$RUNNER" || fail diagnostic-holder-prerequisite
grep -qF 'ios-portal-diagnostic phase:' "$JUSTFILE" || fail diagnostic-owner-command

validation_line="$(grep -nF 'case "$OPERATION" in' "$RUNNER" | head -n 1 | cut -d: -f1)"
supervisor_line="$(grep -nF 'source "$OWNERSHIP_SUPPORT"' "$RUNNER" | head -n 1 | cut -d: -f1)"
[ -n "$validation_line" ] && [ -n "$supervisor_line" ] && [ "$validation_line" -lt "$supervisor_line" ] \
  || fail diagnostic-validation-order
unknown_phase="not-reviewed-$$"
if diagnostic_error="$("$RUNNER" --diagnostic-phase "$unknown_phase" 2>&1)"; then fail diagnostic-unknown-accepted; fi
grep -qF 'FAIL phase=diagnostic-phase' <<<"$diagnostic_error" || fail diagnostic-unknown-message
[ ! -e "$ROOT/target/ios-portal-diagnostic/$unknown_phase" ] || fail diagnostic-unknown-mutated
if "$RUNNER" --diagnostic-phase "$unknown_phase" unexpected >/dev/null 2>&1; then fail diagnostic-ambiguous-accepted; fi
[ ! -e "$ROOT/target/ios-portal-diagnostic/$unknown_phase" ] || fail diagnostic-ambiguous-mutated

grep -qF 'readonly XCTEST_COLD_BUILD_TIMEOUT_SECONDS=1800' "$RUNNER" || fail cold-build-budget
grep -qF 'readonly XCTEST_SCENARIO_TIMEOUT_SECONDS=600' "$RUNNER" || fail scenario-budget
grep -qF 'readonly PORTAL_JOURNEY_TIMEOUT_SECONDS=5400' "$RUNNER" || fail journey-budget
grep -qF 'readonly PORTAL_ACCEPTANCE_TIMEOUT_SECONDS=9000' "$RUNNER" || fail acceptance-budget
grep -qF 'const MAX_TIMEOUT_SECONDS = 9000;' "$SUPERVISOR" || fail supervisor-budget
grep -qF 'CC="$xcode_clang_wrapper" LD="$xcode_clang"' "$RUNNER" || fail xcode-tools
[ -x "$CLANG_WRAPPER" ] && [ ! -L "$CLANG_WRAPPER" ] || fail clang-wrapper
probe_output="$(OXID_XCODE_REAL_CLANG=/bin/echo "$CLANG_WRAPPER" -E -dM /dev/null)" || fail clang-probe
[ -z "$probe_output" ] || fail clang-probe-output
delegate_output="$(OXID_XCODE_REAL_CLANG=/bin/echo "$CLANG_WRAPPER" delegated)" || fail clang-delegate
[ "$delegate_output" = delegated ] || fail clang-delegate-output
grep -qF 'run_ios_build_for_testing() {' "$RUNNER" || fail build-function
grep -qF 'build-for-testing -project "$xcode_project/OxidMobileSmoke.xcodeproj"' "$RUNNER" || fail build-command
grep -qF 'test-without-building -project "$xcode_project/OxidMobileSmoke.xcodeproj"' "$RUNNER" || fail scenario-command
grep -qF 'oxid_ios_run_xctest "$ROOT" portal-build-for-testing "$XCTEST_COLD_BUILD_TIMEOUT_SECONDS"' "$RUNNER" \
  || fail build-supervision
grep -qF 'oxid_ios_run_xctest "$ROOT" "$scenario_name" "$XCTEST_SCENARIO_TIMEOUT_SECONDS"' "$RUNNER" \
  || fail scenario-supervision
if grep -Eq '/usr/bin/xcodebuild test([[:space:]]|$)' "$RUNNER"; then fail scenario-rebuild; fi
grep -qF 'run_deadline 5 rg -qF "$PACKAGE" < <(printf '\''%s\n'\'' "$launch_list")' "$RUNNER" \
  || fail process-absence-stream
if grep -qF '<<<"$launch_list"' "$RUNNER"; then fail process-absence-here-string; fi

build_line="$(grep -nF 'run_ios_build_for_testing || fail xctest-build' "$RUNNER" | cut -d: -f1)"
deadline_line="$(grep -nF 'journey_deadline=$((SECONDS + PORTAL_JOURNEY_TIMEOUT_SECONDS))' "$RUNNER" | cut -d: -f1)"
first_scenario_line="$(grep -nF 'run_ios_test testColdRoute || fail cold-route' "$RUNNER" | cut -d: -f1)"
[ -n "$build_line" ] && [ -n "$deadline_line" ] && [ -n "$first_scenario_line" ] || fail ordering-input
[ "$build_line" -lt "$deadline_line" ] && [ "$deadline_line" -lt "$first_scenario_line" ] || fail bundle-before-journey

printf 'ios-portal-build-budget-contract: PASS build=1800 scenario=600 journey=5400 mode=test-without-building\n'
