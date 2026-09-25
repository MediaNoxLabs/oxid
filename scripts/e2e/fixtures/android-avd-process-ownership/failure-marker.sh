#!/usr/bin/env bash
set -euo pipefail
source "$1"
mode="$2"
cleanup_marker="$3"
oxid_android_avd_failure_marker_reset
failure_phase="unreported-timeout-or-abort"
cleanup() {
  incoming=$?
  oxid_android_avd_emit_failure_marker "$incoming" "$failure_phase"
  : >"$cleanup_marker"
  exit "$incoming"
}
trap cleanup EXIT
case "$mode" in
  nonzero)
    failure_phase="nonzero"
    exit 1
    ;;
  unreported)
    false
    ;;
  signal|timeout)
    wait_pid=""
    trap 'failure_phase=signal-term; if [ -n "${wait_pid:-}" ]; then kill -TERM "$wait_pid" 2>/dev/null || true; wait "$wait_pid" 2>/dev/null || true; fi; exit 143' TERM
    if [ "$mode" = signal ]; then kill -TERM "$$"; fi
    # A background child plus an explicit wait gives Bash an interruptible
    # boundary without emitting platform-specific foreground-job diagnostics.
    sleep 30 &
    wait_pid=$!
    wait "$wait_pid" 2>/dev/null || true
    ;;
  *) exit 64 ;;
esac
