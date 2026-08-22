# SPDX-License-Identifier: Apache-2.0
# Shared, source-only orchestration for the sequential iOS/Android Portal suites.

readonly PORTAL_EXPECTED_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_INTEGRATION_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly PORTAL_INTEGRATION_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly PORTAL_PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly PORTAL_PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"

readonly PORTAL_MOBILE_READY_TIMEOUT_SECONDS=720
readonly PORTAL_MOBILE_CURL_TIMEOUT_SECONDS=10
readonly PORTAL_MOBILE_SUPPORT_GRACE_SECONDS=75
readonly PORTAL_MOBILE_STARTUP_GRACE_SECONDS=675
readonly PORTAL_MOBILE_TERM_GRACE_SECONDS=5

PORTAL_MOBILE_SUPPORT_PID=""
PORTAL_MOBILE_HOLDER_SYNC_PID=""
PORTAL_MOBILE_RUN_TREE=""
PORTAL_MOBILE_STATE_DIR=""
PORTAL_MOBILE_CONTROL_ORIGIN=""
PORTAL_MOBILE_MANIFEST_PATH=""
PORTAL_MOBILE_MANIFEST_SHA256=""
PORTAL_MOBILE_PRIVATE_LOG=""
PORTAL_MOBILE_PLATFORM=""
PORTAL_MOBILE_REPOSITORY_ROOT=""
PORTAL_MOBILE_OXID_HEAD=""
PORTAL_MOBILE_LOCK_DIR=""
PORTAL_MOBILE_LOCK_OWNED=0
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

portal_mobile_wait_bounded() {
  local child_pid="$1" grace_seconds="$2" watchdog_pid wait_status
  (
    sleep "$grace_seconds"
    kill -TERM "$child_pid" >/dev/null 2>&1 || exit 0
    sleep "$PORTAL_MOBILE_TERM_GRACE_SECONDS"
    kill -KILL "$child_pid" >/dev/null 2>&1 || true
  ) &
  watchdog_pid=$!
  if wait "$child_pid"; then wait_status=0; else wait_status=$?; fi
  kill "$watchdog_pid" >/dev/null 2>&1 || true
  wait "$watchdog_pid" >/dev/null 2>&1 || true
  return "$wait_status"
}

portal_mobile_acquire_lock() {
  local attempt owner="" stale_lock
  PORTAL_MOBILE_LOCK_DIR="/tmp/oxid-portal-mobile-$(id -u).lock"
  for attempt in 1 2 3; do
    if (umask 077 && mkdir "$PORTAL_MOBILE_LOCK_DIR") 2>/dev/null; then
      if ! (umask 077 && printf '%s\n' "$$" >"$PORTAL_MOBILE_LOCK_DIR/owner-pid"); then
        rm -rf "$PORTAL_MOBILE_LOCK_DIR"
        portal_mobile_fail lock-owner
        return 1
      fi
      PORTAL_MOBILE_LOCK_OWNED=1
      return 0
    fi
    [ -d "$PORTAL_MOBILE_LOCK_DIR" ] && [ ! -L "$PORTAL_MOBILE_LOCK_DIR" ] || {
      portal_mobile_fail lock-path
      return 1
    }
    owner=""
    for _owner_attempt in 1 2 3 4 5 6 7 8 9 10; do
      if IFS= read -r owner <"$PORTAL_MOBILE_LOCK_DIR/owner-pid" 2>/dev/null; then break; fi
      sleep 0.1
    done
    if [[ "$owner" =~ ^[0-9]+$ ]] && kill -0 "$owner" 2>/dev/null; then
      portal_mobile_fail lock-busy
      return 1
    fi
    # Rename the exact stale lock before removal. Concurrent contenders can
    # race to rename it, but can never remove a replacement lock acquired by
    # another invocation.
    stale_lock="${PORTAL_MOBILE_LOCK_DIR}.stale.$$.$attempt"
    if mv "$PORTAL_MOBILE_LOCK_DIR" "$stale_lock" 2>/dev/null; then
      rm -rf "$stale_lock"
    fi
  done
  portal_mobile_fail lock-busy
  return 1
}

portal_mobile_assert_evidence_source() {
  local current_head tracked_status
  current_head="$(git -C "$PORTAL_MOBILE_REPOSITORY_ROOT" rev-parse HEAD 2>/dev/null || true)"
  tracked_status="$(git -C "$PORTAL_MOBILE_REPOSITORY_ROOT" status --porcelain --untracked-files=no 2>/dev/null || printf invalid)"
  [ -n "$PORTAL_MOBILE_OXID_HEAD" ] && \
    [ "$current_head" = "$PORTAL_MOBILE_OXID_HEAD" ] && \
    [ -z "$tracked_status" ] || {
    portal_mobile_fail oxid-source-changed
    return 1
  }
}

portal_mobile_start() {
  # Install cleanup before any side effect (lock, state dir, fetch, worktree,
  # support process, FIFO, compose stack, manifest) so a startup failure at
  # any later line still tears down whatever was already created. Signals use
  # their conventional statuses and cannot resume interrupted orchestration;
  # EXIT remains the single cleanup owner.
  trap 'portal_mobile_cleanup' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  PORTAL_MOBILE_PLATFORM="$1"
  case "$PORTAL_MOBILE_PLATFORM" in ios|android) ;; *) portal_mobile_fail platform; return 1 ;; esac
  portal_mobile_acquire_lock || return 1

  local source_tree ready_fifo ready_status=""
  PORTAL_MOBILE_REPOSITORY_ROOT="$(git rev-parse --show-toplevel)"
  source_tree="$(portal_mobile_source_tree)" || { portal_mobile_fail source-path; return 1; }
  [ -z "$(git -C "$source_tree" status --porcelain --untracked-files=no)" ] || {
    portal_mobile_fail source-dirty
    return 1
  }
  [ -z "$(git status --porcelain --untracked-files=no)" ] || {
    portal_mobile_fail oxid-tree-dirty
    return 1
  }
  PORTAL_MOBILE_OXID_HEAD="$(git rev-parse HEAD)"
  [[ "$PORTAL_MOBILE_OXID_HEAD" =~ ^[0-9a-f]{40}$ ]] || { portal_mobile_fail oxid-head; return 1; }

  PORTAL_MOBILE_STATE_DIR="$PORTAL_MOBILE_REPOSITORY_ROOT/target/portal-mobile-e2e/$PORTAL_MOBILE_PLATFORM/runtime"
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
  # Open both ends before spawning support so neither side can block in open(2).
  # The read itself is bounded for the full compose/issuer readiness window.
  exec 9<>"$ready_fifo"
  COMPOSE_PROJECT_NAME="oxidportal124${PORTAL_MOBILE_PLATFORM}$$" \
  PORTAL_INTEGRATION_CHECKOUT="$PORTAL_MOBILE_RUN_TREE" \
  OXID_PORTAL_MOBILE_STATE_DIR="$PORTAL_MOBILE_STATE_DIR" \
  OXID_PORTAL_MOBILE_READY_FIFO="$ready_fifo" \
    node "$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/e2e/portal-mobile-support.mjs" \
      >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 &
  PORTAL_MOBILE_SUPPORT_PID=$!
  if ! IFS= read -r -t "$PORTAL_MOBILE_READY_TIMEOUT_SECONDS" -u 9 ready_status; then
    exec 9>&-
    rm -f "$ready_fifo"
    portal_mobile_fail support-ready-timeout
    return 1
  fi
  exec 9>&-
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
}

portal_mobile_finish() {
  local result=0
  if [ -n "$PORTAL_MOBILE_SUPPORT_PID" ]; then
    curl --noproxy '*' --fail --silent --show-error \
      --connect-timeout "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
      --max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
      -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/complete" >/dev/null || result=1
    portal_mobile_wait_bounded \
      "$PORTAL_MOBILE_SUPPORT_PID" "$PORTAL_MOBILE_SUPPORT_GRACE_SECONDS" || result=1
    PORTAL_MOBILE_SUPPORT_PID=""
  fi
  return "$result"
}

portal_mobile_cleanup() {
  # Capture the trap-triggering status before any cleanup command can replace
  # it. Runtime manifests and logs can contain private protocol diagnostics,
  # so they are removed on success, failure, and signal exits alike.
  local incoming_status="$?" cleanup_status=0 source_tree lock_owner=""
  local support_grace="$PORTAL_MOBILE_SUPPORT_GRACE_SECONDS"
  if [ "$PORTAL_MOBILE_CLEANUP_RUNNING" = 1 ]; then
    return "$incoming_status"
  fi
  PORTAL_MOBILE_CLEANUP_RUNNING=1
  trap - EXIT
  trap '' INT TERM
  if declare -F portal_mobile_platform_cleanup >/dev/null 2>&1; then
    portal_mobile_platform_cleanup || cleanup_status=1
  fi
  if [ -n "$PORTAL_MOBILE_HOLDER_SYNC_PID" ]; then
    kill -TERM "$PORTAL_MOBILE_HOLDER_SYNC_PID" >/dev/null 2>&1 || true
    portal_mobile_wait_bounded "$PORTAL_MOBILE_HOLDER_SYNC_PID" \
      "$PORTAL_MOBILE_TERM_GRACE_SECONDS" >/dev/null 2>&1 || true
    PORTAL_MOBILE_HOLDER_SYNC_PID=""
  fi
  if [ -n "$PORTAL_MOBILE_SUPPORT_PID" ]; then
    # Before READY, support can be inside a bounded synchronous compose command;
    # let that command hit its own timeout and run compose-down rather than
    # killing the cleanup owner early and orphaning its child process.
    if [ -z "$PORTAL_MOBILE_CONTROL_ORIGIN" ]; then
      support_grace="$PORTAL_MOBILE_STARTUP_GRACE_SECONDS"
    fi
    curl --noproxy '*' --silent \
      --connect-timeout "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
      --max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
      -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
    portal_mobile_wait_bounded \
      "$PORTAL_MOBILE_SUPPORT_PID" "$support_grace" \
      >/dev/null 2>&1 || cleanup_status=1
    PORTAL_MOBILE_SUPPORT_PID=""
  fi
  if [ -n "$PORTAL_MOBILE_RUN_TREE" ]; then
    source_tree="$(portal_mobile_source_tree || true)"
    if [ -n "$source_tree" ]; then
      git -C "$source_tree" worktree remove --force "$PORTAL_MOBILE_RUN_TREE" \
        >>"${PORTAL_MOBILE_PRIVATE_LOG:-/dev/null}" 2>&1 || cleanup_status=1
    else
      cleanup_status=1
    fi
    PORTAL_MOBILE_RUN_TREE=""
  fi
  if [ -n "$PORTAL_MOBILE_STATE_DIR" ]; then
    rm -rf "$PORTAL_MOBILE_STATE_DIR" || cleanup_status=1
  fi
  if [ "$PORTAL_MOBILE_LOCK_OWNED" = 1 ] && [ -n "$PORTAL_MOBILE_LOCK_DIR" ]; then
    IFS= read -r lock_owner <"$PORTAL_MOBILE_LOCK_DIR/owner-pid" 2>/dev/null || true
    if [ "$lock_owner" = "$$" ]; then
      rm -rf "$PORTAL_MOBILE_LOCK_DIR" || cleanup_status=1
    else
      cleanup_status=1
    fi
    PORTAL_MOBILE_LOCK_OWNED=0
  fi
  if [ "$incoming_status" != 0 ]; then return "$incoming_status"; fi
  return "$cleanup_status"
}
