#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -uo pipefail

if (($# == 0)); then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

command_status=0
"$@" || command_status=$?

# Cache telemetry must never hide the command's result. It makes cache misses,
# non-cacheable incremental crates, and backend errors attributable in each
# hosted lane without archiving a target directory.
if [[ "${SCCACHE_GHA_RW_MODE:-READ_WRITE}" == "READ_ONLY" ]]; then
  echo "sccache remote mode: READ_ONLY (write-error counters are expected for rejected local puts)"
fi
sccache --show-stats || true

exit "$command_status"
