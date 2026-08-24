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
  rm -f -- "${status_file:-}" "${evidence_candidate:-}"
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
shared_receipt=""
shared_before_height=""
shared_before_ids=""
evidence_candidate=""
read_shared_height() {
  local response hex
  response="$(curl --fail --silent --connect-timeout 2 --max-time 5 -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' "$SHARED_MIDNIGHT_NODE_HOST_URL")" || return 1
  hex="$(printf '%s' "$response" | jq -r '.result.number // empty')"
  [[ "$hex" =~ ^0x[0-9a-fA-F]+$ ]] || return 1
  printf '%d\n' "$((16#${hex#0x}))"
}
capture_shared_snapshot() {
  local schema mode ids
  [ -f "$shared_receipt" ] && [ ! -L "$shared_receipt" ] || return 1
  if mode="$(stat -c '%a' -- "$shared_receipt" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' -- "$shared_receipt")"; fi
  [ "$mode" = 600 ] || return 1
  schema="$(sed -n '1p' "$shared_receipt")"; shared_before_height="$(sed -n '2p' "$shared_receipt")"; shared_before_ids="$(sed -n '3,$p' "$shared_receipt")"
  [ "$schema" = oxid-laceid-shared-receipt-v1 ] && [[ "$shared_before_height" =~ ^[0-9]+$ ]] || return 1
  [ "$(printf '%s\n' "$shared_before_ids" | grep -c .)" = 3 ] || return 1
  ids="$(docker ps -a --filter "label=com.docker.compose.project=$SHARED_MIDNIGHT_PROJECT" --quiet | sort)"
  [ "$ids" = "$shared_before_ids" ] || return 1
  [ "$(docker ps --filter "label=com.docker.compose.project=$SHARED_MIDNIGHT_PROJECT" --quiet | sort)" = "$shared_before_ids" ] || return 1
  [ "$(read_shared_height)" -ge "$shared_before_height" ]
}
verify_shared_snapshot() {
  local ids height
  ids="$(docker ps -a --filter "label=com.docker.compose.project=$SHARED_MIDNIGHT_PROJECT" --quiet | sort)" || return 1
  [ "$ids" = "$shared_before_ids" ] && [ "$(docker ps --filter "label=com.docker.compose.project=$SHARED_MIDNIGHT_PROJECT" --quiet | sort)" = "$shared_before_ids" ] || return 1
  height="$(read_shared_height)" || return 1
  [ "$height" -ge "$shared_before_height" ]
}

[ -n "$PROFILE" ] || fail profile
stack_env_load "$PROFILE" || fail "$STACK_ENV_ERROR"
shared_receipt="$LOCAL_STACK_STATE_DIR/$PORTAL_COMPOSE_PROJECT.shared-midnight.receipt"
for command_name in cargo curl docker git grep jq mktemp sed sort stat; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"
done
readonly OXID_HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
[ "$OXID_ROOT" = "$REPO_ROOT" ] && [ "$OXID_COMMIT" = "$OXID_HEAD" ] || fail oxid-pin
[ -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-tree-dirty

status_file="$(umask 077 && mktemp "$LOCAL_STACK_STATE_DIR/.portal-status.XXXXXX")" || fail private-state
stack_env_delegate_portal status >"$status_file" 2>>"$RAW_LOG" || fail portal-status
jq -e '.schema == "laceid-oxid-conformance-lifecycle-v2" and .state == "running" and .midnight_state == "ready"' \
  "$status_file" >/dev/null || fail shared-stack-not-ready
capture_shared_snapshot || fail shared-snapshot-before

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
verify_shared_snapshot || fail shared-snapshot-after
evidence_candidate="$(umask 077 && mktemp "$(dirname -- "$EVIDENCE")/.shared-evidence.XXXXXX")" || fail evidence-candidate
jq '.acceptance.sharedMidnightIdentityUnchanged = true' "$EVIDENCE" >"$evidence_candidate" || fail evidence-attestation
chmod 600 "$evidence_candidate" && mv -f -- "$evidence_candidate" "$EVIDENCE" || fail evidence-attestation
[ -z "$(git -C "$PORTAL_PROTOCOL_SOURCE_DIR" status --porcelain)" ] || fail portal-tree-mutated
stack_env_delegate_portal status >"$status_file" 2>>"$RAW_LOG" || fail portal-status-after
jq -e '.state == "running" and .midnight_state == "ready"' "$status_file" >/dev/null || fail shared-stack-changed
"$REPO_ROOT/scripts/e2e/validate-portal-headless-evidence.sh" "$EVIDENCE" "$OXID_HEAD" \
  >>"$RAW_LOG" 2>&1 || fail evidence-schema
printf 'portal-headless-e2e: PASS evidence=%s\n' "${EVIDENCE#"$REPO_ROOT/"}"
