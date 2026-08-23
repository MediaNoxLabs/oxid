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
# TERM reaches pre-READY support immediately. KILL then waits for the longest
# ten-minute synchronous startup command, 60-second Compose teardown,
# five-second named-resource poll, and a ten-second scheduling margin.
readonly PORTAL_MOBILE_STARTUP_GRACE_SECONDS=675
readonly PORTAL_MOBILE_ADB_OPERATION_TIMEOUT_SECONDS=10
readonly PORTAL_MOBILE_ADB_KILL_GRACE_SECONDS=2
readonly PORTAL_MOBILE_ADB_BOOT_DEADLINE_SECONDS=120
readonly PORTAL_MOBILE_TERM_GRACE_SECONDS=5
readonly PORTAL_MOBILE_OFFER_CAPABILITY_BYTES=64

PORTAL_MOBILE_SUPPORT_PID=""
PORTAL_MOBILE_HOLDER_SYNC_PID=""
PORTAL_MOBILE_RUN_TREE=""
PORTAL_MOBILE_STATE_DIR=""
PORTAL_MOBILE_CONTROL_ORIGIN=""
PORTAL_MOBILE_MANIFEST_PATH=""
PORTAL_MOBILE_MANIFEST_SHA256=""
PORTAL_MOBILE_CAPABILITY_FIFO=""
PORTAL_MOBILE_CAPABILITY_FD_OPEN=0
PORTAL_MOBILE_PRIVATE_LOG=""
PORTAL_MOBILE_EVIDENCE_TEMP=""
PORTAL_MOBILE_PLATFORM=""
PORTAL_MOBILE_REPOSITORY_ROOT=""
PORTAL_MOBILE_OXID_HEAD=""
PORTAL_MOBILE_LOCK_DIR=""
PORTAL_MOBILE_LOCK_OWNED=0
PORTAL_MOBILE_RECLAIM_DIR=""
PORTAL_MOBILE_RECLAIM_OWNED=0
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
  local child_pid="$1" grace_seconds="$2"
  local term_grace_seconds="${3:-$PORTAL_MOBILE_TERM_GRACE_SECONDS}"
  local watchdog_pid wait_status
  (
    sleep "$grace_seconds"
    kill -TERM "$child_pid" >/dev/null 2>&1 || exit 0
    sleep "$term_grace_seconds"
    kill -KILL "$child_pid" >/dev/null 2>&1 || true
  ) &
  watchdog_pid=$!
  if wait "$child_pid"; then wait_status=0; else wait_status=$?; fi
  kill "$watchdog_pid" >/dev/null 2>&1 || true
  wait "$watchdog_pid" >/dev/null 2>&1 || true
  return "$wait_status"
}

portal_mobile_run_captured_bounded() {
  local output_path="$1" total_timeout_seconds="$2" term_grace_seconds="$3"
  shift 3
  local child_pid wait_status=0 term_after_seconds
  if [ "$term_grace_seconds" -gt "$total_timeout_seconds" ]; then
    term_grace_seconds=$total_timeout_seconds
  fi
  term_after_seconds=$((total_timeout_seconds - term_grace_seconds))
  [ -n "$PORTAL_MOBILE_STATE_DIR" ] && \
    [ "$(dirname -- "$output_path")" = "$PORTAL_MOBILE_STATE_DIR" ] && \
    [ -d "$PORTAL_MOBILE_STATE_DIR" ] && [ ! -L "$PORTAL_MOBILE_STATE_DIR" ] || {
    portal_mobile_fail bounded-output
    return 1
  }
  rm -f -- "$output_path" || return 1
  (umask 077 && : >"$output_path") || return 1
  chmod 600 "$output_path" || return 1
  "$@" >"$output_path" 2>>"$PORTAL_MOBILE_PRIVATE_LOG" &
  child_pid=$!
  portal_mobile_wait_bounded \
    "$child_pid" "$term_after_seconds" "$term_grace_seconds" || wait_status=$?
  return "$wait_status"
}

portal_mobile_terminate_bounded() {
  local child_pid="$1" kill_grace_seconds="$2"
  local watchdog_pid wait_status
  # Signal before starting the grace timer so support cannot begin another
  # synchronous child during an unsignaled pre-READY cleanup window.
  kill -TERM "$child_pid" >/dev/null 2>&1 || true
  (
    sleep "$kill_grace_seconds"
    kill -KILL "$child_pid" >/dev/null 2>&1 || true
  ) &
  watchdog_pid=$!
  if wait "$child_pid"; then wait_status=0; else wait_status=$?; fi
  kill "$watchdog_pid" >/dev/null 2>&1 || true
  wait "$watchdog_pid" >/dev/null 2>&1 || true
  return "$wait_status"
}

portal_mobile_release_reclaim_claim() {
  local claim_owner=""
  if [ "$PORTAL_MOBILE_RECLAIM_OWNED" != 1 ] || [ -z "$PORTAL_MOBILE_RECLAIM_DIR" ]; then
    return 0
  fi
  IFS= read -r claim_owner <"$PORTAL_MOBILE_RECLAIM_DIR/owner-pid" 2>/dev/null || true
  if [ "$claim_owner" != "$$" ]; then
    return 1
  fi
  rm -rf -- "$PORTAL_MOBILE_RECLAIM_DIR" || return 1
  PORTAL_MOBILE_RECLAIM_OWNED=0
  return 0
}

portal_mobile_acquire_lock() {
  local attempt owner="" revalidated_owner="" stale_lock
  PORTAL_MOBILE_LOCK_DIR="/tmp/oxid-portal-mobile-$(id -u).lock"
  PORTAL_MOBILE_RECLAIM_DIR="${PORTAL_MOBILE_LOCK_DIR}.reclaim"
  for attempt in 1 2 3; do
    # A reclaim claim closes both sides of the rename race. Normal acquisition
    # checks before mkdir and again after publishing ownership; an ambiguous or
    # abandoned claim is deliberately never reclaimed automatically.
    if [ -e "$PORTAL_MOBILE_RECLAIM_DIR" ] || [ -L "$PORTAL_MOBILE_RECLAIM_DIR" ]; then
      portal_mobile_fail lock-busy
      return 1
    fi
    if (umask 077 && mkdir "$PORTAL_MOBILE_LOCK_DIR") 2>/dev/null; then
      if ! (umask 077 && printf '%s\n' "$$" >"$PORTAL_MOBILE_LOCK_DIR/owner-pid"); then
        rm -rf "$PORTAL_MOBILE_LOCK_DIR"
        portal_mobile_fail lock-owner
        return 1
      fi
      if [ -e "$PORTAL_MOBILE_RECLAIM_DIR" ] || [ -L "$PORTAL_MOBILE_RECLAIM_DIR" ]; then
        owner=""
        IFS= read -r owner <"$PORTAL_MOBILE_LOCK_DIR/owner-pid" 2>/dev/null || true
        [ "$owner" != "$$" ] || rm -rf -- "$PORTAL_MOBILE_LOCK_DIR"
        portal_mobile_fail lock-busy
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
    # A creator can be descheduled between mkdir and its owner-pid write.
    # Missing, unreadable, or partial ownership is therefore busy rather than
    # stale; only a complete numeric owner that no longer exists is reclaimable.
    if ! [[ "$owner" =~ ^[0-9]+$ ]] || kill -0 "$owner" 2>/dev/null; then
      portal_mobile_fail lock-busy
      return 1
    fi
    if ! (umask 077 && mkdir "$PORTAL_MOBILE_RECLAIM_DIR") 2>/dev/null; then
      portal_mobile_fail lock-busy
      return 1
    fi
    PORTAL_MOBILE_RECLAIM_OWNED=1
    if ! (umask 077 && printf '%s\n' "$$" >"$PORTAL_MOBILE_RECLAIM_DIR/owner-pid"); then
      rm -rf -- "$PORTAL_MOBILE_RECLAIM_DIR"
      PORTAL_MOBILE_RECLAIM_OWNED=0
      portal_mobile_fail lock-reclaim
      return 1
    fi

    # Re-read and validate the same dead owner while holding the atomic claim.
    # Any replacement, partial write, liveness change, or path ambiguity fails
    # closed without renaming the lock pathname.
    revalidated_owner=""
    if [ ! -d "$PORTAL_MOBILE_LOCK_DIR" ] || [ -L "$PORTAL_MOBILE_LOCK_DIR" ] || \
      ! IFS= read -r revalidated_owner <"$PORTAL_MOBILE_LOCK_DIR/owner-pid" 2>/dev/null || \
      [ "$revalidated_owner" != "$owner" ] || \
      ! [[ "$revalidated_owner" =~ ^[0-9]+$ ]] || \
      kill -0 "$revalidated_owner" 2>/dev/null; then
      portal_mobile_release_reclaim_claim || true
      portal_mobile_fail lock-reclaim
      return 1
    fi

    stale_lock="$PORTAL_MOBILE_RECLAIM_DIR/stale-lock"
    if [ -e "$stale_lock" ] || [ -L "$stale_lock" ] || \
      ! mv "$PORTAL_MOBILE_LOCK_DIR" "$stale_lock" 2>/dev/null || \
      ! rm -rf -- "$stale_lock"; then
      portal_mobile_release_reclaim_claim || true
      portal_mobile_fail lock-reclaim
      return 1
    fi
    if ! portal_mobile_release_reclaim_claim; then
      portal_mobile_fail lock-reclaim
      return 1
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

portal_mobile_discard_evidence_temp() {
  local candidate="$1" discard_status=0
  rm -f -- "$candidate" || discard_status=1
  if [ "$discard_status" = 0 ] && [ "$PORTAL_MOBILE_EVIDENCE_TEMP" = "$candidate" ]; then
    PORTAL_MOBILE_EVIDENCE_TEMP=""
  fi
  return "$discard_status"
}

portal_mobile_finalize_evidence() {
  local evidence="$1" candidate="$2" expected_document="$3" sentinel="$4"
  shift 4
  local -a jq_arguments=("$@")
  [ "$(dirname -- "$candidate")" = "$(dirname -- "$evidence")" ] && \
    [ "$PORTAL_MOBILE_EVIDENCE_TEMP" = "$candidate" ] && \
    [ -f "$candidate" ] && [ ! -L "$candidate" ] || {
    portal_mobile_discard_evidence_temp "$candidate" || true
    portal_mobile_fail evidence-temp
    return 1
  }
  if ! jq -e "${jq_arguments[@]}" ". == ($expected_document)" "$candidate" >/dev/null; then
    portal_mobile_discard_evidence_temp "$candidate" || true
    portal_mobile_fail evidence-schema
    return 1
  fi
  if rg -qi "$sentinel" "$candidate"; then
    portal_mobile_discard_evidence_temp "$candidate" || true
    portal_mobile_fail evidence-schema
    return 1
  fi
  if ! mv -f -- "$candidate" "$evidence"; then
    portal_mobile_discard_evidence_temp "$candidate" || true
    portal_mobile_fail evidence-publish
    return 1
  fi
  PORTAL_MOBILE_EVIDENCE_TEMP=""
}

portal_mobile_exit() {
  local final_status="$1"
  portal_mobile_cleanup "$final_status" || final_status=$?
  exit "$final_status"
}

portal_mobile_start() {
  # Install cleanup before any side effect (lock, state dir, fetch, worktree,
  # support process, FIFO, compose stack, manifest) so a startup failure at
  # any later line still tears down whatever was already created. Signals use
  # their conventional statuses and cannot resume interrupted orchestration;
  # EXIT remains the single cleanup owner.
  trap 'portal_mobile_exit "$?"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  PORTAL_MOBILE_PLATFORM="$1"
  case "$PORTAL_MOBILE_PLATFORM" in ios|android) ;; *) portal_mobile_fail platform; return 1 ;; esac
  portal_mobile_acquire_lock || return 1

  local source_tree ready_fifo capability_fifo="" ready_status=""
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
  if [ "$PORTAL_MOBILE_PLATFORM" = "android" ]; then
    capability_fifo="$PORTAL_MOBILE_STATE_DIR/offer-capability.fifo"
    mkfifo "$capability_fifo"
    chmod 600 "$capability_fifo"
    exec 8<>"$capability_fifo"
    PORTAL_MOBILE_CAPABILITY_FIFO="$capability_fifo"
    PORTAL_MOBILE_CAPABILITY_FD_OPEN=1
  fi

  if ! git -C "$source_tree" fetch origin \
    "+$PORTAL_INTEGRATION_COMMIT:refs/oxid-evidence/portal-integration" \
    "+refs/pull/17/head:refs/oxid-evidence/portal-pr-17" \
    >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1; then
    portal_mobile_fail source-fetch
    return 1
  fi
  [ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-integration^{commit})" = "$PORTAL_INTEGRATION_COMMIT" ] || {
    portal_mobile_fail integration-commit
    return 1
  }
  [ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-integration^{tree})" = "$PORTAL_INTEGRATION_TREE" ] || {
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
  OXID_PORTAL_MOBILE_PLATFORM="$PORTAL_MOBILE_PLATFORM" \
  OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$PORTAL_MOBILE_CAPABILITY_FIFO" \
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
  # The EXIT owner passes its captured status explicitly before any cleanup
  # command can replace it. Runtime manifests and logs can contain private
  # protocol diagnostics, so they are removed on every exit.
  local incoming_status="$1" cleanup_status=0 source_tree lock_owner=""
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
    if [ -z "$PORTAL_MOBILE_CONTROL_ORIGIN" ]; then
      # Before READY, deliver TERM immediately. Node handles it as soon as any
      # current bounded spawnSync returns, then owns exact Compose teardown.
      # The KILL bound covers that command plus teardown and resource polling.
      portal_mobile_terminate_bounded \
        "$PORTAL_MOBILE_SUPPORT_PID" "$PORTAL_MOBILE_STARTUP_GRACE_SECONDS" \
        >/dev/null 2>&1 || cleanup_status=1
    else
      curl --noproxy '*' --silent \
        --connect-timeout "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
        --max-time "$PORTAL_MOBILE_CURL_TIMEOUT_SECONDS" \
        -X POST "$PORTAL_MOBILE_CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
      portal_mobile_wait_bounded \
        "$PORTAL_MOBILE_SUPPORT_PID" "$PORTAL_MOBILE_SUPPORT_GRACE_SECONDS" \
        >/dev/null 2>&1 || cleanup_status=1
    fi
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
  if [ "$PORTAL_MOBILE_CAPABILITY_FD_OPEN" = 1 ]; then
    exec 8>&-
    PORTAL_MOBILE_CAPABILITY_FD_OPEN=0
  fi
  if [ -n "$PORTAL_MOBILE_CAPABILITY_FIFO" ]; then
    rm -f -- "$PORTAL_MOBILE_CAPABILITY_FIFO" || cleanup_status=1
    PORTAL_MOBILE_CAPABILITY_FIFO=""
  fi
  if [ -n "$PORTAL_MOBILE_STATE_DIR" ]; then
    rm -rf "$PORTAL_MOBILE_STATE_DIR" || cleanup_status=1
  fi
  if [ -n "$PORTAL_MOBILE_EVIDENCE_TEMP" ]; then
    portal_mobile_discard_evidence_temp "$PORTAL_MOBILE_EVIDENCE_TEMP" || cleanup_status=1
  fi
  if [ "$PORTAL_MOBILE_RECLAIM_OWNED" = 1 ]; then
    portal_mobile_release_reclaim_claim || cleanup_status=1
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
