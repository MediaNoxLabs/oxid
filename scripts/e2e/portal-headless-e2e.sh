#!/usr/bin/env bash
set -euo pipefail

readonly EXPECTED_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly INTEGRATION_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly INTEGRATION_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly PROVENANCE_SHA="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly SOURCE_TREE="${PORTAL_SOURCE_TREE:-}"
readonly EVIDENCE="${OXID_PORTAL_EVIDENCE_PATH:-$REPO_ROOT/target/portal-headless-e2e/evidence.json}"
readonly RUN_TREE="${TMPDIR:-/tmp}/oxid-portal-integration-${INTEGRATION_COMMIT:0:8}-$$"
readonly RAW_LOG="${TMPDIR:-/tmp}/oxid-portal-headless-e2e-$$.log"
export COMPOSE_PROJECT_NAME="oxidportal124$$"

stack_started=0
worktree_created=0
cleanup() {
  status=$?
  if [[ "$stack_started" == 1 && -d "$RUN_TREE" ]]; then
    (cd "$RUN_TREE" && just compose-down) >>"$RAW_LOG" 2>&1 || status=1
  fi
  if [[ "$worktree_created" == 1 ]]; then
    git -C "$SOURCE_TREE" worktree remove --force "$RUN_TREE" >>"$RAW_LOG" 2>&1 || status=1
  fi
  if [[ "$status" != 0 && "${OXID_PORTAL_KEEP_FAILURE_LOG:-0}" == 1 ]]; then
    chmod 600 "$RAW_LOG" 2>/dev/null || true
    printf 'portal-headless-e2e: private failure log=%s\n' "$RAW_LOG" >&2
  else
    rm -f "$RAW_LOG"
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

fail() {
  printf 'portal-headless-e2e: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[[ "$SOURCE_TREE" = /* && -d "$SOURCE_TREE" ]] || fail source-path
[[ "$(git -C "$SOURCE_TREE" remote get-url origin 2>/dev/null)" == "$EXPECTED_REMOTE" ]] || fail source-remote
[[ -z "$(git -C "$SOURCE_TREE" status --porcelain 2>/dev/null)" ]] || fail source-dirty
command -v nix >/dev/null 2>&1 || fail missing-nix
command -v docker >/dev/null 2>&1 || fail missing-docker
command -v just >/dev/null 2>&1 || fail missing-just
command -v jq >/dev/null 2>&1 || fail missing-jq
command -v rg >/dev/null 2>&1 || fail missing-rg
docker info >/dev/null 2>&1 || fail docker-daemon

if ! git -C "$SOURCE_TREE" fetch origin \
  "+$INTEGRATION_COMMIT:refs/oxid-evidence/portal-integration" \
  "refs/pull/17/head:refs/oxid-evidence/portal-pr-17" >>"$RAW_LOG" 2>&1; then
  fail source-fetch
fi
[[ "$(git -C "$SOURCE_TREE" rev-parse refs/oxid-evidence/portal-integration^{commit})" == "$INTEGRATION_COMMIT" ]] || fail integration-commit
[[ "$(git -C "$SOURCE_TREE" rev-parse refs/oxid-evidence/portal-integration^{tree})" == "$INTEGRATION_TREE" ]] || fail integration-tree
[[ "$(git -C "$SOURCE_TREE" rev-parse refs/oxid-evidence/portal-pr-17^{commit})" == "$PR_HEAD" ]] || fail pr-head
[[ "$(git -C "$SOURCE_TREE" rev-parse refs/oxid-evidence/portal-pr-17^{tree})" == "$INTEGRATION_TREE" ]] || fail pr-tree
[[ "$(git -C "$SOURCE_TREE" rev-parse "$PROFILE_SOURCE"^{commit})" == "$PROFILE_SOURCE" ]] || fail profile-source
[[ "$(git -C "$SOURCE_TREE" show "$INTEGRATION_COMMIT:crates/issuer-integration/fixtures/openid4vci-final/provenance.json" | shasum -a 256 | awk '{print $1}')" == "$PROVENANCE_SHA" ]] || fail provenance

if ! git -C "$SOURCE_TREE" worktree add --detach "$RUN_TREE" "$INTEGRATION_COMMIT" >>"$RAW_LOG" 2>&1; then
  fail integration-checkout
fi
worktree_created=1
[[ -z "$(git -C "$RUN_TREE" status --porcelain)" ]] || fail integration-checkout-dirty

readonly OXID_HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
[[ -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)" ]] || fail oxid-tree-dirty
rm -f "$EVIDENCE"
stack_started=1
if ! (cd "$RUN_TREE" && just compose-up) >>"$RAW_LOG" 2>&1; then
  fail portal-compose-up
fi

if ! PORTAL_INTEGRATION_TREE="$RUN_TREE" \
  OXID_PORTAL_EVIDENCE_PATH="$EVIDENCE" \
  OXID_PORTAL_EVIDENCE_HEAD="$OXID_HEAD" \
  cargo test --manifest-path "$REPO_ROOT/Cargo.toml" -p oxid-headless \
    --test portal_live_flow \
    landed_portal_service_issues_to_headless_and_restores_in_new_process \
    -- --ignored --exact >>"$RAW_LOG" 2>&1; then
  fail live-flow
fi

[[ -f "$EVIDENCE" ]] || fail missing-evidence
[[ -z "$(git -C "$RUN_TREE" status --porcelain)" ]] || fail portal-tree-mutated
"$REPO_ROOT/scripts/e2e/validate-portal-headless-evidence.sh" "$EVIDENCE" "$OXID_HEAD" \
  >>"$RAW_LOG" 2>&1 || fail evidence-schema
printf 'portal-headless-e2e: PASS evidence=%s\n' "${EVIDENCE#"$REPO_ROOT/"}"
