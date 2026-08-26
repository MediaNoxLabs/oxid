#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
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

cat >"$temporary/grandchild.sh" <<'EOF'
#!/usr/bin/env bash
trap 'printf "TERM\n" >"$2"' TERM
printf '%s\n' "$BASHPID" >"$1"
while :; do sleep 1; done
EOF
chmod 700 "$temporary/grandchild.sh"
cat >"$temporary/owner.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$BASHPID" >"$1"
bash "$2" "$3" "$4"
status=$?
printf '%s\n' "$status" >/dev/null
EOF
chmod 700 "$temporary/owner.sh"
timeout -k 1s 30s "$temporary/owner.sh" "$temporary/owner.pid" \
  "$temporary/grandchild.sh" "$temporary/grandchild.pid" "$temporary/term.seen" &
supervisor_pid=$!
for ((_attempt = 0; _attempt < 50; _attempt++)); do
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
oxid_direct_child_owned "$direct_pid" "$BASHPID" || fail direct-child-owned
kill -TERM "$direct_pid"
wait "$direct_pid" 2>/dev/null || true

timeout -k 1s 30s bash -c 'sleep 30 & printf "%s\n" "$!" >"$1"; wait' \
  _ "$temporary/changed-parent.pid" &
intermediary_pid=$!
for ((_attempt = 0; _attempt < 50; _attempt++)); do
  [ -s "$temporary/changed-parent.pid" ] && break
  timeout -k 1s 1s sleep 0.05
done
changed_parent_pid="$(<"$temporary/changed-parent.pid")"
if oxid_direct_child_owned "$changed_parent_pid" "$BASHPID"; then
  fail changed-parent-refused
fi
oxid_terminate_supervised_job "$intermediary_pid" || fail changed-parent-cleanup
if process_is_live "$changed_parent_pid"; then fail changed-parent-survivor; fi

oxid_emulator_command_matches "/sdk/emulator -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562 || fail emulator-command
if oxid_emulator_command_matches "/sdk/other -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-executable-refusal
fi
if oxid_emulator_command_matches "/sdk/emulator -avd other_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-avd-refusal
fi

fixture_root="$temporary/refusal-repository"
timeout -k 1s 5s mkdir -p "$fixture_root/scripts/e2e" "$temporary/fake-bin"
timeout -k 1s 5s cp "$ROOT/scripts/e2e/portal-virtual-mobile-stack.sh" \
  "$ROOT/scripts/e2e/android-avd-process-ownership.sh" "$fixture_root/scripts/e2e/"
cat >"$temporary/fake-bin/docker" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_DOCKER_LOG"
case "${1:-}" in
  info) exit 0 ;;
  ps) [ "${OXID_FAKE_PROJECT_PRESENT:-1}" = 1 ] && printf 'occupied-public-project\n' ;;
  *) exit 97 ;;
esac
EOF
cat >"$temporary/fake-bin/git" <<'EOF'
#!/usr/bin/env bash
[ "${3:-}" = status ] && exit 0
exit 97
EOF
chmod 700 "$temporary/fake-bin/docker" "$temporary/fake-bin/git"
fake_docker_log="$temporary/docker.log"
public_process_before="$(timeout -k 1s 5s ps -p "$$" -o pid=,ppid=)"
public_listener_before="$(timeout -k 1s 5s lsof -nP -iTCP:19876 -sTCP:LISTEN -Fpcn 2>/dev/null || true)"
public_project_before="$(OXID_FAKE_DOCKER_LOG="$fake_docker_log" PATH="$temporary/fake-bin:$PATH" docker ps -a --quiet)"
if OXID_FAKE_DOCKER_LOG="$fake_docker_log" PATH="$temporary/fake-bin:$PATH" \
  timeout -k 1s 10s "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
  >"$temporary/refusal.out" 2>"$temporary/refusal.err"; then
  fail occupied-project-result
fi
grep -qF 'FAIL phase=occupied-project' "$temporary/refusal.err" || fail occupied-project-phase
public_process_after="$(timeout -k 1s 5s ps -p "$$" -o pid=,ppid=)"
public_listener_after="$(timeout -k 1s 5s lsof -nP -iTCP:19876 -sTCP:LISTEN -Fpcn 2>/dev/null || true)"
public_project_after="$(OXID_FAKE_DOCKER_LOG="$fake_docker_log" PATH="$temporary/fake-bin:$PATH" docker ps -a --quiet)"
[ "$public_process_before" = "$public_process_after" ] || fail occupied-process-mutated
[ "$public_listener_before" = "$public_listener_after" ] || fail occupied-listener-mutated
[ "$public_project_before" = "$public_project_after" ] || fail occupied-project-mutated
[ ! -e "$fixture_root/target" ] || fail occupied-cleanup-mutation
[ ! -e "$fixture_root/target/android-portal-exact-sequence-avd/evidence.json" ] || fail occupied-artifact

timeout -k 1s 5s mkdir -p "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt"
printf 'foreign\n' >"$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker"
foreign_before="$(timeout -k 1s 5s stat -c '%i:%s' \
  "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker" 2>/dev/null || \
  timeout -k 1s 5s stat -f '%i:%z' \
    "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker")"
if OXID_FAKE_PROJECT_PRESENT=0 OXID_FAKE_DOCKER_LOG="$fake_docker_log" \
  PATH="$temporary/fake-bin:$PATH" timeout -k 1s 10s \
  "$fixture_root/scripts/e2e/portal-virtual-mobile-stack.sh" \
  >"$temporary/lock.out" 2>"$temporary/lock.err"; then
  fail foreign-lock-result
fi
grep -qF 'FAIL phase=occupied-lock' "$temporary/lock.err" || fail foreign-lock-phase
foreign_after="$(timeout -k 1s 5s stat -c '%i:%s' \
  "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker" 2>/dev/null || \
  timeout -k 1s 5s stat -f '%i:%z' \
    "$fixture_root/target/portal-virtual-mobile/stack.lock/foreign-receipt/marker")"
[ "$foreign_before" = "$foreign_after" ] || fail foreign-lock-mutated

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

printf 'android-avd-process-ownership-contract: PASS process_group=term-then-kill-no-survivor direct_child=owned changed_parent=refused identity_change=refused occupied_project=unchanged-no-artifact foreign_lock=preserved adb_unset=normal\n'
