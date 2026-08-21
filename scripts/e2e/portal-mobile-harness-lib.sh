# SPDX-License-Identifier: Apache-2.0
# Shared, source-only orchestration for the sequential iOS/Android Portal suites.

readonly PORTAL_EXPECTED_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_INTEGRATION_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly PORTAL_INTEGRATION_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly PORTAL_PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly PORTAL_PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"

PORTAL_MOBILE_SUPPORT_PID=""
PORTAL_MOBILE_HOLDER_SYNC_PID=""
PORTAL_MOBILE_RUN_TREE=""
PORTAL_MOBILE_STATE_DIR=""
PORTAL_MOBILE_CONTROL_ORIGIN=""
PORTAL_MOBILE_MANIFEST_PATH=""
PORTAL_MOBILE_MANIFEST_SHA256=""
PORTAL_MOBILE_PRIVATE_LOG=""
PORTAL_MOBILE_PLATFORM=""
PORTAL_MOBILE_CLEANUP_RUNNING=0

portal_mobile_fail() {
  printf 'portal-mobile-%s: FAIL phase=%s\n' "${PORTAL_MOBILE_PLATFORM:-unknown}" "$1" >&2
  return 1
}

portal_mobile_source_tree() {
  local git_common repo_parent candidate
  git_common="$(git rev-parse --path-format=absolute --git-common-dir)"
  repo_parent="$(dirname -- "$(dirname -- "$git_common")")"
  for candidate in \
    "${PORTAL_SOURCE_TREE:-}" \
    "$repo_parent/lace-id-portal/tmp/worktrees/dev-loops/issue-16" \
    "$repo_parent/lace-id-portal"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ] && \
      [ "$(git -C "$candidate" remote get-url origin 2>/dev/null || true)" = "$PORTAL_EXPECTED_REMOTE" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

portal_mobile_start() {
  PORTAL_MOBILE_PLATFORM="$1"
  case "$PORTAL_MOBILE_PLATFORM" in ios|android) ;; *) portal_mobile_fail platform; return 1 ;; esac

  local repository_root source_tree ready_fifo ready_status oxid_head
  repository_root="$(git rev-parse --show-toplevel)"
  source_tree="$(portal_mobile_source_tree)" || { portal_mobile_fail source-path; return 1; }
  [ -z "$(git -C "$source_tree" status --porcelain --untracked-files=no)" ] || {
    portal_mobile_fail source-dirty
    return 1
  }
  [ -z "$(git status --porcelain --untracked-files=no)" ] || {
    portal_mobile_fail oxid-tree-dirty
    return 1
  }
  oxid_head="$(git rev-parse HEAD)"
  [[ "$oxid_head" =~ ^[0-9a-f]{40}$ ]] || { portal_mobile_fail oxid-head; return 1; }

  PORTAL_MOBILE_STATE_DIR="$repository_root/target/portal-mobile-e2e/$PORTAL_MOBILE_PLATFORM/runtime"
  rm -rf "$PORTAL_MOBILE_STATE_DIR"
  mkdir -p "$PORTAL_MOBILE_STATE_DIR"
  chmod 700 "$PORTAL_MOBILE_STATE_DIR"
  PORTAL_MOBILE_PRIVATE_LOG="$PORTAL_MOBILE_STATE_DIR/orchestrator-private.log"
  : >"$PORTAL_MOBILE_PRIVATE_LOG"
  chmod 600 "$PORTAL_MOBILE_PRIVATE_LOG"

  if ! git -C "$source_tree" fetch origin \
    "integration:refs/remotes/origin/integration" \
    "refs/pull/17/head:refs/oxid-evidence/portal-pr-17" \
    >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1; then
    portal_mobile_fail source-fetch
    return 1
  fi
  [ "$(git -C "$source_tree" rev-parse origin/integration^{commit})" = "$PORTAL_INTEGRATION_COMMIT" ] || {
    portal_mobile_fail integration-commit
    return 1
  }
  [ "$(git -C "$source_tree" rev-parse origin/integration^{tree})" = "$PORTAL_INTEGRATION_TREE" ] || {
    portal_mobile_fail integration-tree
    return 1
  }
  [ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-pr-17^{commit})" = "$PORTAL_PR_HEAD" ] || {
    portal_mobile_fail pr-head
    return 1
  }
  [ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-pr-17^{tree})" = "$PORTAL_INTEGRATION_TREE" ] || {
    portal_mobile_fail pr-tree
    return 1
  }
  [ "$(git -C "$source_tree" rev-parse "$PORTAL_PROFILE_SOURCE"^{commit})" = "$PORTAL_PROFILE_SOURCE" ] || {
    portal_mobile_fail profile-source
    return 1
  }
  [ "$(git -C "$source_tree" show "$PORTAL_INTEGRATION_COMMIT:crates/issuer-integration/fixtures/openid4vci-final/provenance.json" | shasum -a 256 | awk '{print $1}')" = "$PORTAL_PROVENANCE_SHA256" ] || {
    portal_mobile_fail provenance
    return 1
  }

  PORTAL_MOBILE_RUN_TREE="${TMPDIR:-/tmp}/oxid-portal-mobile-${PORTAL_MOBILE_PLATFORM}-${PORTAL_INTEGRATION_COMMIT:0:8}-$$"
  if ! git -C "$source_tree" worktree add --detach "$PORTAL_MOBILE_RUN_TREE" "$PORTAL_INTEGRATION_COMMIT" \
    >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1; then
    portal_mobile_fail source-checkout
    return 1
  fi
  [ -z "$(git -C "$PORTAL_MOBILE_RUN_TREE" status --porcelain)" ] || {
    portal_mobile_fail source-checkout-dirty
    return 1
  }

  ready_fifo="$PORTAL_MOBILE_STATE_DIR/ready.fifo"
  mkfifo "$ready_fifo"
  chmod 600 "$ready_fifo"
  COMPOSE_PROJECT_NAME="oxidportal124${PORTAL_MOBILE_PLATFORM}$$" \
  PORTAL_INTEGRATION_TREE="$PORTAL_MOBILE_RUN_TREE" \
  OXID_PORTAL_MOBILE_STATE_DIR="$PORTAL_MOBILE_STATE_DIR" \
  OXID_PORTAL_MOBILE_READY_FIFO="$ready_fifo" \
    node "$repository_root/scripts/e2e/portal-mobile-support.mjs" \
      >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 &
  PORTAL_MOBILE_SUPPORT_PID=$!
  IFS= read -r ready_status <"$ready_fifo"
  rm -f "$ready_fifo"
  [ "$ready_status" = "READY" ] || {
    portal_mobile_fail "${ready_status#FAIL:}"
    return 1
  }
  kill -0 "$PORTAL_MOBILE_SUPPORT_PID" 2>/dev/null || {
    portal_mobile_fail support-exited
    return 1
  }

  local ready="$PORTAL_MOBILE_STATE_DIR/ready.json"
  PORTAL_MOBILE_CONTROL_ORIGIN="$(jq -r '.controlOrigin // empty' "$ready")"
  PORTAL_MOBILE_MANIFEST_PATH="$(jq -r '.manifestPath // empty' "$ready")"
  PORTAL_MOBILE_MANIFEST_SHA256="$(jq -r '.manifestSha256 // empty' "$ready")"
  [ "$PORTAL_MOBILE_CONTROL_ORIGIN" = "http://127.0.0.1:18091" ] || {
    portal_mobile_fail control-origin
    return 1
  }
  [ -f "$PORTAL_MOBILE_MANIFEST_PATH" ] && \
    [[ "$PORTAL_MOBILE_MANIFEST_SHA256" =~ ^[0-9a-f]{64}$ ]] && \
    [ "$(shasum -a 256 "$PORTAL_MOBILE_MANIFEST_PATH" | awk '{print $1}')" = "$PORTAL_MOBILE_MANIFEST_SHA256" ] || {
    portal_mobile_fail manifest
    return 1
  }

  export OXID_MOBILE_PORTAL_PROFILE=local
  export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$PORTAL_MOBILE_MANIFEST_PATH"
  export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$PORTAL_MOBILE_MANIFEST_SHA256"
  trap 'portal_mobile_cleanup' EXIT INT TERM
}

portal_mobile_finish() {
  local result=0
  if [ -n "$PORTAL_MOBILE_SUPPORT_PID" ]; then
    curl --noproxy '*' --fail --silent --show-error -X POST \
      "$PORTAL_MOBILE_CONTROL_ORIGIN/complete" >/dev/null || result=1
    wait "$PORTAL_MOBILE_SUPPORT_PID" || result=1
    PORTAL_MOBILE_SUPPORT_PID=""
  fi
  return "$result"
}

portal_mobile_cleanup() {
  local incoming_status=$? cleanup_status=0 source_tree
  if [ "$PORTAL_MOBILE_CLEANUP_RUNNING" = 1 ]; then
    return "$incoming_status"
  fi
  PORTAL_MOBILE_CLEANUP_RUNNING=1
  trap - EXIT INT TERM
  if [ -n "$PORTAL_MOBILE_HOLDER_SYNC_PID" ]; then
    kill "$PORTAL_MOBILE_HOLDER_SYNC_PID" >/dev/null 2>&1 || true
    wait "$PORTAL_MOBILE_HOLDER_SYNC_PID" >/dev/null 2>&1 || true
    PORTAL_MOBILE_HOLDER_SYNC_PID=""
  fi
  if [ -n "$PORTAL_MOBILE_SUPPORT_PID" ]; then
    curl --noproxy '*' --silent -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
    wait "$PORTAL_MOBILE_SUPPORT_PID" >/dev/null 2>&1 || cleanup_status=1
    PORTAL_MOBILE_SUPPORT_PID=""
  fi
  if [ -n "$PORTAL_MOBILE_RUN_TREE" ]; then
    source_tree="$(portal_mobile_source_tree || true)"
    if [ -n "$source_tree" ]; then
      git -C "$source_tree" worktree remove --force "$PORTAL_MOBILE_RUN_TREE" \
        >>"${PORTAL_MOBILE_PRIVATE_LOG:-/dev/null}" 2>&1 || cleanup_status=1
    fi
    PORTAL_MOBILE_RUN_TREE=""
  fi
  if [ "$incoming_status" = 0 ] && [ "$cleanup_status" = 0 ] && [ -n "$PORTAL_MOBILE_STATE_DIR" ]; then
    rm -rf "$PORTAL_MOBILE_STATE_DIR"
  elif [ -n "$PORTAL_MOBILE_STATE_DIR" ]; then
    printf 'portal-mobile-%s: private failure artifacts=%s\n' "$PORTAL_MOBILE_PLATFORM" "$PORTAL_MOBILE_STATE_DIR" >&2
  fi
  if [ "$incoming_status" != 0 ]; then return "$incoming_status"; fi
  return "$cleanup_status"
}
