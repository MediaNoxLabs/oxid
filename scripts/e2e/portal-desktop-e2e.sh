#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly REPOSITORY_ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly EVIDENCE_ROOT="$REPOSITORY_ROOT/target/portal-desktop-e2e"
readonly RUNTIME="$EVIDENCE_ROOT/runtime"
readonly HOME_ROOT="$RUNTIME/home"
readonly CONTROL_ROOT="$HOME_ROOT/Library/Application Support/io.medianox.oxid/desktop-test"
readonly APP_SUPPORT_ROOT="$HOME_ROOT/Library/Application Support/io.medianox.oxid"
readonly WALLET_ROOT="$RUNTIME/wallet"
readonly STACK_RUNTIME="$REPOSITORY_ROOT/target/portal-virtual-mobile/runtime"
readonly STACK_BUILD_ENV="$STACK_RUNTIME/build.env"
readonly STACK_CAPABILITY="$STACK_RUNTIME/portal-offer.capability"
readonly STACK_CONTROL_CONFIG="$STACK_RUNTIME/control-curl.conf"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly INDEXER_WS="ws://127.0.0.1:8088/api/v4/graphql/ws"
readonly INDEXER_HTTP="http://127.0.0.1:8088/api/v4/graphql"
readonly NODE_WS="ws://127.0.0.1:9944"
readonly PROOF_SERVER="http://127.0.0.1:6300"
readonly STANDALONE_ADDRESS="mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh"

stack_pid=""
app_pid=""
cleanup_running=0

fail() {
  local driver_failure=""
  if [ -f "$CONTROL_ROOT/driver-failed" ]; then
    driver_failure="$(cat "$CONTROL_ROOT/driver-failed")"
    [[ "$driver_failure" =~ ^failed:[a-z-]+$ ]] || driver_failure="failed:invalid-marker"
  fi
  printf 'portal-desktop-e2e: FAIL phase=%s driver=%s\n' "$1" "${driver_failure:-none}" >&2
  exit 1
}

cleanup() {
  local incoming=$? cleanup_status=0
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM HUP
  set +e
  if [ -n "$app_pid" ]; then
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
    app_pid=""
  fi
  if [ -n "$stack_pid" ]; then
    kill -TERM "$stack_pid" >/dev/null 2>&1 || true
    wait "$stack_pid" >/dev/null 2>&1 || true
    stack_pid=""
  fi
  if [ -n "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" ]; then
    cleanup_status=1
  fi
  rm -rf -- "$RUNTIME"
  if [ -e "$RUNTIME" ] || [ -L "$RUNTIME" ]; then cleanup_status=1; fi
  if [ "$cleanup_status" -ne 0 ]; then
    incoming=1
    printf 'portal-desktop-e2e: exact cleanup could not be proven\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

wait_for_file() {
  local wanted="$1" failure="${2:-}" maximum="${3:-900}" deadline
  deadline=$((SECONDS + maximum))
  while [ "$SECONDS" -lt "$deadline" ]; do
    [ -n "$failure" ] && [ -f "$failure" ] && return 2
    [ -f "$wanted" ] && return 0
    sleep 0.1
  done
  return 1
}

control_curl() {
  curl --config "$STACK_CONTROL_CONFIG" --noproxy '*' \
    --fail --silent --show-error --max-time 30 "$@"
}

launch_app() {
  local log="$1"
  HOME="$HOME_ROOT" \
  OXID_PROFILE_STORE_PATH="$WALLET_ROOT/profiles.json" \
  OXID_DID_STORE_PATH="$WALLET_ROOT/private/did-records.json" \
  OXID_CREDENTIAL_STORE_PATH="$WALLET_ROOT/private/credentials.enc" \
  OXID_CREDENTIAL_KEY_PATH="$WALLET_ROOT/private/credentials.key" \
  OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH="$OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH" \
  OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256" \
  OXID_MIDNIGHT_NETWORK_ID=undeployed \
  OXID_MIDNIGHT_INDEXER_WS_URL="$INDEXER_WS" \
  OXID_MIDNIGHT_INDEXER_HTTP_URL="$INDEXER_HTTP" \
  OXID_MIDNIGHT_NODE_WS_URL="$NODE_WS" \
  OXID_MIDNIGHT_PROOF_SERVER_URL="$PROOF_SERVER" \
  OXID_MIDNIGHT_UNSHIELDED_ADDRESS="$STANDALONE_ADDRESS" \
  env -u OXID_MIDNIGHT_PROVING_CACHE_DIR \
      -u OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH \
      -u OXID_MIDNIGHT_DUST_CHECKPOINT_PATH \
      -u OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH \
      -u OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH \
      -u OXID_MIDNIGHT_DID_RESOLVER_URL \
      -u OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT \
      -u OXID_PASSPORT_VAULT_COMPOSER \
      -u OXID_PASSPORT_VAULT_STORE_PATH \
      -u OXID_PRESENTATION_ARTIFACTS_DIR \
      "$REPOSITORY_ROOT/target/debug/oxid-app" >"$log" 2>&1 &
  app_pid=$!
}

capture_app_window() {
  local output="$1" bounds x y width height
  bounds="$(/usr/bin/osascript - "$app_pid" <<'APPLESCRIPT'
on run argv
  set targetPid to (item 1 of argv) as integer
  tell application "System Events"
    tell first process whose unix id is targetPid
      set frontmost to true
      set windowPosition to position of window 1
      set windowSize to size of window 1
      return ((item 1 of windowPosition) as text) & "," & ((item 2 of windowPosition) as text) & "," & ((item 1 of windowSize) as text) & "," & ((item 2 of windowSize) as text)
    end tell
  end tell
end run
APPLESCRIPT
)" || return 1
  IFS=, read -r x y width height <<EOF_BOUNDS
$bounds
EOF_BOUNDS
  [[ "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ && "$width" =~ ^[0-9]+$ && "$height" =~ ^[0-9]+$ ]] || return 1
  [ "$width" -ge 320 ] && [ "$height" -ge 480 ] || return 1
  /usr/sbin/screencapture -x -R"$x,$y,$width,$height" "$output"
  [ -s "$output" ]
}

for command_name in cargo curl docker file git jq node osascript screencapture shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
[ "$(uname -s)-$(uname -m)" = Darwin-arm64 ] || fail arm64-darwin-required
docker info >/dev/null 2>&1 || fail docker
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail occupied-project

umask 077
rm -rf -- "$RUNTIME"
mkdir -p -- "$CONTROL_ROOT" "$WALLET_ROOT/private" "$EVIDENCE_ROOT/screenshots"
chmod 700 "$RUNTIME" "$HOME_ROOT" "$APP_SUPPORT_ROOT" "$CONTROL_ROOT" "$WALLET_ROOT" "$WALLET_ROOT/private"
rm -f -- "$EVIDENCE_ROOT/evidence.json" "$EVIDENCE_ROOT/screenshots/consent.png" "$EVIDENCE_ROOT/screenshots/restart.png"

node "$REPOSITORY_ROOT/scripts/e2e/portal-virtual-mobile-offer-harness.mjs" --contract-test >/dev/null
cargo test --manifest-path "$REPOSITORY_ROOT/Cargo.toml" \
  -p oxid-adapter-identity-ingress --features desktop-test-qr-scanner \
  desktop_test_scanner >/dev/null
cargo test --manifest-path "$REPOSITORY_ROOT/Cargo.toml" \
  -p oxid-ui-dioxus identity_scan_admission_rejects_busy_pending_and_late_results >/dev/null
cargo build --manifest-path "$REPOSITORY_ROOT/Cargo.toml" \
  -p oxid-app --no-default-features --features desktop-portal-test >/dev/null
file_output="$(file "$REPOSITORY_ROOT/target/debug/oxid-app")"
case "$file_output" in *"Mach-O 64-bit arm64"*) ;; *) fail app-not-arm64-mach-o ;; esac

"$REPOSITORY_ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" >"$RUNTIME/stack.log" 2>&1 &
stack_pid=$!
wait_for_file "$STACK_BUILD_ENV" || fail stack-readiness
kill -0 "$stack_pid" 2>/dev/null || fail stack-exited
# Public authenticated deployment facts only; the one-shot offer and capability
# are never sourced into this process environment.
# shellcheck source=/dev/null
source "$STACK_BUILD_ENV"
[[ "$OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH" = /* ]] || fail manifest-path
[[ "$OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail manifest-digest

launch_app "$RUNTIME/app-first.log"
if ! wait_for_file "$CONTROL_ROOT/driver-started" "$CONTROL_ROOT/driver-failed" 60; then
  if kill -0 "$app_pid" 2>/dev/null; then
    fail driver-not-started-app-running
  elif grep -q 'desktop Portal test configuration is invalid' "$RUNTIME/app-first.log"; then
    fail desktop-profile-startup
  else
    fail first-app-exited-before-driver
  fi
fi
wait_for_file "$CONTROL_ROOT/sync-and-holder-visible" "$CONTROL_ROOT/driver-failed" \
  || fail first-rendered-setup
kill -0 "$app_pid" 2>/dev/null || fail first-app-exited
[ -f "$WALLET_ROOT/private/did-records.json" ] || fail holder-store
control_curl -X POST --data-binary @"$WALLET_ROOT/private/did-records.json" \
  "$CONTROL_ORIGIN/holder" >/dev/null || fail holder-sync
mkdir -p -- "$APP_SUPPORT_ROOT"
chmod 700 "$APP_SUPPORT_ROOT"
[ -f "$STACK_CAPABILITY" ] && [ ! -L "$STACK_CAPABILITY" ] || fail capability-source
mv -- "$STACK_CAPABILITY" "$APP_SUPPORT_ROOT/portal-offer.capability"
chmod 600 "$APP_SUPPORT_ROOT/portal-offer.capability"
printf 'ok\n' >"$CONTROL_ROOT/holder-ready"
chmod 600 "$CONTROL_ROOT/holder-ready"

wait_for_file "$CONTROL_ROOT/consent-visible" "$CONTROL_ROOT/driver-failed" \
  || fail consent-not-visible
control_curl "$CONTROL_ORIGIN/counters" >"$RUNTIME/counters-before-consent.json" \
  || fail pre-consent-counters
jq -e '.token == 0 and .nonce == 0 and .credential == 0 and .kyc >= 2' \
  "$RUNTIME/counters-before-consent.json" >/dev/null || fail issuer-called-before-consent
capture_app_window "$EVIDENCE_ROOT/screenshots/consent.png" || fail consent-screenshot
printf 'ok\n' >"$CONTROL_ROOT/consent-approved"
chmod 600 "$CONTROL_ROOT/consent-approved"

wait_for_file "$CONTROL_ROOT/first-complete" "$CONTROL_ROOT/driver-failed" \
  || fail issuance-or-reverify
control_curl "$CONTROL_ORIGIN/counters" >"$RUNTIME/counters-after-consent.json" \
  || fail post-consent-counters
jq -e '.token == 1 and .nonce == 1 and .credential == 1 and .issuerResolutionSuccess >= 1' \
  "$RUNTIME/counters-after-consent.json" >/dev/null || fail issuer-call-counts
control_curl "$CONTROL_ORIGIN/handoff-status" >"$RUNTIME/handoff-status.json" \
  || fail handoff-status
jq -e '.state == "consumed"' "$RUNTIME/handoff-status.json" >/dev/null || fail handoff-not-consumed
[ ! -e "$APP_SUPPORT_ROOT/portal-offer.capability" ] || fail capability-not-burned
[ -s "$WALLET_ROOT/private/credentials.enc" ] || fail encrypted-store
if grep -aEqi 'Alice|Example|John|Doe|AB1234567|pre-authorized|access[_-]?token|c_nonce|openid-credential-offer' \
  "$WALLET_ROOT/private/credentials.enc"; then
  fail plaintext-at-rest
fi
kill "$app_pid" >/dev/null 2>&1 || fail first-app-stop
wait "$app_pid" || fail first-app-status
app_pid=""
[ ! -s "$RUNTIME/app-first.log" ] || fail first-app-log

printf 'ok\n' >"$CONTROL_ROOT/restart"
chmod 600 "$CONTROL_ROOT/restart"
launch_app "$RUNTIME/app-restart.log"
wait_for_file "$CONTROL_ROOT/restart-complete" "$CONTROL_ROOT/driver-failed" \
  || fail restart-reverify
capture_app_window "$EVIDENCE_ROOT/screenshots/restart.png" || fail restart-screenshot
kill "$app_pid" >/dev/null 2>&1 || fail restart-app-stop
wait "$app_pid" || fail restart-app-status
app_pid=""
[ ! -s "$RUNTIME/app-restart.log" ] || fail restart-app-log

readonly OXID_HEAD="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)"
readonly OXID_TREE="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD^{tree})"
file "$EVIDENCE_ROOT/screenshots/consent.png" | grep -q 'PNG image data' \
  || fail consent-png
file "$EVIDENCE_ROOT/screenshots/restart.png" | grep -q 'PNG image data' \
  || fail restart-png
jq -cn \
  --arg head "$OXID_HEAD" --arg tree "$OXID_TREE" \
  --arg portal_commit "$PORTAL_COMMIT" --arg portal_tree "$PORTAL_TREE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" '
  {
    schema:"oxid-phase2-arm64-darwin-dioxus-v1",
    oxid:{head:$head,tree:$tree},
    portal:{integrationCommit:$portal_commit,integrationTree:$portal_tree,provenanceSha256:$provenance},
    activationRoute:"dioxus-document-rendered-click",
    binary:"native-arm64-macho-oxid-app",
    issuerImplementation:"lace-id-portal-rust",
    diditProviderMode:"lace-smocker",
    midnightInteractionProven:"oxid-app-indexer-sync",
    nodeInteractionProven:false,
    proofServerInteractionProven:false,
    screenshots:["consent.png","restart.png"],
    acceptance:{
      exactQrOfferCrossedPort:true,
      oneShotHandoffBurned:true,
      malformedUntrustedReplayConcurrentFailClosed:true,
      pendingRequestReplacementBlocked:true,
      visibleOfferAndConsent:true,
      issuerCallsBlockedBeforeConsent:true,
      explicitConsent:true,
      digitalPassportVerified:true,
      encryptedPersistence:true,
      listing:true,
      freshReverification:true,
      restartRestoration:true,
      appObservedMidnightSync:true,
      issuerDidBootstrappedAndResolved:true,
      laceDiditMockExercised:true,
      noExternalProviderCall:true,
      releaseExcluded:true,
      hostedTargetExcluded:true
    }
  }' >"$EVIDENCE_ROOT/evidence.json"
chmod 600 "$EVIDENCE_ROOT/evidence.json" "$EVIDENCE_ROOT/screenshots/consent.png" "$EVIDENCE_ROOT/screenshots/restart.png"
if grep -Eqi \
  'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|Alice|Example|John|Doe|AB1234567|capability|seed|"(route|claim|grant|token|nonce|credential|proof|private|log|pid|timestamp|path)"[[:space:]]*:' \
  "$EVIDENCE_ROOT/evidence.json"; then
  fail evidence-denylist
fi
jq -e --arg head "$OXID_HEAD" --arg tree "$OXID_TREE" '
  .oxid == {head:$head,tree:$tree}
  and .activationRoute == "dioxus-document-rendered-click"
  and .nodeInteractionProven == false
  and .proofServerInteractionProven == false
  and (.acceptance | to_entries | all(.value == true))' \
  "$EVIDENCE_ROOT/evidence.json" >/dev/null || fail evidence-schema

printf 'portal-desktop-e2e: PASS evidence=target/portal-desktop-e2e/evidence.json screenshots=target/portal-desktop-e2e/screenshots/{consent,restart}.png\n'
