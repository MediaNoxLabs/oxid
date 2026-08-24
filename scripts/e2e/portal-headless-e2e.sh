#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
# shellcheck source=scripts/e2e/stack-env-v1.sh
source "$REPO_ROOT/scripts/e2e/stack-env-v1.sh"
readonly PROFILE="${1:-${STACK_ENV_FILE:-}}"
readonly EVIDENCE="${OXID_PORTAL_EVIDENCE_PATH:-$REPO_ROOT/target/portal-headless-e2e/evidence.json}"
readonly RAW_LOG="${TMPDIR:-/tmp}/oxid-portal-headless-e2e-$$.log"
status_file=""
cleanup() {
  local status=$?
  trap - EXIT INT TERM
  trap '' INT TERM
  rm -f -- "${status_file:-}"
  if [ "$status" != 0 ] && [ "${OXID_PORTAL_KEEP_FAILURE_LOG:-0}" = 1 ]; then
    chmod 600 "$RAW_LOG" 2>/dev/null || true
    printf 'portal-headless-e2e: private failure log retained\n' >&2
  else
    rm -f -- "$RAW_LOG"
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fail() { printf 'portal-headless-e2e: FAIL phase=%s\n' "$1" >&2; exit 1; }

[ -n "$PROFILE" ] || fail profile
stack_env_load "$PROFILE" || fail "$STACK_ENV_ERROR"
for command_name in cargo docker git jq mktemp; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"
done
readonly OXID_HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
[ "$OXID_ROOT" = "$REPO_ROOT" ] && [ "$OXID_COMMIT" = "$OXID_HEAD" ] || fail oxid-pin
[ -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-tree-dirty

status_file="$(umask 077 && mktemp "$LOCAL_STACK_STATE_DIR/.portal-status.XXXXXX")" || fail private-state
stack_env_delegate_portal status >"$status_file" 2>>"$RAW_LOG" || fail portal-status
jq -e '.schema == "laceid-oxid-conformance-lifecycle-v2" and .state == "running" and .midnight_state == "ready"' \
  "$status_file" >/dev/null || fail shared-stack-not-ready

if ! PORTAL_INTEGRATION_TREE="$PORTAL_PROTOCOL_SOURCE_DIR" \
  OXID_PORTAL_COMPOSE_PROJECT="$PORTAL_COMPOSE_PROJECT" \
  OXID_PORTAL_HELPER_COMMIT="$PORTAL_HELPER_COMMIT" \
  OXID_PORTAL_HELPER_TREE="$PORTAL_HELPER_TREE" \
  OXID_PORTAL_EVIDENCE_PATH="$EVIDENCE" \
  OXID_PORTAL_EVIDENCE_HEAD="$OXID_HEAD" \
  cargo test --manifest-path "$REPO_ROOT/Cargo.toml" -p oxid-headless \
    --test portal_live_flow \
    landed_portal_service_issues_to_headless_and_restores_in_new_process \
    -- --ignored --exact >>"$RAW_LOG" 2>&1; then
  fail live-flow
fi

[ -f "$EVIDENCE" ] || fail missing-evidence
[ -z "$(git -C "$PORTAL_PROTOCOL_SOURCE_DIR" status --porcelain)" ] || fail portal-tree-mutated
stack_env_delegate_portal status >"$status_file" 2>>"$RAW_LOG" || fail portal-status-after
jq -e '.state == "running" and .midnight_state == "ready"' "$status_file" >/dev/null || fail shared-stack-changed
"$REPO_ROOT/scripts/e2e/validate-portal-headless-evidence.sh" "$EVIDENCE" "$OXID_HEAD" \
  >>"$RAW_LOG" 2>&1 || fail evidence-schema
printf 'portal-headless-e2e: PASS evidence=%s\n' "${EVIDENCE#"$REPO_ROOT/"}"
