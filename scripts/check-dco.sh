#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if (($# != 2)); then
  echo "Usage: $0 <base-revision> <head-revision>" >&2
  exit 2
fi

root="$(git rev-parse --show-toplevel)"
export REPOSITORY_PATH="$root"
export BASE_SHA="$1"
export HEAD_SHA="$2"
export PR_ACTOR="${PR_ACTOR:-}"

# Compatibility entry point: the authoritative contribution policy now checks
# Conventional Commits and OpenPGP envelopes together with DCO. Keeping this
# wrapper prevents older local commands from silently exercising a weaker gate.
exec node "$root/scripts/ci/contribution-policy.mjs" commits
