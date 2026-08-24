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
node --check scripts/e2e/portal-mobile-offer-handoff.mjs
node --check scripts/e2e/portal-mobile-offer-handoff.test.mjs
node --test scripts/e2e/portal-mobile-offer-handoff.test.mjs
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
  /^portal-mobile-smoke([[:space:]].*)?:/ { capture=1; next }
  capture && /^[^[:space:]#]/ { exit }
  capture { print }
' Justfile; } || true)"
ios_recipe_line="$(grep -n 'test-ios-portal-flow.sh' <<<"$portal_recipe" || true)"
android_recipe_line="$(grep -n 'test-android-portal-flow.sh' <<<"$portal_recipe" || true)"
ios_line="$(cut -d: -f1 <<<"$ios_recipe_line")"
android_line="$(cut -d: -f1 <<<"$android_recipe_line")"
ios_command="$(cut -d: -f2- <<<"$ios_recipe_line" | awk '{$1=$1; print}')"
android_command="$(cut -d: -f2- <<<"$android_recipe_line" | awk '{$1=$1; print}')"
if [[ ! "$ios_line" =~ ^[0-9]+$ || ! "$android_line" =~ ^[0-9]+$ ]] ||
  [ "$ios_line" -ge "$android_line" ] ||
  [ "$ios_command" != 'STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-ios-portal-flow.sh' ] ||
  [ "$android_command" != 'STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-android-portal-flow.sh' ]; then
  echo "portal-mobile-smoke must run exact iOS then Android commands without shell operators or backgrounding." >&2
  exit 1
fi

for marker in \
  OXID_STANDALONE_PORTAL_PROFILE \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256 \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_PATH \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_SHA256; do
  rg -q "$marker" apps/oxid scripts || {
    echo "Portal compile-time profile marker is missing: $marker" >&2
    exit 1
  }
done

for capability_boundary in \
  'randomFillSync(Buffer.alloc(32))' \
  'timingSafeEqual(candidate, this.#capability)' \
  'this.#state = "consuming"' \
  'this.#capability.fill(0)' \
  'offer.fill(0)' \
  'Authorization: Bearer ' \
  'portal-offer.capability' \
  'preparePrivateCapabilityPaths' \
  'iosCapabilityCandidatePath' \
  'files/.portal-offer.capability.tmp' \
  'fs::remove_file(path)' \
  'zeroize::Zeroizing' \
  'arm-android-offer' \
  'head -c "$PORTAL_MOBILE_OFFER_CAPABILITY_BYTES" <&8' \
  'run-as io.medianox.oxid sh -c'; do
  rg -qF "$capability_boundary" \
    scripts/e2e/portal-mobile-offer-handoff.mjs \
    scripts/e2e/portal-mobile-support.mjs \
    scripts/e2e/portal-mobile-harness-lib.sh \
    scripts/test-android-portal-flow.sh \
    crates/adapters/identity-ingress/src/lib.rs || {
    echo "Portal one-shot capability boundary is missing: $capability_boundary" >&2
    exit 1
  }
done
if rg -n 'process\.argv.*(capability|offer)|ready(Path|\[[^]]+\]|\.[A-Za-z]).*capability|credentialOfferUri.*(console|process\.(stdout|stderr))|Authorization.*MOBILE_TEST_OFFER_TRIGGER|OXID_BUILD_.*CAPABILITY|env!\([^)]*CAPABILITY|capability=.*(simctl|adb|spawn)' \
  scripts/e2e/portal-mobile-* scripts/run-ios-simulator.sh scripts/run-android-emulator.sh \
  scripts/test-ios-portal-flow.sh scripts/test-android-portal-flow.sh \
  crates/adapters/identity-ingress/src/lib.rs; then
  echo "Portal capability must not enter argv, logs, ready/evidence data, or build inputs." >&2
  exit 1
fi

for port in 6300 8088 18091 18093 9944 18090; do
  rg -q "${port}" scripts/test-android-portal-flow.sh || {
    echo "Android Portal harness is missing exact reverse port $port." >&2
    exit 1
  }
done

# Portal verification has no future-time slack. The disposable QEMU clock must
# be set from the host through Android's privileged alarm service at startup and
# again immediately before the positive issuance path. The second sync repairs
# any drift accumulated during the preparation-only negative scenarios.
clock_sync_calls="$(rg -c '^synchronize_android_clock$' scripts/test-android-portal-flow.sh || true)"
clock_lead_calls="$(rg -cF 'sync_epoch + 2' scripts/test-android-portal-flow.sh || true)"
clock_set_gate_calls="$(rg -cF 'run_adb_gate_operation clock-set-time' scripts/test-android-portal-flow.sh || true)"
clock_read_gate_calls="$(rg -cF 'run_adb_gate_operation clock-read-time' scripts/test-android-portal-flow.sh || true)"
if ! rg -qF 'shell cmd alarm set-time' scripts/test-android-portal-flow.sh ||
  [ "$clock_sync_calls" != 2 ] ||
  [ "$clock_lead_calls" != 1 ] ||
  [ "$clock_set_gate_calls" != 1 ] ||
  [ "$clock_read_gate_calls" != 1 ] ||
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
initial_activation_ready='Boolean(${button("Activate development wallet")} || ${button("Use my receive address")})'
managed_did_activation_failure='managed DID creation ran without activated development custody'
strict_offer_boundary_wait='strict credential-offer boundary'
if ! rg -qF "$initial_activation_ready" tests/mobile/android-portal-flow.mjs ||
  ! rg -qF "$managed_did_activation_failure" tests/mobile/android-portal-flow.mjs ||
  ! rg -qF "$strict_offer_boundary_wait" tests/mobile/android-portal-flow.mjs ||
  [ "$(rg -cF "$activation_complete_wait" tests/mobile/android-portal-flow.mjs)" -ne 2 ]; then
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
cleanup_trap_marker='trap '\''portal_mobile_exit "$?"'\'' EXIT'
if [[ "$first_statement" != *"$cleanup_trap_marker"* ]]; then
  echo "portal_mobile_start must install its fail-closed EXIT cleanup owner before its first side effect." >&2
  exit 1
fi
for signal_trap in "trap 'exit 130' INT" "trap 'exit 143' TERM"; do
  if [ "$(grep -cF "$signal_trap" scripts/e2e/portal-mobile-harness-lib.sh)" -ne 1 ]; then
    echo "Portal mobile signal handler is missing or duplicated: $signal_trap" >&2
    exit 1
  fi
done
if [ "$(grep -cF "$cleanup_trap_marker" scripts/e2e/portal-mobile-harness-lib.sh)" -ne 1 ]; then
  echo "portal_mobile_exit must have exactly one early EXIT trap." >&2
  exit 1
fi

# The EXIT owner must preserve a clean zero status, turn a final cleanup
# failure into command failure, and never replace an earlier nonzero status.
# Exercise the real cleanup function and hook rather than relying on trap text.
clean_exit_status=0
bash -c '
  source "$1"
  portal_mobile_platform_cleanup() { return 0; }
  trap '\''portal_mobile_exit "$?"'\'' EXIT
  exit 0
' _ scripts/e2e/portal-mobile-harness-lib.sh || clean_exit_status=$?
if [ "$clean_exit_status" -ne 0 ]; then
  echo "Portal EXIT cleanup must preserve a clean zero status." >&2
  exit 1
fi
cleanup_exit_status=0
bash -c '
  source "$1"
  portal_mobile_platform_cleanup() { return 1; }
  trap '\''portal_mobile_exit "$?"'\'' EXIT
  exit 0
' _ scripts/e2e/portal-mobile-harness-lib.sh || cleanup_exit_status=$?
if [ "$cleanup_exit_status" -ne 1 ]; then
  echo "Portal EXIT cleanup must fail an otherwise successful command." >&2
  exit 1
fi
original_exit_status=0
bash -c '
  source "$1"
  portal_mobile_platform_cleanup() { return 1; }
  trap '\''portal_mobile_exit "$?"'\'' EXIT
  exit 42
' _ scripts/e2e/portal-mobile-harness-lib.sh || original_exit_status=$?
if [ "$original_exit_status" -ne 42 ]; then
  echo "Portal EXIT cleanup must preserve the original nonzero status." >&2
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
if ! bash -c '
  set -euo pipefail
  source "$1"
  [ "$PORTAL_MOBILE_STARTUP_GRACE_SECONDS" -eq $((10 * 60 + 60 + 5 + 10)) ]
' _ scripts/e2e/portal-mobile-harness-lib.sh; then
  echo "Pre-READY KILL grace must cover the ten-minute command, 60-second teardown, five-second poll, and scheduling margin." >&2
  exit 1
fi
# Scale the pre-READY KILL grace down to two seconds. An observer must see TERM
# during the first second while the TERM-handling child remains alive until the
# full bound; this fails if TERM is delayed until the KILL deadline.
bash -c '
  set -euo pipefail
  source "$1"
  scratch="$(mktemp -d)"
  child_pid=""
  trap '\''[ -z "$child_pid" ] || kill -KILL "$child_pid" >/dev/null 2>&1 || true; rm -rf "$scratch"'\'' EXIT
  term_marker="$scratch/term"
  ready_marker="$scratch/ready"
  observed_marker="$scratch/observed"
  (
    trap '\''printf TERM >"$term_marker"'\'' TERM
    : >"$ready_marker"
    while :; do sleep 0.1; done
  ) &
  child_pid=$!
  for _attempt in $(seq 1 100); do
    [ ! -e "$ready_marker" ] || break
    sleep 0.01
  done
  [ -e "$ready_marker" ]
  (
    sleep 1
    [ -s "$term_marker" ]
    kill -0 "$child_pid"
    : >"$observed_marker"
  ) &
  observer_pid=$!
  started=$SECONDS
  wait_status=0
  portal_mobile_terminate_bounded "$child_pid" 2 || wait_status=$?
  child_pid=""
  wait "$observer_pid"
  elapsed=$((SECONDS - started))
  [ -e "$observed_marker" ]
  [ "$wait_status" -eq 137 ]
  [ "$elapsed" -ge 2 ]
  [ "$elapsed" -le 6 ]
' _ scripts/e2e/portal-mobile-harness-lib.sh 2>/dev/null
for immediate_term_marker in \
  'kill -TERM "$child_pid"' \
  'portal_mobile_terminate_bounded \' \
  '"$PORTAL_MOBILE_SUPPORT_PID" "$PORTAL_MOBILE_STARTUP_GRACE_SECONDS"'; do
  rg -qF "$immediate_term_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Pre-READY support shutdown is missing immediate-TERM/full-bound marker: $immediate_term_marker" >&2
    exit 1
  }
done
if ! rg -qF 'rm -rf "$PORTAL_MOBILE_STATE_DIR"' scripts/e2e/portal-mobile-harness-lib.sh ||
  rg -qF 'private failure artifacts=' scripts/e2e/portal-mobile-harness-lib.sh; then
  echo "Portal private runtime must be removed on every exit." >&2
  exit 1
fi
# Generic launcher and xcodebuild output can contain selected device ids. Keep
# those streams and every Xcode result artifact in the cleanup-owned runtime.
if ! rg -Uq 'run-ios-simulator\.sh" \\\n[[:space:]]+>>"\$PORTAL_MOBILE_PRIVATE_LOG" 2>&1' scripts/test-ios-portal-flow.sh ||
  ! rg -Uq 'run-android-emulator\.sh" \\\n[[:space:]]+>>"\$PORTAL_MOBILE_PRIVATE_LOG" 2>&1' scripts/test-android-portal-flow.sh ||
  ! rg -Uq 'CODE_SIGNING_ALLOWED=NO \\\n[[:space:]]+>>"\$PORTAL_MOBILE_PRIVATE_LOG" 2>&1' scripts/test-ios-portal-flow.sh ||
  ! rg -qF -- '-derivedDataPath "$PORTAL_MOBILE_STATE_DIR/ios-derived-data"' scripts/test-ios-portal-flow.sh ||
  ! rg -qF -- '-resultBundlePath "$PORTAL_MOBILE_STATE_DIR/ios-results.xcresult"' scripts/test-ios-portal-flow.sh ||
  rg -qF 'target/mobile-tests/ios-portal-derived-data' scripts/test-ios-portal-flow.sh; then
  echo "Portal launcher/Xcode logs must remain private and cleanup-owned." >&2
  exit 1
fi
# Every adb operation in cold reboot/readiness uses the shared TERM/KILL
# helper, writes only to cleanup-owned private files, and consumes one real
# elapsed-time boot deadline. No attempt count may multiply transport hangs.
for adb_gate_marker in \
  'run_adb_gate_operation devices ' \
  'run_adb_gate_operation qemu-before-reboot ' \
  'run_adb_gate_operation reboot ' \
  'run_adb_gate_operation wait-for-device ' \
  'run_adb_gate_operation boot-completed ' \
  'run_adb_gate_operation devices-after-launch ' \
  'run_adb_gate_operation qemu-after-launch ' \
  'run_adb_gate_operation clock-set-time ' \
  'run_adb_gate_operation clock-read-time ' \
  'adb_boot_deadline=$((SECONDS + PORTAL_MOBILE_ADB_BOOT_DEADLINE_SECONDS))' \
  'while [ "$SECONDS" -lt "$adb_boot_deadline" ]' \
  'portal_mobile_fail emulator-reconnect'; do
  rg -qF "$adb_gate_marker" scripts/test-android-portal-flow.sh || {
    echo "Android bounded reboot/readiness marker is missing: $adb_gate_marker" >&2
    exit 1
  }
done
for adb_bound_marker in \
  'portal_mobile_run_captured_bounded' \
  '"$(dirname -- "$output_path")" = "$PORTAL_MOBILE_STATE_DIR"' \
  'term_after_seconds=$((total_timeout_seconds - term_grace_seconds))' \
  '"$child_pid" "$term_after_seconds" "$term_grace_seconds"'; do
  rg -qF "$adb_bound_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Android adb private-output TERM/KILL bound is missing: $adb_bound_marker" >&2
    exit 1
  }
done
if rg -n 'for _attempt in \$\(seq 1 120\)|\$adb_command devices|\$adb_command.*getprop (ro\.kernel\.qemu|sys\.boot_completed)|\$adb_command.*shell (cmd alarm set-time|date -u \+%s)' \
  scripts/test-android-portal-flow.sh; then
  echo "Android reboot/readiness still contains an attempt-multiplied or direct unbounded adb query." >&2
  exit 1
fi
# Exercise the shared captured-command helper with a TERM-responsive hung
# process. Its partial output remains owner-private and its nonzero status is
# propagated rather than stranding EXIT cleanup.
bash -c '
  set -euo pipefail
  source "$1"
  scratch="$(mktemp -d)"
  trap '\''rm -rf "$scratch"'\'' EXIT
  PORTAL_MOBILE_PLATFORM=android
  PORTAL_MOBILE_STATE_DIR="$scratch"
  PORTAL_MOBILE_PRIVATE_LOG="$scratch/private.log"
  : >"$PORTAL_MOBILE_PRIVATE_LOG"
  chmod 600 "$PORTAL_MOBILE_PRIVATE_LOG"
  output="$scratch/adb-hung.out"
  status=0
  started=$SECONDS
  portal_mobile_run_captured_bounded "$output" 2 1 \
    sh -c '\''printf ready; trap "exit 42" TERM; while :; do :; done'\'' || status=$?
  elapsed=$((SECONDS - started))
  [ "$status" -eq 42 ]
  [ "$elapsed" -ge 1 ]
  [ "$elapsed" -le 2 ]
  [ "$(tr -d '\''\r\n'\'' <"$output")" = ready ]
  [ "$(LC_ALL=C ls -ld "$output" | cut -c2-10)" = rw------- ]
' _ scripts/e2e/portal-mobile-harness-lib.sh

# Mobile support attaches to the same authenticated shared profile. It must not
# own Portal service start/stop or create another detached protocol worktree.
for shared_stack_marker in \
  'STACK_ENV_FILE="$STACK_ENV_PATH"' \
  'stack_env_load "$STACK_ENV_FILE"' \
  'stack_env_delegate_portal status' \
  'COMPOSE_PROJECT_NAME="$PORTAL_COMPOSE_PROJECT"' \
  'PORTAL_INTEGRATION_CHECKOUT="$source_tree"' \
  'runLogged(localHeadlessScript, ["status", stackEnvFile], repositoryRoot)' \
  '`label=com.docker.compose.project=${composeProjectName}`'; do
  rg -qF "$shared_stack_marker" \
    scripts/e2e/portal-mobile-harness-lib.sh scripts/e2e/portal-mobile-support.mjs || {
    echo "Portal support shared-stack attachment marker regressed: $shared_stack_marker" >&2
    exit 1
  }
done
if rg -n 'compose-(up|down)|\["compose",|docker[[:space:]]+compose|worktree add' \
  scripts/e2e/portal-mobile-harness-lib.sh scripts/e2e/portal-mobile-support.mjs; then
  echo "Portal mobile support must delegate lifecycle and must not own Portal Compose or protocol worktrees." >&2
  exit 1
fi
if [ "$(rg -c 'timeout: (timeoutMs|HOST_COMMAND_TIMEOUT_MS)' scripts/e2e/portal-mobile-support.mjs)" -ne 4 ] ||
  [ "$(rg -c 'killSignal: "SIGKILL"' scripts/e2e/portal-mobile-support.mjs)" -ne 4 ] ||
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

# Hosted scanners must keep the intentional loopback readiness probe and the
# public negative JWT fixture without weakening repository-wide rules. The
# OpenGrep false positive is removed by using Node's explicit loopback HTTP API;
# Checkov removes only the digest-authenticated synthetic fixture from its scan checkout.
negative_fixture="fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/openid4vci-final/negative/unsupported-proof-alg.json"
negative_fixture_sha256="82c8944a6fa7bad89632c324bba46411bc35556bf7172a8971ec6d6cda2fbe3f"
if ! rg -qF 'function issuerMetadataReady()' scripts/e2e/portal-mobile-support.mjs ||
  ! rg -qF 'const request = http.request({' scripts/e2e/portal-mobile-support.mjs ||
  rg -qF 'nosemgrep:' scripts/e2e/portal-mobile-support.mjs ||
  [ "$(shasum -a 256 "$negative_fixture" | awk '{print $1}')" != "$negative_fixture_sha256" ] ||
  ! rg -qF "fixture=$negative_fixture" .github/workflows/scan.yml ||
  ! rg -qF "$negative_fixture_sha256" .github/workflows/scan.yml ||
  ! rg -qF 'sha256sum --check --strict' .github/workflows/scan.yml ||
  ! rg -qF 'rm -- "$fixture"' .github/workflows/scan.yml; then
  echo "Portal scanner exceptions must remain digest-authenticated exact-path/explicit-loopback only." >&2
  exit 1
fi

if [ "$(rg -lF '.token == 2 and .nonce == 1 and .credential == 1' scripts/test-ios-portal-flow.sh scripts/test-android-portal-flow.sh | wc -l | tr -d ' ')" -ne 2 ]; then
  echo "Both Portal platform suites must account for one failed and one successful token attempt." >&2
  exit 1
fi

# The owner-private profile authenticates a persistent detached protocol source
# and the separate signed lifecycle helper. Mobile support never fetches or
# creates another checkout during a conformance run.
for source_pin_marker in \
  'PORTAL_HELPER_COMMIT="8915760a4523d282fa07d45a48b7f58e4287bb54"' \
  'PORTAL_HELPER_TREE="1317e109cf0792c0e1d7c8f9e2b8857251f6e92d"' \
  'stack_env_load "$STACK_ENV_FILE"' \
  'source_tree="$(portal_mobile_source_tree)"' \
  '[ "$source_tree" = "$PORTAL_PROTOCOL_SOURCE_DIR" ]'; do
  rg -qF "$source_pin_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Immutable Portal profile provenance marker is missing: $source_pin_marker" >&2
    exit 1
  }
done
if rg -n 'fetch origin|worktree add|origin/integration\^\{' scripts/e2e/portal-mobile-harness-lib.sh; then
  echo "Portal reproduction must use only the authenticated persistent detached source." >&2
  exit 1
fi

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

# Both platform scripts must write only to a private sibling candidate and use
# the shared exact-jq/sentinel/rename finalizer. Direct evidence redirection
# would truncate an earlier valid attestation before validation completes.
for platform_script in scripts/test-ios-portal-flow.sh scripts/test-android-portal-flow.sh; do
  for evidence_marker in \
    'mktemp "$evidence_directory/.evidence.json.tmp.XXXXXX"' \
    'PORTAL_MOBILE_EVIDENCE_TEMP="$evidence_temp"' \
    'chmod 600 "$evidence_temp"' \
    '"$evidence_document" >"$evidence_temp"' \
    'portal_mobile_finalize_evidence' \
    '"$evidence" "$evidence_temp" "$evidence_document" "$evidence_sentinel"'; do
    rg -qF "$evidence_marker" "$platform_script" || {
      echo "Portal evidence atomic-publication marker is missing from $platform_script: $evidence_marker" >&2
      exit 1
    }
  done
  if rg -qF '>"$evidence"' "$platform_script"; then
    echo "Portal evidence must never be generated directly into evidence.json: $platform_script" >&2
    exit 1
  fi
  candidate_line="$(grep -nF 'mktemp "$evidence_directory/.evidence.json.tmp.XXXXXX"' "$platform_script" | cut -d: -f1)"
  generation_line="$(grep -nF '"$evidence_document" >"$evidence_temp"' "$platform_script" | cut -d: -f1)"
  finalization_line="$(grep -nF 'portal_mobile_finalize_evidence \' "$platform_script" | cut -d: -f1)"
  if [ "$candidate_line" -ge "$generation_line" ] || [ "$generation_line" -ge "$finalization_line" ]; then
    echo "Portal evidence candidate creation, generation, and finalization order regressed: $platform_script" >&2
    exit 1
  fi
done
for finalizer_marker in \
  '. == ($expected_document)' \
  'rg -qi "$sentinel" "$candidate"' \
  'mv -f -- "$candidate" "$evidence"'; do
  rg -qF "$finalizer_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Portal evidence finalizer is missing exact validation/publication marker: $finalizer_marker" >&2
    exit 1
  }
done

# Behavioral proof: jq mismatch, sentinel rejection, and publication failure
# delete only the private candidate and preserve byte-identical old evidence;
# a fully valid candidate replaces it.
bash -c '
  set -euo pipefail
  source "$1"
  scratch="$(mktemp -d)"
  trap '\''rm -rf "$scratch"'\'' EXIT
  evidence="$scratch/evidence.json"
  printf '\''%s\n'\'' '\''{"schema":"old"}'\'' >"$evidence"

  candidate="$scratch/.evidence.json.tmp.invalid"
  printf '\''%s\n'\'' '\''{"schema":"wrong"}'\'' >"$candidate"
  PORTAL_MOBILE_EVIDENCE_TEMP="$candidate"
  if portal_mobile_finalize_evidence "$evidence" "$candidate" '\''{"schema":"new"}'\'' forbidden >/dev/null 2>&1; then
    exit 1
  fi
  grep -qF '\''{"schema":"old"}'\'' "$evidence"
  [ ! -e "$candidate" ]

  candidate="$scratch/.evidence.json.tmp.sentinel"
  printf '\''%s\n'\'' '\''{"schema":"new","note":"forbidden"}'\'' >"$candidate"
  PORTAL_MOBILE_EVIDENCE_TEMP="$candidate"
  if portal_mobile_finalize_evidence "$evidence" "$candidate" '\''{"schema":"new","note":"forbidden"}'\'' forbidden >/dev/null 2>&1; then
    exit 1
  fi
  grep -qF '\''{"schema":"old"}'\'' "$evidence"
  [ ! -e "$candidate" ]

  old_evidence="$scratch/old-evidence.json"
  cp "$evidence" "$old_evidence"
  candidate="$scratch/.evidence.json.tmp.publish-failure"
  printf '\''%s\n'\'' '\''{"schema":"new"}'\'' >"$candidate"
  PORTAL_MOBILE_EVIDENCE_TEMP="$candidate"
  mv() { return 1; }
  publish_status=0
  portal_mobile_finalize_evidence "$evidence" "$candidate" '\''{"schema":"new"}'\'' forbidden \
    >/dev/null 2>&1 || publish_status=$?
  unset -f mv
  [ "$publish_status" -ne 0 ]
  cmp -s "$old_evidence" "$evidence"
  [ ! -e "$candidate" ]
  [ -z "$PORTAL_MOBILE_EVIDENCE_TEMP" ]

  candidate="$scratch/.evidence.json.tmp.valid"
  printf '\''%s\n'\'' '\''{"schema":"new"}'\'' >"$candidate"
  PORTAL_MOBILE_EVIDENCE_TEMP="$candidate"
  portal_mobile_finalize_evidence "$evidence" "$candidate" '\''{"schema":"new"}'\'' forbidden
  jq -e '\''. == {"schema":"new"}'\'' "$evidence" >/dev/null
  [ ! -e "$candidate" ]
  [ -z "$PORTAL_MOBILE_EVIDENCE_TEMP" ]
' _ scripts/e2e/portal-mobile-harness-lib.sh

# CDP uses a dynamically allocated, exactly owned forward. Both opening the
# socket and every command are bounded, and terminal WebSocket events reject
# all pending commands so top-level await cannot remain unsettled.
for cdp_marker in \
  '.url == "https://dioxus.index.html/"' \
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
for timeout_busy_marker in \
  'accessible disabled offer-check busy state' \
  'application.buttons["Checking offer…"]' \
  'The in-progress offer check must be disabled'; do
  rg -qF "$timeout_busy_marker" tests/mobile/android-portal-flow.mjs tests/mobile/ios/OxidUITests/PortalFlowTests.swift || {
    echo "Portal timeout busy-state assertion is missing: $timeout_busy_marker" >&2
    exit 1
  }
done

# Post-consent cleanup uncertainty is fail closed: consent is cleared, while
# the payload-free prepared review and its route lock survive until a real
# process boundary. The positive issuance must run first so that boundary is
# explicit rather than accidentally relying on a second issuance in one process.
locked_review_notice='This protocol is unavailable in the current build. Session cleanup is unavailable; this review remains locked until refusal succeeds or the app restarts.'
for locked_review_source in \
  tests/mobile/android-portal-flow.mjs \
  tests/mobile/ios/OxidUITests/PortalFlowTests.swift; do
  rg -qF "$locked_review_notice" "$locked_review_source" || {
    echo "Portal cleanup-uncertainty notice is missing from $locked_review_source." >&2
    exit 1
  }
done
for locked_review_marker in \
  'consentCleared: Boolean(consent) && !consent.checked' \
  'failed issuance retained prepared review and route lock' \
  'XCTAssertEqual(clearedConsent.value as? String, "0")' \
  'A failed issuance must retain the prepared review and route lock'; do
  rg -qF "$locked_review_marker" tests/mobile/android-portal-flow.mjs tests/mobile/ios/OxidUITests/PortalFlowTests.swift || {
    echo "Portal retained-review assertion is missing: $locked_review_marker" >&2
    exit 1
  }
done
if [ "$(rg -cF 'credential_issuance_review_blocks_replacement(' crates/ui-dioxus/src/lib.rs)" -lt 4 ] ||
  ! rg -qF 'prepared.is_some_and(|review| review.state == "awaiting_consent")' crates/ui-dioxus/src/lib.rs ||
  ! rg -qF 'let Some(manual_review_reserved) =' crates/ui-dioxus/src/lib.rs ||
  ! rg -qF 'begin_credential_preview_single_flight_value(&mut busy)' crates/ui-dioxus/src/lib.rs; then
  echo "Credential Preview must be single-flight and replace only terminal prepared reviews." >&2
  exit 1
fi
if rg -n 'failed issuance route release|A failed issuance must clear the retained router request|post-consent transport failure must release' \
  tests/mobile/android-portal-flow.mjs tests/mobile/ios/OxidUITests/PortalFlowTests.swift; then
  echo "Portal mobile suites still demand obsolete post-consent route release." >&2
  exit 1
fi

for exact_counter_marker in \
  'XCTAssertEqual(try counters()["token"], 1)' \
  'XCTAssertEqual(try counters()["token"], 2)' \
  'counts.token !== 1 || counts.nonce !== 1 || counts.credential !== 1' \
  'counts.token !== 2 || counts.nonce !== 1 || counts.credential !== 1'; do
  rg -qF "$exact_counter_marker" tests/mobile/android-portal-flow.mjs tests/mobile/ios/OxidUITests/PortalFlowTests.swift || {
    echo "Portal reordered issuance counter assertion is missing: $exact_counter_marker" >&2
    exit 1
  }
done

ios_positive_line="$(grep -nF '"Credential issued, verified, and stored in the protected inventory."' tests/mobile/ios/OxidUITests/PortalFlowTests.swift | cut -d: -f1)"
ios_locked_line="$(grep -nF "$locked_review_notice" tests/mobile/ios/OxidUITests/PortalFlowTests.swift | cut -d: -f1)"
ios_cold_line="$(grep -nF 'try deliver("real-cold", in: application)' tests/mobile/ios/OxidUITests/PortalFlowTests.swift | cut -d: -f1)"
if [[ ! "$ios_positive_line" =~ ^[0-9]+$ || ! "$ios_locked_line" =~ ^[0-9]+$ || ! "$ios_cold_line" =~ ^[0-9]+$ ]] ||
  (( ios_positive_line >= ios_locked_line || ios_locked_line >= ios_cold_line )); then
  echo "iOS Portal flow must run positive issuance, locked failure, then real-cold delivery." >&2
  exit 1
fi

android_second_sync_line="$(grep -n '^synchronize_android_clock$' scripts/test-android-portal-flow.sh | tail -n 1 | cut -d: -f1)"
android_positive_line="$(grep -n '^run_webview_scenario issue$' scripts/test-android-portal-flow.sh | cut -d: -f1)"
android_locked_line="$(grep -n '^run_webview_scenario issue-error$' scripts/test-android-portal-flow.sh | cut -d: -f1)"
android_force_stop_line="$(grep -nF 'shell am force-stop io.medianox.oxid' scripts/test-android-portal-flow.sh | cut -d: -f1)"
android_cold_line="$(grep -n '^run_webview_scenario cold-route$' scripts/test-android-portal-flow.sh | cut -d: -f1)"
if [[ ! "$android_second_sync_line" =~ ^[0-9]+$ || ! "$android_positive_line" =~ ^[0-9]+$ ||
      ! "$android_locked_line" =~ ^[0-9]+$ || ! "$android_force_stop_line" =~ ^[0-9]+$ ||
      ! "$android_cold_line" =~ ^[0-9]+$ ]] ||
  (( android_second_sync_line >= android_positive_line ||
     android_positive_line >= android_locked_line ||
     android_locked_line >= android_force_stop_line ||
     android_force_stop_line >= android_cold_line )); then
  echo "Android Portal flow must sync, issue successfully, retain the failed review, then cross force-stop/cold-route." >&2
  exit 1
fi

for workflow in .github/workflows/ci.yml .github/workflows/quality.yml .github/workflows/scan.yml; do
  if [ "$(rg -c '^    branches: \[develop, integration, main\]$' "$workflow")" -ne 2 ]; then
    echo "Hosted push and pull-request checks must both include integration in $workflow." >&2
    exit 1
  fi
done
for lock_marker in \
  'mkdir "$PORTAL_MOBILE_LOCK_DIR"' \
  'PORTAL_MOBILE_RECLAIM_DIR="${PORTAL_MOBILE_LOCK_DIR}.reclaim"' \
  'mkdir "$PORTAL_MOBILE_RECLAIM_DIR"' \
  '[ "$revalidated_owner" != "$owner" ]' \
  'mv "$PORTAL_MOBILE_LOCK_DIR" "$stale_lock"' \
  'portal_mobile_release_reclaim_claim' \
  'owner-pid'; do
  rg -qF "$lock_marker" scripts/e2e/portal-mobile-harness-lib.sh || {
    echo "Atomic stale-safe Portal mobile lock marker is missing: $lock_marker" >&2
    exit 1
  }
done
# A live creator paused between mkdir and owner-pid publication must remain the
# owner of its directory. The contender fails busy and never renames the lock.
bash -c '
  set -euo pipefail
  source "$1"
  fake_uid="99$$"
  id() { printf '\''%s\n'\'' "$fake_uid"; }
  lock_dir="/tmp/oxid-portal-mobile-${fake_uid}.lock"
  trap '\''rm -rf "$lock_dir" "$lock_dir.reclaim"'\'' EXIT
  mkdir "$lock_dir"
  : >"$lock_dir/live-creator"
  (
    sleep 2
    printf '\''%s\n'\'' "$$" >"$lock_dir/owner-pid"
  ) &
  creator_pid=$!
  if portal_mobile_acquire_lock >/dev/null 2>&1; then
    exit 1
  fi
  [ -d "$lock_dir" ]
  [ -e "$lock_dir/live-creator" ]
  [ -z "$(compgen -G "$lock_dir.stale.*" || true)" ]
  wait "$creator_pid"
  [ -s "$lock_dir/owner-pid" ]
' _ scripts/e2e/portal-mobile-harness-lib.sh

# A contender that passed its first claim check before reclamation is paused at
# mkdir. The reclaimer then moves the validated stale lock while holding its
# claim. The contender may create a would-be replacement, but its post-mkdir
# claim check must remove only that replacement and fail busy; the reclaimer
# never renames/deletes it. Use a fake uid so this never touches the real lock.
bash -c '
  set -euo pipefail
  source "$1"
  fake_uid="98$$"
  id() { printf '\''%s\n'\'' "$fake_uid"; }
  lock_dir="/tmp/oxid-portal-mobile-${fake_uid}.lock"
  reclaim_dir="$lock_dir.reclaim"
  contender_ready="$lock_dir.contender-ready"
  release_contender="$lock_dir.release-contender"
  contender_failed="$lock_dir.contender-failed"
  moved_marker="$lock_dir.moved"
  release_reclaimer="$lock_dir.release-reclaimer"
  acquired_marker="$lock_dir.acquired"
  contender_pid=""
  reclaimer_pid=""
  trap '\''[ -z "$contender_pid" ] || kill -KILL "$contender_pid" >/dev/null 2>&1 || true; [ -z "$reclaimer_pid" ] || kill -KILL "$reclaimer_pid" >/dev/null 2>&1 || true; rm -rf "$lock_dir" "$reclaim_dir" "$lock_dir".*'\'' EXIT
  mkdir "$lock_dir"
  printf '\''%s\n'\'' 999999999 >"$lock_dir/owner-pid"
  ! kill -0 999999999 2>/dev/null

  (
    mkdir() {
      local target="${*: -1}"
      if [ "$target" = "$lock_dir" ] && [ ! -e "$contender_ready" ]; then
        : >"$contender_ready"
        while [ ! -e "$release_contender" ]; do sleep 0.01; done
      fi
      command mkdir "$@"
    }
    if portal_mobile_acquire_lock >/dev/null 2>&1; then
      exit 1
    fi
    : >"$contender_failed"
  ) &
  contender_pid=$!
  for _attempt in $(seq 1 500); do
    [ ! -e "$contender_ready" ] || break
    sleep 0.01
  done
  [ -e "$contender_ready" ]

  (
    mv() {
      command mv "$@"
      : >"$moved_marker"
      while [ ! -e "$release_reclaimer" ]; do sleep 0.01; done
    }
    portal_mobile_acquire_lock
    : >"$acquired_marker"
  ) &
  reclaimer_pid=$!
  for _attempt in $(seq 1 500); do
    [ ! -e "$moved_marker" ] || break
    sleep 0.01
  done
  [ -e "$moved_marker" ]
  [ -d "$reclaim_dir" ]
  [ ! -e "$lock_dir" ]

  : >"$release_contender"
  wait "$contender_pid"
  contender_pid=""
  [ -e "$contender_failed" ]
  [ ! -e "$lock_dir" ]
  [ -d "$reclaim_dir" ]

  : >"$release_reclaimer"
  wait "$reclaimer_pid"
  reclaimer_pid=""
  [ -e "$acquired_marker" ]
  [ -d "$lock_dir" ]
  rm -rf "$lock_dir"

  # Even a dead/ambiguous reclaim owner is fail-closed; only explicit operator
  # cleanup may recover the abandoned claim.
  mkdir "$reclaim_dir"
  printf '\''%s\n'\'' 999999999 >"$reclaim_dir/owner-pid"
  if portal_mobile_acquire_lock >/dev/null 2>&1; then
    exit 1
  fi
  [ -d "$reclaim_dir" ]
  [ ! -e "$lock_dir" ]
' _ scripts/e2e/portal-mobile-harness-lib.sh

echo "Portal mobile harness syntax, cleanup status/signal bounds, atomic evidence publication, bounded named-resource queries, bounded Android reboot readiness, stale-lock reclaim serialization, exact CDP ownership, evidence pinning, secret-free delivery, and hosted PR filters passed."
