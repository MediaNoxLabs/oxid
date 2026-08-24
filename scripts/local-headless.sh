#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Thin cross-owner orchestration for the reviewed shared headless profile.

set -euo pipefail
export LC_ALL=C
CDPATH=
readonly repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=scripts/e2e/stack-env-v1.sh
source "$repository_root/scripts/e2e/stack-env-v1.sh"
readonly standalone="$repository_root/scripts/standalone-lifecycle.sh"
operation="${1:-}"
profile="${2:-}"
[ "$#" -eq 2 ] || { printf 'local-headless: error=usage\n' >&2; exit 2; }
case "$operation" in up|status|test|down) ;; *) printf 'local-headless: error=usage\n' >&2; exit 2 ;; esac
if ! stack_env_load "$profile"; then printf 'local-headless: error=%s\n' "$STACK_ENV_ERROR" >&2; exit 2; fi
for command_name in jq mktemp; do command -v "$command_name" >/dev/null 2>&1 || {
  printf 'local-headless: error=missing_tool\n' >&2; exit 2;
}
done
[ -x "$standalone" ] || { printf 'local-headless: error=invalid_helper\n' >&2; exit 2; }

midnight_result=""
portal_result=""
cleanup_files() { rm -f -- "${midnight_result:-}" "${portal_result:-}"; }
trap cleanup_files EXIT INT TERM
new_result_file() { umask 077; mktemp "$LOCAL_STACK_STATE_DIR/.local-headless-result.XXXXXX"; }
combine_results() {
  local output_operation="$1"
  jq -cn --slurpfile midnight "$midnight_result" --slurpfile portal "$portal_result" \
    --arg operation "$output_operation" \
    '{schema:"oxid-local-headless-lifecycle-v1",operation:$operation,profile:"headless",midnight:$midnight[0],portal:$portal[0]}'
}

run_status() {
  midnight_result="$(new_result_file)"; portal_result="$(new_result_file)"
  "$standalone" status "$STACK_ENV_PATH" >"$midnight_result" || return 1
  stack_env_delegate_portal status >"$portal_result" || return 1
  combine_results status
}

run_up() {
  local receipt="$LOCAL_STACK_STATE_DIR/oxid-standalone.owner.receipt" started_midnight=0
  # This non-mutating call delegates full secret/schema validation to the exact
  # authenticated Portal helper before either owner mutates its project.
  stack_env_delegate_portal status >/dev/null || {
    printf 'local-headless: error=portal_validation_failed\n' >&2
    return 1
  }
  [ -e "$receipt" ] || [ -L "$receipt" ] || started_midnight=1
  midnight_result="$(new_result_file)"; portal_result="$(new_result_file)"
  if ! "$standalone" ensure "$STACK_ENV_PATH" >"$midnight_result"; then return 1; fi
  if ! stack_env_delegate_portal up >"$portal_result"; then
    # Roll back Midnight only when this call created the exact owner receipt.
    # Attach and prior-owner invocations never stop shared infrastructure.
    if [ "$started_midnight" = 1 ] && [ -f "$receipt" ] && [ ! -L "$receipt" ]; then
      "$standalone" down "$STACK_ENV_PATH" >/dev/null 2>&1 || true
    fi
    printf 'local-headless: error=portal_up_failed\n' >&2
    return 1
  fi
  combine_results up
}

run_down() {
  midnight_result="$(new_result_file)"; portal_result="$(new_result_file)"
  # Portal is always asked to release only its own exact project first. A
  # continuity failure stops here; Oxid does not compound an ambiguous cleanup.
  if ! stack_env_delegate_portal down >"$portal_result"; then
    printf 'local-headless: error=portal_down_failed\n' >&2
    return 1
  fi
  "$standalone" down "$STACK_ENV_PATH" >"$midnight_result" || return 1
  combine_results down
}

case "$operation" in
  up) run_up ;;
  status) run_status ;;
  test)
    run_status >/dev/null
    STACK_ENV_FILE="$STACK_ENV_PATH" "$repository_root/scripts/e2e/portal-headless-e2e.sh" "$STACK_ENV_PATH"
    ;;
  down) run_down ;;
esac
