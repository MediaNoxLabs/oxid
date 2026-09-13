#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -uo pipefail

if (($# == 0)); then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

command_status=0
diagnostic_log=""

cleanup() {
  [[ -z "$diagnostic_log" ]] || rm -f -- "$diagnostic_log"
}
trap cleanup EXIT

if [[ "${SCCACHE_BACKEND_DIAGNOSTICS:-off}" == "on" ]] \
  && diagnostic_log=$(umask 077; mktemp "${TMPDIR:-/tmp}/oxid-sccache-backend-errors.XXXXXX"); then
  # Capture only sccache's own error channel. The wrapped build keeps its
  # ordinary stdout/stderr so a real compiler or test failure stays visible.
  SCCACHE_ERROR_LOG="$diagnostic_log" SCCACHE_LOG=error "$@" || command_status=$?
else
  "$@" || command_status=$?
fi

# Cache telemetry must never hide the command's result. It makes cache misses,
# non-cacheable incremental crates, and backend errors attributable in each
# hosted lane without archiving a target directory.
if [[ "${SCCACHE_GHA_RW_MODE:-READ_WRITE}" == "READ_ONLY" ]]; then
  echo "sccache remote mode: READ_ONLY (write-error counters are expected for rejected local puts)"
fi
sccache --show-stats || true
if [[ -n "$diagnostic_log" ]]; then
  node "$(dirname "$0")/sccache-backend-error-counts.mjs" "$diagnostic_log"
fi

exit "$command_status"
