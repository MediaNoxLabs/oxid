#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
export CDPATH=
export OXID_IOS_OPERATION_TIMEOUT_SECONDS="${OXID_IOS_OPERATION_TIMEOUT_SECONDS:-120}"

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly ROOT
readonly RUN_ROOT="$ROOT/target/ios-standalone-local-acceptance"
readonly PRIVATE_STATE="$RUN_ROOT/private"
readonly RECEIPT="$PRIVATE_STATE/simulator-receipt.json"
readonly STACK_STATE="$PRIVATE_STATE/standalone-stack"
readonly FAUCET_BUILD_LOG="$PRIVATE_STATE/faucet-build.log"
readonly FAUCET_LOG="$PRIVATE_STATE/faucet.log"
readonly EVIDENCE="$RUN_ROOT/evidence.json"
readonly APP_BUNDLE="$ROOT/target/dx/oxid-app/debug/ios/OxidApp.app"

# shellcheck source=e2e/ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"
oxid_ios_supervise_acceptance "$ROOT" ios-standalone-local 3600 \
  "$ROOT/scripts/test-ios-standalone-local.sh" "$@"

simulator_owned=0
stack_owned=0
faucet_pid=""
journey_passed=0

fail() {
  printf 'ios-standalone-local: FAIL phase=%s\n' "$1" >&2
  exit 1
}

stop_faucet() {
  [ -n "$faucet_pid" ] || return 0
  if kill -0 "$faucet_pid" 2>/dev/null; then
    kill "$faucet_pid" 2>/dev/null || return 1
    wait "$faucet_pid" 2>/dev/null || true
    kill -0 "$faucet_pid" 2>/dev/null && return 1
  fi
  faucet_pid=""
}

cleanup() {
  local incoming=$?
  trap - EXIT INT TERM HUP
  set +e
  stop_faucet || incoming=1
  if [ "$simulator_owned" -eq 1 ]; then
    oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$RECEIPT" >/dev/null 2>&1 || incoming=1
    simulator_owned=0
  fi
  if [ "$stack_owned" -eq 1 ]; then
    OXID_STANDALONE_STATE_DIR="$STACK_STATE" \
      "$ROOT/scripts/standalone-down.sh" >/dev/null 2>&1 || incoming=1
    stack_owned=0
  fi
  if [ "$incoming" -eq 0 ] && [ "$journey_passed" -eq 1 ]; then
    rm -rf -- "$PRIVATE_STATE"
  else
    printf 'ios-standalone-local: private diagnostics retained mode=0700\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT INT TERM HUP

[ "$(uname -s)" = Darwin ] || fail platform
[ -z "${OXID_IOS_DEVICE:-}" ] || fail ambient-device-selector
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail dirty-source

# Desktop supervisors do not always inherit the interactive shell profile even
# when Nix is installed. Admit the canonical multi-user profile explicitly.
if ! command -v nix >/dev/null 2>&1 \
  && [ -x /nix/var/nix/profiles/default/bin/nix ]; then
  export PATH="/nix/var/nix/profiles/default/bin:$PATH"
fi
for command_name in cargo curl docker git jq nix node python3 rustup shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-tool-$command_name"
done
if [ "${OXID_IOS_ACCEPTANCE_IN_NIX:-0}" != 1 ]; then
  export OXID_SKIP_PI_PROVISION=1
  exec nix develop "$ROOT" --command env \
    OXID_IOS_ACCEPTANCE_IN_NIX=1 "$0" "$@"
fi
command -v timeout >/dev/null 2>&1 || fail timeout-capability
python3 -c 'from PIL import Image' >/dev/null 2>&1 || fail python-pillow
[ -x /usr/bin/xcodebuild ] && [ -x /usr/bin/xcrun ] && [ -x /usr/bin/plutil ] \
  || fail xcode-tools
docker info >/dev/null 2>&1 || fail docker

HEAD="$(git -C "$ROOT" rev-parse HEAD)" || fail source-head
TREE="$(git -C "$ROOT" rev-parse 'HEAD^{tree}')" || fail source-tree
readonly HEAD TREE
[[ "$HEAD" =~ ^[0-9a-f]{40}$ && "$TREE" =~ ^[0-9a-f]{40}$ ]] || fail source-head
git -C "$ROOT" verify-commit "$HEAD" >/dev/null 2>&1 || fail source-signature

[ ! -L "$ROOT/target" ] || fail target-symlink
mkdir -p -- "$ROOT/target" || fail target-directory
[ -d "$ROOT/target" ] || fail target-directory
[ ! -e "$RUN_ROOT" ] && [ ! -L "$RUN_ROOT" ] || fail occupied-evidence
existing_stack="$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --quiet)"
[ -z "$existing_stack" ] || fail occupied-standalone-stack
mkdir -m 700 "$RUN_ROOT" "$PRIVATE_STATE" || fail private-state

timeout -k 30s 1800s cargo build --manifest-path "$ROOT/Cargo.toml" --locked \
  -p oxid-headless --features standalone-faucet \
  --bin oxid-standalone-faucet-http >"$FAUCET_BUILD_LOG" 2>&1 \
  || fail faucet-build

DEVELOPER_DIR_SELECTED="$(
  oxid_ios_discover_developer_directory "${OXID_XCODE_DEVELOPER_DIR:-}"
)" || fail developer-directory
readonly DEVELOPER_DIR_SELECTED
selector_pair="$(
  oxid_ios_resolve_selectors "$DEVELOPER_DIR_SELECTED" \
    "${OXID_IOS_RUNTIME_ID:-com.apple.CoreSimulator.SimRuntime.iOS-17-5}" \
    "${OXID_IOS_DEVICE_TYPE_ID:-com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation}"
)" || fail selectors
IFS=$'\t' read -r RUNTIME_ID DEVICE_TYPE_ID <<<"$selector_pair"
readonly RUNTIME_ID DEVICE_TYPE_ID
readonly DEVICE_NAME="oxid-standalone-${HEAD:0:12}"
DEVICE="$(
  oxid_ios_create_owned "$DEVELOPER_DIR_SELECTED" "$RUNTIME_ID" \
    "$DEVICE_TYPE_ID" "$DEVICE_NAME" "$RECEIPT"
)" || fail simulator-create
readonly DEVICE
simulator_owned=1
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" boot >/dev/null \
  || fail simulator-boot
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" bootstatus -b >/dev/null \
  || fail simulator-ready

stack_owned=1
OXID_STANDALONE_STATE_DIR="$STACK_STATE" "$ROOT/scripts/standalone-up.sh" local \
  || fail standalone-up

"$ROOT/scripts/run-standalone-faucet-http.sh" >"$FAUCET_LOG" 2>&1 &
faucet_pid=$!
faucet_ready=0
for _attempt in $(seq 1 120); do
  kill -0 "$faucet_pid" 2>/dev/null || fail faucet-exited
  if curl --fail --silent --max-time 2 http://127.0.0.1:36301/health >/dev/null; then
    faucet_ready=1
    break
  fi
  sleep 1
done
[ "$faucet_ready" -eq 1 ] || fail faucet-readiness

OXID_IOS_DEVICE="$DEVICE" \
OXID_IOS_RESET_DATA=1 \
OXID_STANDALONE_NETWORK_PROFILE=local \
OXID_XCODE_DEVELOPER_DIR="$DEVELOPER_DIR_SELECTED" \
  "$ROOT/scripts/run-ios-simulator.sh" build || fail app-build
oxid_ios_owned_simctl "$DEVELOPER_DIR_SELECTED" "$RECEIPT" install "$APP_BUNDLE" >/dev/null \
  || fail app-install

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)" || fail xcodegen-build
generated_project_root="$PRIVATE_STATE/xcode"
mkdir -m 700 "$generated_project_root" || fail xcode-project-state
OXID_REPOSITORY_ROOT="$ROOT" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$ROOT/tests/mobile/ios/project.yml" \
    --project "$generated_project_root" >/dev/null || fail xcodegen

host_user="$(id -un)"
started_at="$(date +%s)"
oxid_ios_run_xctest "$ROOT" standalone-local-account 1800 env -i \
  "DEVELOPER_DIR=$DEVELOPER_DIR_SELECTED" \
  "HOME=$HOME" \
  "LANG=${LANG:-en_US.UTF-8}" \
  "LOGNAME=$host_user" \
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
  "TMPDIR=${TMPDIR:-/tmp}" \
  "USER=$host_user" \
  /usr/bin/xcodebuild test \
    -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
    -scheme OxidUITests \
    -destination "platform=iOS Simulator,id=$DEVICE" \
    -derivedDataPath "$PRIVATE_STATE/derived-data" \
    -only-testing:"OxidUITests/StandaloneLocalAccountTests/testSynchronizesProtectedAccountFromLocalStandaloneStack" \
    CODE_SIGNING_ALLOWED=NO || fail xctest
finished_at="$(date +%s)"

[ "$(git -C "$ROOT" rev-parse HEAD)" = "$HEAD" ] || fail head-changed
[ "$(git -C "$ROOT" rev-parse 'HEAD^{tree}')" = "$TREE" ] || fail tree-changed
[ -z "$(git -C "$ROOT" status --porcelain)" ] || fail source-changed

stop_faucet || fail faucet-cleanup
oxid_ios_delete_owned "$DEVELOPER_DIR_SELECTED" "$RECEIPT" >/dev/null \
  || fail simulator-cleanup
simulator_owned=0
OXID_STANDALONE_STATE_DIR="$STACK_STATE" "$ROOT/scripts/standalone-down.sh" >/dev/null \
  || fail standalone-cleanup
stack_owned=0
rm -rf -- "$PRIVATE_STATE" || fail private-cleanup
[ ! -e "$PRIVATE_STATE" ] || fail private-cleanup
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --quiet)" ] \
  || fail standalone-leak

runtime_version="$(
  oxid_ios_xcrun "$DEVELOPER_DIR_SELECTED" simctl list runtimes -j \
    | jq -er --arg runtime "$RUNTIME_ID" \
      'first(.runtimes[] | select(.identifier == $runtime) | .version)'
)" || fail runtime-version
jq -n \
  --arg head "$HEAD" --arg tree "$TREE" --arg runtime "$runtime_version" \
  --arg deviceType "$DEVICE_TYPE_ID" \
  --argjson started "$started_at" --argjson finished "$finished_at" \
  '{schema:"oxid-ios-standalone-local-evidence-v1",oxid:{head:$head,tree:$tree},
    platform:{kind:"ios_simulator",runtime:$runtime,deviceType:$deviceType},
    outcome:{passed:true,startedAtUnix:$started,finishedAtUnix:$finished,
      durationSeconds:($finished-$started),fixedGrantNight:50000,
      automaticConvergence:true,manualSyncUsed:false},
    cleanup:{receiptOwnedSimulator:true,receiptOwnedStandaloneStack:true,
      faucetChildStopped:true,privateDiagnosticsRemoved:true}}' >"$EVIDENCE" \
  || fail evidence
chmod 644 "$EVIDENCE"
journey_passed=1
printf 'ios-standalone-local: PASS evidence=%s\n' "$EVIDENCE"
