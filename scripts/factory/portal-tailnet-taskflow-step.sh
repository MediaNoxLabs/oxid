#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"

# shellcheck source=../lib/nix-path.sh
source "$REPOSITORY_ROOT/scripts/lib/nix-path.sh"
oxid_admit_daemon_nix_path
# shellcheck source=../lib/docker-engine-health.sh
source "$REPOSITORY_ROOT/scripts/lib/docker-engine-health.sh"

readonly OPERATION="${1:-}"
readonly MODE="${2:-}"

fail() {
  printf 'portal-tailnet-taskflow: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ "$MODE" = prepare-only ] || fail mode

case "$OPERATION" in
  preflight)
    for command_name in docker git jq nix node openssl shasum; do
      command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"
    done
    oxid_require_docker_engine >/dev/null || fail docker-engine
    [ -x "$REPOSITORY_ROOT/scripts/test-android-portal-tailnet-physical.sh" ] || fail lifecycle-entrypoint
    [ -f "$REPOSITORY_ROOT/.pi/taskflows/flows/demos/portal-tailnet-prepare.json" ] || fail flow-definition
    [ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail tracked-tree-dirty
    printf '%s\n' 'portal-tailnet-taskflow: PREFLIGHT mode=prepare-only device=not-required tailnet=not-required'
    ;;
  handoff)
    printf '%s\n' 'portal-tailnet-taskflow: PREPARED next=owner-gated-tailnet-start device=not-yet-required'
    ;;
  *)
    fail usage
    ;;
esac
