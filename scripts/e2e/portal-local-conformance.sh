#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly PROFILE="${1:-${STACK_ENV_FILE:-}}"
# shellcheck source=scripts/e2e/stack-env-v1.sh
source "$REPOSITORY_ROOT/scripts/e2e/stack-env-v1.sh"
[ -n "$PROFILE" ] || { printf 'portal-local-conformance: FAIL phase=profile\n' >&2; exit 2; }
stack_env_load "$PROFILE" || { printf 'portal-local-conformance: FAIL phase=%s\n' "$STACK_ENV_ERROR" >&2; exit 2; }
readonly SOURCE_TREE="$PORTAL_PROTOCOL_SOURCE_DIR"
readonly LOCAL_HEADLESS="$REPOSITORY_ROOT/scripts/local-headless.sh"
readonly SOURCE_VALIDATOR="$REPOSITORY_ROOT/scripts/e2e/validate-portal-source-checkout.sh"
readonly EVIDENCE_TOOL="$REPOSITORY_ROOT/scripts/e2e/portal-local-evidence.mjs"
readonly RESOURCE_CHECKER="$REPOSITORY_ROOT/scripts/e2e/check-portal-resource-leaks.sh"
readonly LOCK_RUNNER="$REPOSITORY_ROOT/scripts/e2e/with-portal-local-lock.sh"
readonly LOCK_FILE="${TMPDIR:-/tmp}/oxid-portal-local-conformance-$(id -u).lock"
readonly STAGING_ROOT="$REPOSITORY_ROOT/target/portal-local-conformance/.run-$$"
readonly STAGED_HEADLESS="$STAGING_ROOT/evidence/headless.json"
readonly STAGED_IOS="$STAGING_ROOT/evidence/ios.json"
readonly STAGED_ANDROID="$STAGING_ROOT/evidence/android.json"
readonly RETAINED_HEADLESS="$REPOSITORY_ROOT/target/portal-headless-e2e/evidence.json"
readonly RETAINED_IOS="$REPOSITORY_ROOT/target/portal-mobile-e2e/ios/evidence.json"
readonly RETAINED_ANDROID="$REPOSITORY_ROOT/target/portal-mobile-e2e/android/evidence.json"
readonly PRIOR_HEADLESS="$STAGING_ROOT/prior/headless.json"
readonly PRIOR_IOS="$STAGING_ROOT/prior/ios.json"
readonly PRIOR_ANDROID="$STAGING_ROOT/prior/android.json"
if [ "${OXID_PORTAL_LOCAL_LOCK_HELD:-}" != "$LOCK_FILE" ]; then
  exec "$LOCK_RUNNER" "$LOCK_FILE" -- env \
    OXID_PORTAL_LOCAL_LOCK_HELD="$LOCK_FILE" \
    "$REPOSITORY_ROOT/scripts/e2e/portal-local-conformance.sh" "$@"
fi
if "$LOCK_RUNNER" "$LOCK_FILE" -- true >/dev/null 2>&1; then
  printf 'portal-local-conformance: FAIL phase=lock-not-held\n' >&2
  exit 1
fi

EXPECTED_HEAD=""
EXPECTED_BRANCH=""
PRIOR_HEADLESS_PRESENT=0
PRIOR_IOS_PRESENT=0
PRIOR_ANDROID_PRESENT=0
PUBLICATION_STARTED=0
PUBLICATION_COMPLETE=0
PUBLISH_HEADLESS_TEMP=""
PUBLISH_IOS_TEMP=""
PUBLISH_ANDROID_TEMP=""
STACK_STARTED=0

fail() {
  printf 'portal-local-conformance: FAIL phase=%s\n' "$1" >&2
  exit 1
}

assert_repository_state() {
  local head branch tracked
  head="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD 2>/dev/null || true)"
  branch="$(git -C "$REPOSITORY_ROOT" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
  tracked="$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no 2>/dev/null || printf invalid)"
  [ "$head" = "$EXPECTED_HEAD" ] && [ "$branch" = "$EXPECTED_BRANCH" ] && [ -z "$tracked" ] || {
    printf 'portal-local-conformance: FAIL phase=oxid-source-changed\n' >&2
    return 1
  }
}

assert_no_harness_leaks() {
  "$RESOURCE_CHECKER" "$REPOSITORY_ROOT" "$SOURCE_TREE" "${1:-$PORTAL_COMPOSE_PROJECT}" >/dev/null
}

backup_retained() {
  local retained="$1" backup="$2"
  if [ -e "$retained" ] || [ -L "$retained" ]; then
    [ -f "$retained" ] && [ ! -L "$retained" ] || fail retained-evidence-path
    cp -p -- "$retained" "$backup" || fail retained-evidence-backup
    printf '1\n'
  else
    printf '0\n'
  fi
}

restore_retained() {
  local retained="$1" backup="$2" present="$3" candidate
  mkdir -p -- "$(dirname -- "$retained")" || return 1
  if [ "$present" = 1 ]; then
    candidate="$(umask 077 && mktemp "$(dirname -- "$retained")/.evidence.rollback.XXXXXX")" || return 1
    cp -p -- "$backup" "$candidate" || { rm -f -- "$candidate"; return 1; }
    mv -f -- "$candidate" "$retained" || { rm -f -- "$candidate"; return 1; }
  else
    rm -f -- "$retained" || return 1
  fi
}

rollback_publication() {
  local status=0
  restore_retained "$RETAINED_HEADLESS" "$PRIOR_HEADLESS" "$PRIOR_HEADLESS_PRESENT" || status=1
  restore_retained "$RETAINED_IOS" "$PRIOR_IOS" "$PRIOR_IOS_PRESENT" || status=1
  restore_retained "$RETAINED_ANDROID" "$PRIOR_ANDROID" "$PRIOR_ANDROID_PRESENT" || status=1
  return "$status"
}

cleanup() {
  local incoming_status=$? cleanup_status=0
  trap - EXIT
  trap '' INT TERM
  if [ "$STACK_STARTED" = 1 ]; then
    "$LOCAL_HEADLESS" down "$STACK_ENV_PATH" >/dev/null 2>&1 || cleanup_status=1
    STACK_STARTED=0
  fi
  if [[ "$SOURCE_TREE" = /* ]] && [ -d "$SOURCE_TREE" ]; then
    assert_no_harness_leaks || cleanup_status=1
  fi
  if [ "$PUBLICATION_STARTED" = 1 ] && [ "$PUBLICATION_COMPLETE" != 1 ]; then
    rollback_publication || cleanup_status=1
  fi
  rm -f -- "${PUBLISH_HEADLESS_TEMP:-}" "${PUBLISH_IOS_TEMP:-}" "${PUBLISH_ANDROID_TEMP:-}" 2>/dev/null || cleanup_status=1
  rm -rf -- "$STAGING_ROOT" || cleanup_status=1
  if [ "$incoming_status" != 0 ]; then exit "$incoming_status"; fi
  exit "$cleanup_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_step() {
  local label="$1"
  shift
  assert_repository_state || return 1
  "$SOURCE_VALIDATOR" "$SOURCE_TREE" --offline >/dev/null
  printf 'portal-local-conformance: START step=%s head=%s\n' "$label" "$EXPECTED_HEAD"
  "$@"
  assert_repository_state || return 1
  "$SOURCE_VALIDATOR" "$SOURCE_TREE" --offline >/dev/null
  assert_no_harness_leaks || return 1
  printf 'portal-local-conformance: PASS step=%s head=%s\n' "$label" "$EXPECTED_HEAD"
}

publish_evidence_set() {
  mkdir -p -- "$(dirname -- "$RETAINED_HEADLESS")" "$(dirname -- "$RETAINED_IOS")" "$(dirname -- "$RETAINED_ANDROID")"
  PUBLISH_HEADLESS_TEMP="$(umask 077 && mktemp "$(dirname -- "$RETAINED_HEADLESS")/.evidence.publish.XXXXXX")"
  PUBLISH_IOS_TEMP="$(umask 077 && mktemp "$(dirname -- "$RETAINED_IOS")/.evidence.publish.XXXXXX")"
  PUBLISH_ANDROID_TEMP="$(umask 077 && mktemp "$(dirname -- "$RETAINED_ANDROID")/.evidence.publish.XXXXXX")"
  cp -- "$STAGED_HEADLESS" "$PUBLISH_HEADLESS_TEMP"
  cp -- "$STAGED_IOS" "$PUBLISH_IOS_TEMP"
  cp -- "$STAGED_ANDROID" "$PUBLISH_ANDROID_TEMP"
  chmod 600 "$PUBLISH_HEADLESS_TEMP" "$PUBLISH_IOS_TEMP" "$PUBLISH_ANDROID_TEMP"
  PUBLICATION_STARTED=1
  mv -f -- "$PUBLISH_HEADLESS_TEMP" "$RETAINED_HEADLESS"
  PUBLISH_HEADLESS_TEMP=""
  mv -f -- "$PUBLISH_IOS_TEMP" "$RETAINED_IOS"
  PUBLISH_IOS_TEMP=""
  mv -f -- "$PUBLISH_ANDROID_TEMP" "$RETAINED_ANDROID"
  PUBLISH_ANDROID_TEMP=""
  node "$EVIDENCE_TOOL" validate \
    --head "$EXPECTED_HEAD" \
    --headless "$RETAINED_HEADLESS" \
    --ios "$RETAINED_IOS" \
    --android "$RETAINED_ANDROID" >/dev/null
}

cd "$REPOSITORY_ROOT"
for command_name in docker git jq node rg; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"
done
[[ "$SOURCE_TREE" = /* ]] || fail source-path
EXPECTED_HEAD="$(git rev-parse HEAD)"
EXPECTED_BRANCH="$(git symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
[[ "$EXPECTED_HEAD" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head
[ -n "$EXPECTED_BRANCH" ] || fail oxid-branch
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail oxid-tree-dirty

mkdir -p -- "$STAGING_ROOT/evidence" "$STAGING_ROOT/prior"
chmod 700 "$STAGING_ROOT" "$STAGING_ROOT/evidence" "$STAGING_ROOT/prior"
PRIOR_HEADLESS_PRESENT="$(backup_retained "$RETAINED_HEADLESS" "$PRIOR_HEADLESS")"
PRIOR_IOS_PRESENT="$(backup_retained "$RETAINED_IOS" "$PRIOR_IOS")"
PRIOR_ANDROID_PRESENT="$(backup_retained "$RETAINED_ANDROID" "$PRIOR_ANDROID")"

"$SOURCE_VALIDATOR" "$SOURCE_TREE" --offline >/dev/null
"$LOCAL_HEADLESS" up "$STACK_ENV_PATH" >/dev/null
STACK_STARTED=1
assert_repository_state
assert_no_harness_leaks

run_step "headless" env \
  STACK_ENV_FILE="$STACK_ENV_PATH" \
  OXID_PORTAL_KEEP_FAILURE_LOG=0 \
  OXID_PORTAL_EVIDENCE_PATH="$STAGED_HEADLESS" \
  "$REPOSITORY_ROOT/scripts/e2e/portal-headless-e2e.sh" "$STACK_ENV_PATH"
run_step "ios-portal" env \
  STACK_ENV_FILE="$STACK_ENV_PATH" \
  OXID_PORTAL_KEEP_FAILURE_LOG=0 \
  OXID_PORTAL_IOS_EVIDENCE_PATH="$STAGED_IOS" \
  "$REPOSITORY_ROOT/scripts/test-ios-portal-flow.sh"
run_step "ios-standard" "$REPOSITORY_ROOT/scripts/test-ios-profile-flow.sh"
node "$EVIDENCE_TOOL" attest-standard-smoke \
  --platform ios --evidence "$STAGED_IOS" --head "$EXPECTED_HEAD" >/dev/null
assert_repository_state
run_step "android-portal" env \
  STACK_ENV_FILE="$STACK_ENV_PATH" \
  OXID_PORTAL_KEEP_FAILURE_LOG=0 \
  OXID_PORTAL_ANDROID_EVIDENCE_PATH="$STAGED_ANDROID" \
  "$REPOSITORY_ROOT/scripts/test-android-portal-flow.sh"
run_step "android-standard" "$REPOSITORY_ROOT/scripts/test-android-profile-flow.sh"
node "$EVIDENCE_TOOL" attest-standard-smoke \
  --platform android --evidence "$STAGED_ANDROID" --head "$EXPECTED_HEAD" >/dev/null
assert_repository_state

node "$EVIDENCE_TOOL" validate \
  --head "$EXPECTED_HEAD" \
  --headless "$STAGED_HEADLESS" \
  --ios "$STAGED_IOS" \
  --android "$STAGED_ANDROID" >/dev/null
"$LOCAL_HEADLESS" down "$STACK_ENV_PATH" >/dev/null
STACK_STARTED=0
assert_no_harness_leaks
publish_evidence_set
assert_repository_state
assert_no_harness_leaks
PUBLICATION_COMPLETE=1
printf 'portal-local-conformance: PASS head=%s evidence=headless,ios,android\n' "$EXPECTED_HEAD"
