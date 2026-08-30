#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
# shellcheck source=android-avd-process-ownership.sh
source "$ROOT/scripts/e2e/android-avd-process-ownership.sh"

fail() {
  printf 'android-avd-process-ownership-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

command -v timeout >/dev/null 2>&1 || fail timeout-capability
if timeout -k 1s 0.1s sleep 30; then
  fail timeout-result
else
  [ "$?" -eq 124 ] || fail timeout-result
fi

temporary="$(timeout -k 1s 5s mktemp -d "${TMPDIR:-/tmp}/oxid-avd-contract.XXXXXX")"
cleanup() { timeout -k 1s 5s rm -rf -- "$temporary"; }
trap cleanup EXIT

empty_inventory=$'List of devices attached\n\n'
physical_inventory=$'List of devices attached\nR5CT1234ABC\tdevice product:fixture transport_id:1\n'
mixed_inventory=$'List of devices attached\nemulator-5562\tdevice product:sdk_gphone transport_id:1\nR5CT1234ABC\tdevice product:fixture transport_id:2\n'
wrong_emulator_inventory=$'List of devices attached\nemulator-5554\tdevice product:sdk_gphone transport_id:1\n'
exact_inventory=$'List of devices attached\nemulator-5562\tdevice product:sdk_gphone transport_id:1\n'
oxid_adb_inventory_is_empty "$empty_inventory" || fail adb-empty
if oxid_adb_inventory_is_empty "$physical_inventory"; then fail adb-physical-only; fi
if oxid_adb_inventory_is_empty "$mixed_inventory"; then fail adb-mixed; fi
if oxid_adb_inventory_is_exact_online "$wrong_emulator_inventory" emulator-5562; then fail adb-wrong-serial; fi
if oxid_adb_inventory_is_exact_online "$mixed_inventory" emulator-5562; then fail adb-mixed-exact; fi
oxid_adb_inventory_is_exact_online "$exact_inventory" emulator-5562 || fail adb-exact

cat >"$temporary/fake-adb" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_ADB_INVENTORY_LOG"
[ "$*" = 'devices -l' ] || {
  printf 'MUTATION\n' >>"$OXID_FAKE_ADB_INVENTORY_LOG"
  exit 97
}
printf 'List of devices attached\nR5CT1234ABC\tdevice product:fixture transport_id:1\n'
EOF
chmod 700 "$temporary/fake-adb"
: >"$temporary/adb-inventory.log"
if OXID_FAKE_ADB_INVENTORY_LOG="$temporary/adb-inventory.log" \
  oxid_require_empty_adb_inventory "$temporary/fake-adb"; then
  fail adb-physical-preflight
fi
[ "$(wc -l <"$temporary/adb-inventory.log" | tr -d ' ')" -eq 1 ] || fail adb-physical-mutation
[ "$(<"$temporary/adb-inventory.log")" = 'devices -l' ] || fail adb-physical-command

cat >"$temporary/grandchild.sh" <<'EOF'
#!/usr/bin/env bash
trap 'printf "TERM\n" >"$2"' TERM
printf '%s\n' "$$" >"$1"
while :; do sleep 1; done
EOF
chmod 700 "$temporary/grandchild.sh"
cat >"$temporary/owner.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$$" >"$1"
bash "$2" "$3" "$4"
status=$?
printf '%s\n' "$status" >/dev/null
EOF
chmod 700 "$temporary/owner.sh"
timeout -k 1s 30s "$temporary/owner.sh" "$temporary/owner.pid" \
  "$temporary/grandchild.sh" "$temporary/grandchild.pid" "$temporary/term.seen" &
supervisor_pid=$!
for ((_attempt = 0; _attempt < 200; _attempt++)); do
  [ -s "$temporary/grandchild.pid" ] && break
  timeout -k 1s 1s sleep 0.05
done
[ -s "$temporary/grandchild.pid" ] || fail process-group-ready
owner_pid="$(<"$temporary/owner.pid")"
grandchild_pid="$(<"$temporary/grandchild.pid")"
oxid_job_is_running "$supervisor_pid" || fail supervisor-job
oxid_terminate_supervised_job "$supervisor_pid" || fail process-group-result
[ -f "$temporary/term.seen" ] || fail grandchild-term
process_is_live() {
  local state
  state="$(timeout -k 1s 5s ps -p "$1" -o stat= 2>/dev/null || true)"
  [ -n "$state" ] && [[ "$state" != Z* ]]
}
if process_is_live "$owner_pid" || process_is_live "$grandchild_pid"; then
  fail process-group-survivor
fi

sleep 30 &
direct_pid=$!
oxid_direct_child_owned "$direct_pid" "$$" || fail direct-child-owned
kill -TERM "$direct_pid"
wait "$direct_pid" 2>/dev/null || true

timeout -k 1s 30s bash -c 'sleep 30 & printf "%s\n" "$!" >"$1"; wait' \
  _ "$temporary/changed-parent.pid" &
intermediary_pid=$!
for ((_attempt = 0; _attempt < 200; _attempt++)); do
  [ -s "$temporary/changed-parent.pid" ] && break
  timeout -k 1s 1s sleep 0.05
done
changed_parent_pid="$(<"$temporary/changed-parent.pid")"
if oxid_direct_child_owned "$changed_parent_pid" "$$"; then
  fail changed-parent-refused
fi
oxid_terminate_supervised_job "$intermediary_pid" || fail changed-parent-cleanup
if process_is_live "$changed_parent_pid"; then fail changed-parent-survivor; fi

oxid_emulator_command_matches "/sdk/emulator -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562 || fail emulator-command
oxid_emulator_command_matches "/sdk/qemu/darwin-aarch64/qemu-system-aarch64 -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562 || fail emulator-qemu-exec-command
if oxid_emulator_command_matches "/sdk/other -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-executable-refusal
fi
if oxid_emulator_command_matches "/sdk/qemu/darwin-aarch64/nested/qemu-system-aarch64 -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-qemu-nested-refusal
fi
if oxid_emulator_command_matches "/other/qemu/darwin-aarch64/qemu-system-aarch64 -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-qemu-sdk-refusal
fi
if oxid_emulator_command_matches "/sdk/emulator -avd other_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-avd-refusal
fi

cat >"$temporary/emulator.mjs" <<'EOF'
import fs from "node:fs";
process.on("SIGTERM", () => fs.writeFileSync(process.env.OXID_FAKE_EMULATOR_TERM, "TERM\n"));
fs.writeFileSync(process.env.OXID_FAKE_EMULATOR_READY, "READY\n");
setInterval(() => {}, 1000);
EOF
OXID_FAKE_EMULATOR_READY="$temporary/emulator-ready.seen" \
OXID_FAKE_EMULATOR_TERM="$temporary/emulator-term.seen" \
  node "$temporary/emulator.mjs" -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562 &
fake_emulator_pid=$!
fake_emulator_executable=node
for ((_attempt = 0; _attempt < 200; _attempt++)); do
  [ -f "$temporary/emulator-ready.seen" ] \
    && oxid_emulator_job_owned "$fake_emulator_pid" "$$" "$fake_emulator_executable" exact_avd 5562 \
    && break
  timeout -k 1s 1s sleep 0.05
done
[ -f "$temporary/emulator-ready.seen" ] || fail direct-emulator-ready
oxid_emulator_job_owned "$fake_emulator_pid" "$$" "$fake_emulator_executable" exact_avd 5562 \
  || fail direct-emulator-owned
oxid_terminate_emulator_job "$fake_emulator_pid" "$$" "$fake_emulator_executable" exact_avd 5562 \
  || fail direct-emulator-cleanup
[ -f "$temporary/emulator-term.seen" ] || fail direct-emulator-term
process_is_live "$fake_emulator_pid" && fail direct-emulator-survivor

fixture_root="$temporary/refusal-repository"
timeout -k 1s 5s mkdir -p "$fixture_root/scripts/e2e" "$fixture_root/scripts" "$temporary/fake-bin"
timeout -k 1s 5s cp "$ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" \
  "$ROOT/scripts/e2e/android-avd-process-ownership.sh" "$fixture_root/scripts/e2e/"
timeout -k 1s 5s cp "$ROOT/scripts/test-android-portal-exact-sequence-avd.sh" "$fixture_root/scripts/"
cat >"$temporary/fake-bin/docker" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_DOCKER_LOG"
case "${1:-}" in
  info) exit 0 ;;
  ps)
    count=0
    [ ! -f "$OXID_FAKE_DOCKER_COUNT" ] || count="$(cat "$OXID_FAKE_DOCKER_COUNT")"
    count=$((count + 1))
    printf '%s\n' "$count" >"$OXID_FAKE_DOCKER_COUNT"
    if [ "$count" -eq 1 ]; then
      behavior="${OXID_FAKE_DOCKER_INITIAL:-empty}"
    else
      behavior="${OXID_FAKE_DOCKER_CLEANUP:-empty}"
      receipt="$(find "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock" \
        -mindepth 1 -maxdepth 1 -type d -name 'receipt-*' -print -quit)"
      case "${OXID_FAKE_STACK_MUTATION:-none}" in
        receipt-nonempty) printf 'block\n' >"$receipt/blocker" ;;
        replace-receipt)
          rmdir "$receipt" && mkdir "$receipt" && printf 'foreign\n' >"$receipt/marker"
          ;;
        replace-lock)
          rm -rf "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock"
          mkdir "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock"
          printf 'foreign\n' >"$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock/marker"
          ;;
      esac
    fi
    case "$behavior" in
      empty) ;;
      nonempty) printf 'occupied-public-project\n' ;;
      error) exit 96 ;;
      timeout) sleep 30 ;;
      *) exit 95 ;;
    esac
    ;;
  *) exit 97 ;;
esac
EOF
cat >"$temporary/fake-bin/git" <<'EOF'
#!/usr/bin/env bash
if [ "${3:-}" = status ]; then exit 0; fi
if [ "${1:-}" = clone ]; then
  if [ "${OXID_FAKE_GIT_CLONE_MODE:-fail}" = block ]; then
    printf 'ready\n' >"$OXID_FAKE_GIT_READY"
    sleep 2
  fi
fi
exit 97
EOF
chmod 700 "$temporary/fake-bin/docker" "$temporary/fake-bin/git"

run_stack_fixture() {
  local name="$1" initial="$2" cleanup_behavior="$3" mutation="$4"
  rm -rf -- "$fixture_root/target"
  : >"$temporary/docker-$name.log"
  rm -f -- "$temporary/docker-$name.count"
  if OXID_FAKE_DOCKER_LOG="$temporary/docker-$name.log" \
    OXID_FAKE_DOCKER_COUNT="$temporary/docker-$name.count" \
    OXID_FAKE_DOCKER_INITIAL="$initial" OXID_FAKE_DOCKER_CLEANUP="$cleanup_behavior" \
    OXID_FAKE_STACK_MUTATION="$mutation" OXID_FAKE_STACK_ROOT="$fixture_root" \
    OXID_STACK_DOCKER_QUERY_TIMEOUT_SECONDS=0.2 PATH="$temporary/fake-bin:$PATH" \
    timeout -k 1s 8s "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
    >"$temporary/$name.out" 2>"$temporary/$name.err"; then
    fail "$name-result"
  fi
}

run_stack_fixture occupied-project nonempty empty none
grep -qF 'FAIL phase=occupied-project' "$temporary/occupied-project.err" || fail occupied-project-phase
[ ! -e "$fixture_root/target" ] || fail occupied-cleanup-mutation

run_stack_fixture initial-query-error error empty none
grep -qF 'FAIL phase=docker-query' "$temporary/initial-query-error.err" || fail initial-query-error-phase
[ ! -e "$fixture_root/target" ] || fail initial-query-error-mutation

for behavior in error timeout nonempty; do
  run_stack_fixture "cleanup-$behavior" empty "$behavior" none
  grep -qF 'cleanup could not prove owned-state restoration' "$temporary/cleanup-$behavior.err" \
    || fail "cleanup-$behavior-phase"
  [ -d "$fixture_root/target/portal-virtual-mobile/runtime" ] || fail "cleanup-$behavior-state"
  receipt_path="$(printf '%s\n' "$fixture_root"/target/portal-virtual-mobile/stack.lock/receipt-*)"
  [ -d "$receipt_path" ] || fail "cleanup-$behavior-receipt"
done

run_stack_fixture cleanup-empty empty empty none
[ ! -e "$fixture_root/target/portal-virtual-mobile/runtime" ] || fail cleanup-empty-state
[ ! -e "$fixture_root/target/portal-virtual-mobile/stack.lock" ] || fail cleanup-empty-lock

for mutation in receipt-nonempty replace-receipt replace-lock; do
  run_stack_fixture "lock-$mutation" empty empty "$mutation"
  grep -qF 'owned lock proof/removal failed' "$temporary/lock-$mutation.err" \
    || fail "lock-$mutation-phase"
  [ -d "$fixture_root/target/portal-virtual-mobile/stack.lock" ] || fail "lock-$mutation-preserved"
done
[ -f "$fixture_root/target/portal-virtual-mobile/stack.lock/marker" ] || fail foreign-parent-marker

rm -rf -- "$fixture_root/target"
timeout -k 1s 5s mkdir -p "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt"
printf 'foreign\n' >"$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker"
foreign_before="$(oxid_filesystem_identity "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker")"
rm -f -- "$temporary/docker-foreign-lock.count"
if OXID_FAKE_DOCKER_LOG="$temporary/docker-foreign-lock.log" \
  OXID_FAKE_DOCKER_COUNT="$temporary/docker-foreign-lock.count" OXID_FAKE_DOCKER_INITIAL=empty \
  OXID_FAKE_STACK_ROOT="$fixture_root" PATH="$temporary/fake-bin:$PATH" timeout -k 1s 8s \
  "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
  >"$temporary/foreign-lock.out" 2>"$temporary/foreign-lock.err"; then
  fail foreign-lock-result
fi
grep -qF 'FAIL phase=occupied-lock' "$temporary/foreign-lock.err" || fail foreign-lock-phase
grep -qF 'owner-reviewed stale-lock recovery' "$temporary/foreign-lock.err" || fail foreign-lock-guidance
foreign_after="$(oxid_filesystem_identity "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker")"
[ "$foreign_before" = "$foreign_after" ] || fail foreign-lock-mutated

rm -rf -- "$fixture_root/target"
timeout -k 1s 5s mkdir -p "$fixture_root/target/android-portal-exact-sequence-avd"
printf '{"stale":true}\n' >"$fixture_root/target/android-portal-exact-sequence-avd/evidence.json"
stale_before="$(shasum -a 256 "$fixture_root/target/android-portal-exact-sequence-avd/evidence.json")"
if timeout -k 1s 8s "$fixture_root/scripts/test-android-portal-exact-sequence-avd.sh" \
  >"$temporary/stale-evidence.out" 2>"$temporary/stale-evidence.err"; then
  fail stale-evidence-result
fi
grep -qF 'FAIL phase=occupied-evidence' "$temporary/stale-evidence.err" || fail stale-evidence-phase
stale_after="$(shasum -a 256 "$fixture_root/target/android-portal-exact-sequence-avd/evidence.json")"
[ "$stale_before" = "$stale_after" ] || fail stale-evidence-mutated
[ "$(find "$fixture_root/target/android-portal-exact-sequence-avd" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" -eq 1 ] \
  || fail stale-evidence-artifact
if grep -Eq 'mv[[:space:]].*-f.*EVIDENCE' "$fixture_root/scripts/test-android-portal-exact-sequence-avd.sh"; then
  fail evidence-force-move
fi
for runner in \
  "$ROOT/scripts/test-android-portal-exact-sequence-avd.sh" \
  "$ROOT/scripts/test-ios-portal-exact-sequence-simulator.sh"; do
  grep -qF 'oxid_poll_job_dead "$portal_pid" 1200 || true' "$runner" \
    || fail portal-cleanup-grace
  grep -qF 'if [ "$build_owned" -eq 1 ] && [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ]; then' "$runner" \
    || fail failed-build-preservation
  grep -qF 'if [ "$private_state_owned" -eq 1 ] && [ "$incoming" -eq 0 ] && [ "$cleanup_ok" = true ]; then' "$runner" \
    || fail failed-log-preservation
  grep -qF 'BUILD_SOURCE="$(run_deadline 5 mktemp -d "${TMPDIR:-/tmp}/oxid-' "$runner" \
    || fail detached-build-source
  grep -qF 'oxid_path_has_identity "$BUILD_SOURCE" "$build_identity"' "$runner" \
    || fail build-source-identity
  if grep -qF 'readonly BUILD_SOURCE="$PRIVATE_STATE/build-source"' "$runner"; then
    fail nested-build-source
  fi
done
grep -qF 'run_deadline 5 mkdir "$xcode_project" || fail xcode-project-create' \
  "$ROOT/scripts/test-ios-portal-exact-sequence-simulator.sh" || fail xcode-project-parent

acquisition_parent="$temporary/evidence-acquisition"
timeout -k 1s 5s mkdir "$acquisition_parent"
: >"$temporary/acquisition-winners"
for contender in one two; do
  (
    if mkdir "$acquisition_parent/run" 2>/dev/null; then
      printf '%s\n' "$contender" >>"$temporary/acquisition-winners"
      sleep 0.2
    fi
  ) &
done
wait
[ "$(wc -l <"$temporary/acquisition-winners" | tr -d ' ')" -eq 1 ] || fail concurrent-evidence-acquisition

rm -rf -- "$fixture_root/target"
rm -f -- "$temporary/docker-signal.count" "$temporary/signal-ready"
OXID_FAKE_DOCKER_LOG="$temporary/docker-signal.log" OXID_FAKE_DOCKER_COUNT="$temporary/docker-signal.count" \
  OXID_FAKE_DOCKER_INITIAL=empty OXID_FAKE_DOCKER_CLEANUP=empty OXID_FAKE_GIT_CLONE_MODE=block \
  OXID_FAKE_GIT_READY="$temporary/signal-ready" OXID_FAKE_STACK_ROOT="$fixture_root" \
  PATH="$temporary/fake-bin:$PATH" "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
  >"$temporary/signal.out" 2>"$temporary/signal.err" &
signal_pid=$!
for ((_attempt = 0; _attempt < 50; _attempt++)); do
  [ -f "$temporary/signal-ready" ] && break
  timeout -k 1s 1s sleep 0.05
done
[ -f "$temporary/signal-ready" ] || fail signal-TERM-partial-ready
kill -TERM "$signal_pid"
for ((_attempt = 0; _attempt < 80; _attempt++)); do
  kill -0 "$signal_pid" 2>/dev/null || break
  timeout -k 1s 1s sleep 0.05
done
if kill -0 "$signal_pid" 2>/dev/null; then
  kill -KILL "$signal_pid" 2>/dev/null || true
  fail signal-TERM-cleanup-timeout
fi
signal_status=0
wait "$signal_pid" 2>/dev/null || signal_status=$?
[ "$signal_status" -eq 143 ] || fail signal-TERM-status
[ ! -e "$fixture_root/target/portal-virtual-mobile/runtime" ] || fail signal-TERM-state
[ ! -e "$fixture_root/target/portal-virtual-mobile/stack.lock" ] || fail signal-TERM-lock

rm -rf -- "$fixture_root/target"
rm -f -- "$temporary/docker-signal-int.count" "$temporary/signal-int-ready"
if OXID_FAKE_DOCKER_LOG="$temporary/docker-signal-int.log" \
  OXID_FAKE_DOCKER_COUNT="$temporary/docker-signal-int.count" \
  OXID_FAKE_DOCKER_INITIAL=empty OXID_FAKE_DOCKER_CLEANUP=empty OXID_FAKE_GIT_CLONE_MODE=block \
  OXID_FAKE_GIT_READY="$temporary/signal-int-ready" OXID_FAKE_STACK_ROOT="$fixture_root" \
  PATH="$temporary/fake-bin:$PATH" timeout -s INT -k 3s 0.5s \
  "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
  >"$temporary/signal-int.out" 2>"$temporary/signal-int.err"; then
  fail signal-INT-result
fi
[ -f "$temporary/signal-int-ready" ] || fail signal-INT-partial-ready
[ ! -e "$fixture_root/target/portal-virtual-mobile/runtime" ] || fail signal-INT-state
[ ! -e "$fixture_root/target/portal-virtual-mobile/stack.lock" ] || fail signal-INT-lock

launcher_bin="$temporary/launcher-bin"
launcher_sdk="$temporary/android-sdk"
timeout -k 1s 5s mkdir -p "$launcher_bin" "$launcher_sdk/platform-tools"
for tool in nix rustup java node; do
  cat >"$launcher_bin/$tool" <<'EOF'
#!/usr/bin/env bash
exit 97
EOF
  chmod 700 "$launcher_bin/$tool"
done
cat >"$launcher_sdk/platform-tools/adb" <<'EOF'
#!/usr/bin/env bash
parent="$(ps -p "$PPID" -o comm= 2>/dev/null)"
printf 'parent=%s serial=%s args=%s\n' "$parent" "${ANDROID_SERIAL:-unset}" "$*" >>"$OXID_FAKE_ADB_LOG"
case "${1:-}" in
  devices) printf 'List of devices attached\nfixture-device\tdevice\n' ;;
  get-state) printf 'offline\n' ;;
esac
EOF
chmod 700 "$launcher_sdk/platform-tools/adb"
fake_adb_log="$temporary/adb.log"
if OXID_FAKE_ADB_LOG="$fake_adb_log" ANDROID_HOME="$launcher_sdk" \
  PATH="$launcher_bin:$PATH" timeout -k 1s 10s "$ROOT/scripts/run-android-emulator.sh" \
  >"$temporary/launcher.out" 2>"$temporary/launcher.err"; then
  fail launcher-unset-result
fi
grep -qF 'selected Android device is not online' "$temporary/launcher.err" || fail launcher-unset-phase
[ "$(wc -l <"$fake_adb_log" | tr -d ' ')" -eq 2 ] || fail launcher-adb-paths
if grep -q 'parent=timeout' "$fake_adb_log"; then fail launcher-unset-timeout; fi
grep -q 'serial=unset args=devices' "$fake_adb_log" || fail launcher-discovery-wrapper
grep -q 'serial=fixture-device args=get-state' "$fake_adb_log" || fail launcher-selected-wrapper

printf 'android-avd-process-ownership-contract: PASS process_group=bounded-term-kill direct_emulator=bounded-term-kill docker_query=error-timeout-nonempty evidence=no-clobber-concurrent lock=identity-preserved signal=partial-readiness failure=diagnostics-preserved adb_inventory=physical-mixed-refused adb_unset=normal\n'
