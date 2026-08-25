# SPDX-License-Identifier: Apache-2.0
# Shared, source-only orchestration for the sequential iOS/Android Portal suites.

readonly PORTAL_EXPECTED_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
# Names stay distinct from the parsed STACK_ENV_FILE fields populated later by
# stack-env-v1.sh; readonly authority constants must not shadow loader output.
readonly PORTAL_EXPECTED_HELPER_COMMIT="da9adad711a83c25505f96d88809c7320d049b2e"
readonly PORTAL_EXPECTED_HELPER_TREE="01a78541d24b7402a0eb1f7d1ca2c0f91de95fd3"
readonly PORTAL_INTEGRATION_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly PORTAL_INTEGRATION_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly PORTAL_PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly PORTAL_PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly PORTAL_EXPECTED_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"

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
PORTAL_MOBILE_PROFILE=""
PORTAL_MOBILE_PUBLIC_ORIGIN=""
PORTAL_MOBILE_TAILNET_SERVE_ACTIVE=0
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
  [ -n "${PORTAL_PROTOCOL_SOURCE_DIR:-}" ] && [ -d "$PORTAL_PROTOCOL_SOURCE_DIR" ] || return 1
  printf '%s\n' "$PORTAL_PROTOCOL_SOURCE_DIR"
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
  PORTAL_MOBILE_PROFILE="${OXID_MOBILE_PORTAL_PROFILE:-local}"
  PORTAL_MOBILE_PUBLIC_ORIGIN="${OXID_BUILD_PORTAL_PUBLIC_ORIGIN:-}"
  case "$PORTAL_MOBILE_PROFILE" in
    local) ;;
    tailnet-ios-simulator)
      [ "$PORTAL_MOBILE_PLATFORM" = ios ] && \
        [[ "$PORTAL_MOBILE_PUBLIC_ORIGIN" =~ ^https://([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.ts\.net:9443$ ]] || {
        portal_mobile_fail tailnet-origin
        return 1
      }
      ;;
    tailnet-android-physical)
      [ "$PORTAL_MOBILE_PLATFORM" = android ] && \
        [ "$PORTAL_MOBILE_PUBLIC_ORIGIN" = "https://yuriys-macbook-pro.taila4adff.ts.net:9443" ] || {
        portal_mobile_fail tailnet-origin
        return 1
      }
      ;;
    *) portal_mobile_fail profile; return 1 ;;
  esac
  portal_mobile_acquire_lock || return 1

  local source_tree ready_fifo capability_fifo="" ready_status=""
  PORTAL_MOBILE_REPOSITORY_ROOT="$(git rev-parse --show-toplevel)"
  # shellcheck source=scripts/e2e/stack-env-v1.sh
  source "$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/e2e/stack-env-v1.sh"
  [ -n "${STACK_ENV_FILE:-}" ] || { portal_mobile_fail profile; return 1; }
  stack_env_load "$STACK_ENV_FILE" || { portal_mobile_fail "$STACK_ENV_ERROR"; return 1; }
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

  [ "$source_tree" = "$PORTAL_PROTOCOL_SOURCE_DIR" ] || {
    portal_mobile_fail source-path
    return 1
  }
  status_file="$PORTAL_MOBILE_STATE_DIR/shared-status.json"
  stack_env_delegate_portal status >"$status_file" 2>>"$PORTAL_MOBILE_PRIVATE_LOG" || {
    portal_mobile_fail shared-status
    return 1
  }
  jq -e '.state == "running" and .midnight_state == "ready"' "$status_file" >/dev/null || {
    portal_mobile_fail shared-stack-not-ready
    return 1
  }
  rm -f -- "$status_file"

  ready_fifo="$PORTAL_MOBILE_STATE_DIR/ready.fifo"
  mkfifo "$ready_fifo"
  chmod 600 "$ready_fifo"
  # Open both ends before spawning support so neither side can block in open(2).
  # The read itself is bounded for the full compose/issuer readiness window.
  exec 9<>"$ready_fifo"
  COMPOSE_PROJECT_NAME="$PORTAL_COMPOSE_PROJECT" \
  STACK_ENV_FILE="$STACK_ENV_PATH" \
  OXID_LOCAL_HEADLESS_SCRIPT="$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/local-headless.sh" \
  PORTAL_INTEGRATION_CHECKOUT="$source_tree" \
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
  expected_issuer_origin="http://127.0.0.1:18090"
  expected_resolver_origin="http://127.0.0.1:18093"
  if [[ "$PORTAL_MOBILE_PROFILE" = tailnet-ios-simulator || \
        "$PORTAL_MOBILE_PROFILE" = tailnet-android-physical ]]; then
    expected_issuer_origin="$PORTAL_MOBILE_PUBLIC_ORIGIN"
    expected_resolver_origin="$PORTAL_MOBILE_PUBLIC_ORIGIN/issuer-resolver"
  fi
  [ "$(jq -r '.issuerOrigin // empty' "$ready")" = "$expected_issuer_origin" ] && \
    [ "$(jq -r '.issuerResolverOrigin // empty' "$ready")" = "$expected_resolver_origin" ] && \
    [ "$(jq -r '.offerUrl // empty' "$ready")" = "$expected_issuer_origin/offer" ] || {
    portal_mobile_fail public-origins
    return 1
  }
  [ -f "$PORTAL_MOBILE_MANIFEST_PATH" ] && \
    [[ "$PORTAL_MOBILE_MANIFEST_SHA256" =~ ^[0-9a-f]{64}$ ]] && \
    [ "$(shasum -a 256 "$PORTAL_MOBILE_MANIFEST_PATH" | awk '{print $1}')" = "$PORTAL_MOBILE_MANIFEST_SHA256" ] || {
    portal_mobile_fail manifest
    return 1
  }

  if [[ "$PORTAL_MOBILE_PROFILE" = tailnet-ios-simulator || \
        "$PORTAL_MOBILE_PROFILE" = tailnet-android-physical ]]; then
    OXID_PORTAL_TAILNET_STATE_DIR="$PORTAL_MOBILE_STATE_DIR/tailscale-serve" \
      "$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/portal-tailnet-serve.sh" \
        up "$PORTAL_MOBILE_PUBLIC_ORIGIN" >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 || {
      portal_mobile_fail tailscale-serve
      return 1
    }
    PORTAL_MOBILE_TAILNET_SERVE_ACTIVE=1
    public_host="${PORTAL_MOBILE_PUBLIC_ORIGIN#https://}"
    public_host="${public_host%:9443}"
    export OXID_BUILD_MIDNIGHT_INDEXER_WS_URL="wss://$public_host:8443/api/v4/graphql/ws"
    export OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL="https://$public_host:8443/api/v4/graphql"
    export OXID_BUILD_MIDNIGHT_NODE_WS_URL="wss://$public_host:10000"
    export OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL="https://$public_host"
  fi
  export OXID_MOBILE_PORTAL_PROFILE="$PORTAL_MOBILE_PROFILE"
  export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$PORTAL_MOBILE_MANIFEST_PATH"
  export OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$PORTAL_MOBILE_MANIFEST_SHA256"
}

portal_mobile_finish() {
  local result=0
  if [ "$PORTAL_MOBILE_TAILNET_SERVE_ACTIVE" = 1 ]; then
    OXID_PORTAL_TAILNET_STATE_DIR="$PORTAL_MOBILE_STATE_DIR/tailscale-serve" \
      "$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/portal-tailnet-serve.sh" \
        down "$PORTAL_MOBILE_PUBLIC_ORIGIN" >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 || result=1
    PORTAL_MOBILE_TAILNET_SERVE_ACTIVE=0
  fi
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
  if [ "$PORTAL_MOBILE_TAILNET_SERVE_ACTIVE" = 1 ]; then
    OXID_PORTAL_TAILNET_STATE_DIR="$PORTAL_MOBILE_STATE_DIR/tailscale-serve" \
      "$PORTAL_MOBILE_REPOSITORY_ROOT/scripts/portal-tailnet-serve.sh" \
        down "$PORTAL_MOBILE_PUBLIC_ORIGIN" >>"${PORTAL_MOBILE_PRIVATE_LOG:-/dev/null}" 2>&1 || cleanup_status=1
    PORTAL_MOBILE_TAILNET_SERVE_ACTIVE=0
  fi
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
