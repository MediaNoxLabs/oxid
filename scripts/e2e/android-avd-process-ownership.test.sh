#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
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

temporary="$(mktemp -d "${TMPDIR:-/tmp}/oxid-avd-contract.XXXXXX")"
cleanup() { rm -rf -- "$temporary"; }
trap cleanup EXIT

cat >"$temporary/hang.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$$" >"$1"
exec sleep 300
EOF
chmod 700 "$temporary/hang.sh"
if timeout -k 1s 2s "$temporary/hang.sh" "$temporary/hang.pid"; then
  fail hanging-command-result
else
  [ "$?" -eq 124 ] || fail hanging-command-result
fi
hang_pid="$(cat "$temporary/hang.pid")"
if kill -0 "$hang_pid" 2>/dev/null; then fail hanging-command-survivor; fi

cat >"$temporary/signal-owner.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
source "$ROOT/scripts/e2e/android-avd-process-ownership.sh"
child_pid=""
child_identity=""
cleanup_child() {
  trap - EXIT INT TERM
  if [ -n "\$child_pid" ] && kill -0 "\$child_pid" 2>/dev/null; then
    oxid_terminate_owned_process "\$child_pid" "\$child_identity" 20
  fi
  if [ -n "\$child_pid" ]; then wait "\$child_pid" 2>/dev/null || true; fi
}
trap cleanup_child EXIT
trap 'exit 143' TERM
sleep 300 &
child_pid=\$!
child_identity="\$(oxid_process_identity "\$child_pid")"
printf '%s\n' "\$child_pid" >"$temporary/child.pid"
printf 'ready\n' >"$temporary/ready"
while kill -0 "\$child_pid" 2>/dev/null; do sleep 0.1; done
EOF
chmod 700 "$temporary/signal-owner.sh"
"$temporary/signal-owner.sh" &
owner_pid=$!
for _attempt in $(seq 1 50); do [ -f "$temporary/ready" ] && break; sleep 0.05; done
[ -f "$temporary/ready" ] || fail signal-fixture-ready
child_pid="$(cat "$temporary/child.pid")"
kill -TERM "$owner_pid"
wait "$owner_pid" 2>/dev/null || [ "$?" -eq 143 ] || fail signal-owner-status
if kill -0 "$child_pid" 2>/dev/null; then fail signal-child-survivor; fi

fixture_identity=$'42\034birth-a\034/bin/tool\034/bin/tool --fixture'
changed_identity=$'42\034birth-b\034/bin/tool\034/bin/tool --fixture'
oxid_same_process_identity "$fixture_identity" "$fixture_identity" || fail identity-equal
if oxid_same_process_identity "$fixture_identity" "$changed_identity"; then
  fail identity-change-refused
fi
oxid_emulator_command_matches "/sdk/emulator -avd exact_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562 || fail emulator-command
if oxid_emulator_command_matches "/sdk/emulator -avd other_avd -read-only -no-snapshot -no-snapshot-save -port 5562" \
  /sdk/emulator exact_avd 5562; then
  fail emulator-command-refusal
fi

printf 'android-avd-process-ownership-contract: PASS timeout=bounded signal_cleanup=no-survivor identity_change=refused\n'
