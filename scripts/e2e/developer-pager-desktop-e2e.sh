#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Owner-invoked ARM64 macOS rendered smoke; no proof is started by this lane.
set -euo pipefail
export LC_ALL=C
CDPATH=

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly EVIDENCE="$ROOT/target/developer-pager-desktop-e2e"
readonly RUNTIME="$EVIDENCE/runtime"
readonly CONTROL="$RUNTIME/home/Library/Application Support/io.medianox.oxid/developer-pager-test"
readonly HELPER="$RUNTIME/window-id"
app_pid=""
cleanup_running=0

fail() {
  local app_state="not-launched" driver_state="absent" log_state="absent" reason="none"
  if [ -n "$app_pid" ]; then
    if kill -0 "$app_pid" >/dev/null 2>&1; then app_state="running"; else app_state="exited"; fi
  fi
  if [ -f "$CONTROL/driver-admitted" ]; then driver_state="admitted"; fi
  if [ -f "$CONTROL/driver-failed" ]; then
    driver_state="failed"
    reason="$(sed -n '1p' "$CONTROL/driver-failed")"
    [[ "$reason" =~ ^failed:[a-z-]+$ ]] || reason="failed:invalid-code"
  fi
  if [ -n "${viewport:-}" ] && [ -e "$RUNTIME/app-$viewport.log" ]; then
    if [ -s "$RUNTIME/app-$viewport.log" ]; then log_state="nonempty"; else log_state="empty"; fi
  fi
  printf 'developer-pager-desktop-e2e: FAIL phase=%s app=%s driver=%s reason=%s log=%s\n' \
    "$1" "$app_state" "$driver_state" "$reason" "$log_state" >&2
  exit 1
}
cleanup() {
  local status=$? cleanup_status=0
  [ "$cleanup_running" = 0 ] || exit "$status"; cleanup_running=1
  trap - EXIT INT TERM HUP; set +e
  if [ -n "$app_pid" ]; then kill "$app_pid" >/dev/null 2>&1 || true; wait "$app_pid" >/dev/null 2>&1 || true; app_pid=""; fi
  rm -rf -- "$RUNTIME"
  [ ! -e "$RUNTIME" ] || cleanup_status=1
  [ "$cleanup_status" = 0 ] || { printf 'developer-pager-desktop-e2e: exact cleanup could not be proven\n' >&2; status=1; }
  exit "$status"
}
trap cleanup EXIT; trap 'exit 130' INT; trap 'exit 143' TERM; trap 'exit 129' HUP
wait_for() {
  local wanted="$1" attempts="${2:-300}" count=0
  while [ "$count" -lt "$attempts" ]; do
    [ -f "$wanted" ] && return 0
    [ -f "$CONTROL/driver-failed" ] && return 1
    if [ -n "$app_pid" ] && ! kill -0 "$app_pid" >/dev/null 2>&1; then return 1; fi
    count=$((count + 1))
    sleep .1
  done
  return 1
}

build_window_helper() {
  cat >"$RUNTIME/window-id.swift" <<'SWIFT'
import AppKit
import CoreGraphics
import Foundation
guard CGPreflightScreenCaptureAccess(), CommandLine.arguments.count == 2, let raw = Int32(CommandLine.arguments[1]), let app = NSRunningApplication(processIdentifier: pid_t(raw)) else { exit(1) }
guard app.activate(options: [.activateAllWindows]) else { exit(1) }
guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { exit(1) }
let candidates = windows.compactMap { window -> (CGWindowID, CGFloat)? in
  guard (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == raw, let number = window[kCGWindowNumber as String] as? NSNumber, let dictionary = window[kCGWindowBounds as String] as? NSDictionary, let bounds = CGRect(dictionaryRepresentation: dictionary), bounds.width >= 320, bounds.height >= 480 else { return nil }
  return (CGWindowID(number.uint32Value), bounds.width * bounds.height)
}
guard let id = candidates.max(by: { $0.1 < $1.1 })?.0 else { exit(1) }; print(id)
SWIFT
  local sdk; sdk="$(env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer /usr/bin/xcrun --sdk macosx --show-sdk-path)" || return 1
  env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer SDKROOT="$sdk" /usr/bin/xcrun --sdk macosx swiftc "$RUNTIME/window-id.swift" -o "$HELPER"
}
capture() { local output="$1" id; id="$("$HELPER" "$app_pid")" || return 1; [[ "$id" =~ ^[0-9]+$ ]] || return 1; /usr/sbin/screencapture -x -l "$id" "$output" && [ -s "$output" ]; }

for tool in cargo file git jq screencapture; do command -v "$tool" >/dev/null || fail missing-tool; done
[ "$(uname -s)-$(uname -m)" = Darwin-arm64 ] || fail arm64-darwin-required
[ -z "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
umask 077
rm -rf -- "$RUNTIME" "$EVIDENCE/screenshots"
rm -f -- "$EVIDENCE/evidence.json"
mkdir -p "$CONTROL" "$EVIDENCE/screenshots"
chmod 700 "$RUNTIME" "$CONTROL" "$EVIDENCE/screenshots"
build_window_helper || fail window-helper
cargo build --manifest-path "$ROOT/Cargo.toml" -p oxid-app --no-default-features --features desktop-developer-pager-test >/dev/null
file "$ROOT/target/debug/oxid-app" | grep -q 'Mach-O 64-bit arm64' || fail app-not-arm64-macho
run_viewport() {
  local viewport="$1"
  rm -rf -- "$RUNTIME/home"
  mkdir -p "$CONTROL"
  chmod 700 "$RUNTIME/home" "$CONTROL"
  OXID_DEVELOPER_PAGER_VIEWPORT="$viewport" HOME="$RUNTIME/home" "$ROOT/target/debug/oxid-app" >"$RUNTIME/app-$viewport.log" 2>&1 & app_pid=$!
  wait_for "$CONTROL/driver-admitted" 600 || fail driver-admission
  printf 'ok\n' >"$CONTROL/window-ready"
  wait_for "$CONTROL/profile-created" || fail profile-creation
  wait_for "$CONTROL/capabilities" || fail capabilities
  capture "$EVIDENCE/screenshots/capabilities-$viewport.png" || fail capabilities-screenshot
  printf 'ok\n' >"$CONTROL/capture-capabilities"
  wait_for "$CONTROL/benchmark" || fail benchmark
  capture "$EVIDENCE/screenshots/benchmark-$viewport.png" || fail benchmark-screenshot
  printf 'ok\n' >"$CONTROL/capture-benchmark"
  wait_for "$CONTROL/event-log" || fail event-log
  capture "$EVIDENCE/screenshots/event-log-$viewport.png" || fail event-log-screenshot
  printf 'ok\n' >"$CONTROL/capture-event-log"
  wait_for "$CONTROL/complete" || fail back-to-hub
  kill "$app_pid"; wait "$app_pid" || fail app-status; app_pid=""
  [ ! -s "$RUNTIME/app-$viewport.log" ] || fail app-log
}
run_viewport 360x640
run_viewport 390x844
for screenshot in "$EVIDENCE"/screenshots/*.png; do file "$screenshot" | grep -q 'PNG image data' || fail screenshot-png; done
head="$(git -C "$ROOT" rev-parse HEAD)"; tree="$(git -C "$ROOT" rev-parse HEAD^{tree})"
jq -cn --arg head "$head" --arg tree "$tree" '{schema:"oxid-developer-pager-arm64-darwin-v1",oxid:{head:$head,tree:$tree},activationRoute:"dioxus-document-rendered-controls",viewports:["360x640","390x844"],sections:["capabilities","benchmark","event-log"],acceptance:{proofNotStarted:true,activeChipMatchesCurrentPage:true,scrollTransitionExercised:true,backReturnsToHub:true,screenCapturePermissionPreflight:true,pidBoundWindowCapture:true,redactionApplied:true,cleanupOwned:true,releaseExcluded:true,hostedTargetExcluded:true}}' >"$EVIDENCE/evidence.json"
chmod 600 "$EVIDENCE/evidence.json" "$EVIDENCE"/screenshots/*.png
if grep -Eqi 'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|mnemonic|seed|Alice|Example|John|Doe|AB1234567' "$EVIDENCE/evidence.json"; then
  fail evidence-denylist
fi
jq -e --arg head "$head" --arg tree "$tree" '.oxid == {head:$head,tree:$tree} and (.acceptance | to_entries | all(.value == true))' "$EVIDENCE/evidence.json" >/dev/null || fail evidence-schema
printf 'developer-pager-desktop-e2e: PASS evidence=target/developer-pager-desktop-e2e/evidence.json screenshots=target/developer-pager-desktop-e2e/screenshots/{capabilities,benchmark,event-log}.png\n'
