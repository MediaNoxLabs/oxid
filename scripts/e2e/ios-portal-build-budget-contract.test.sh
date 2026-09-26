#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
readonly RUNNER="$ROOT/scripts/test-ios-portal-exact-sequence-simulator.sh"
readonly SUPERVISOR="$ROOT/scripts/e2e/ios-xcode-supervisor.mjs"
readonly CLANG_WRAPPER="$ROOT/scripts/e2e/xcode-swift-only-clang-wrapper.sh"

fail() {
  printf 'ios-portal-build-budget-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -f "$RUNNER" ] && [ ! -L "$RUNNER" ] || fail runner

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

build_line="$(grep -nF 'run_ios_build_for_testing || fail xctest-build' "$RUNNER" | cut -d: -f1)"
deadline_line="$(grep -nF 'journey_deadline=$((SECONDS + PORTAL_JOURNEY_TIMEOUT_SECONDS))' "$RUNNER" | cut -d: -f1)"
first_scenario_line="$(grep -nF 'run_ios_test testColdRoute || fail cold-route' "$RUNNER" | cut -d: -f1)"
[ -n "$build_line" ] && [ -n "$deadline_line" ] && [ -n "$first_scenario_line" ] || fail ordering-input
[ "$build_line" -lt "$deadline_line" ] && [ "$deadline_line" -lt "$first_scenario_line" ] || fail bundle-before-journey

printf 'ios-portal-build-budget-contract: PASS build=1800 scenario=600 journey=5400 mode=test-without-building\n'
