#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Resolve standalone ownership state outside launcher-scoped temporary roots.
# The Git common directory is shared by managed worktrees, so a later shell can
# validate the same receipt without broadening Docker ownership.
oxid_standalone_state_directory() {
  local repository_root="$1"
  local state_directory git_common_directory git_top_level legacy_directory physical_repository_root physical_git_top_level

  if [ -n "${OXID_STANDALONE_STATE_DIR:-}" ]; then
    state_directory="$OXID_STANDALONE_STATE_DIR"
  else
    legacy_directory="${TMPDIR:-/tmp}/oxid-standalone"
    if [ -e "$legacy_directory" ] || [ -L "$legacy_directory" ]; then
      echo "Legacy standalone state detected at the retired temporary layout; durable state ignores it. Verify it is unused, then remove only that directory manually." >&2
    fi
    git_top_level="$(git -C "$repository_root" rev-parse --show-toplevel 2>/dev/null)" || {
      echo "Cannot resolve durable standalone state outside a Git checkout; set OXID_STANDALONE_STATE_DIR to an absolute private directory." >&2
      return 1
    }
    physical_repository_root="$(cd -- "$repository_root" && pwd -P)" || return 1
    physical_git_top_level="$(cd -- "$git_top_level" && pwd -P)" || return 1
    if [ "$physical_repository_root" != "$physical_git_top_level" ]; then
      echo "Standalone launcher root is not the checkout Git top level; refusing enclosing repository state." >&2
      return 1
    fi
    git_common_directory="$(git -C "$git_top_level" rev-parse --git-common-dir 2>/dev/null)" || {
      echo "Cannot resolve the checkout Git common directory; set OXID_STANDALONE_STATE_DIR to an absolute private directory." >&2
      return 1
    }
    case "$git_common_directory" in
      /*) ;;
      *) git_common_directory="$git_top_level/$git_common_directory" ;;
    esac
    state_directory="${git_common_directory%/}/oxid/standalone"
  fi

  while [ "$state_directory" != "/" ] && [ "${state_directory%/}" != "$state_directory" ]; do
    state_directory="${state_directory%/}"
  done
  if [ "$state_directory" = "/" ]; then
    echo "Standalone state directory must not be the filesystem root." >&2
    return 1
  fi
  case "$state_directory" in
    /*) ;;
    *)
      echo "Standalone state directory must be absolute." >&2
      return 1
      ;;
  esac
  if [ -L "$state_directory" ]; then
    echo "Standalone state directory must not be a symlink." >&2
    return 1
  fi
  local existing_parent="$state_directory" unresolved_suffix="" component physical_parent
  while [ ! -d "$existing_parent" ]; do
    if [ -L "$existing_parent" ]; then
      echo "Standalone state path must not contain dangling symlinks." >&2
      return 1
    fi
    if [ -e "$existing_parent" ]; then
      echo "Standalone state path must contain directories only." >&2
      return 1
    fi
    component="${existing_parent##*/}"
    case "$component" in
      ''|.|..)
        echo "Standalone state directory has an invalid path component." >&2
        return 1
        ;;
    esac
    unresolved_suffix="/$component$unresolved_suffix"
    existing_parent="${existing_parent%/*}"
    [ -n "$existing_parent" ] || existing_parent="/"
  done
  physical_parent="$(cd -- "$existing_parent" && pwd -P)" || return 1
  state_directory="${physical_parent%/}$unresolved_suffix"
  printf '%s\n' "$state_directory"
}

oxid_standalone_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ]
}

oxid_standalone_file_mode() {
  if stat -c '%a' "$1" 2>/dev/null; then
    return
  fi
  stat -f '%Lp' "$1"
}

oxid_standalone_prepare_state_directory() {
  local state_directory="$1"
  local parent_directory

  if [ -e "$state_directory" ] || [ -L "$state_directory" ]; then
    if [ ! -d "$state_directory" ] || [ -L "$state_directory" ]; then
      echo "Standalone state directory must be a private regular directory." >&2
      return 1
    fi
    if [ "$(oxid_standalone_file_mode "$state_directory")" != 700 ]; then
      echo "Existing standalone state directory must have mode 700; refusing permission mutation." >&2
      return 1
    fi
    return
  fi

  parent_directory="${state_directory%/*}"
  [ -n "$parent_directory" ] || parent_directory=/
  mkdir -p -- "$parent_directory"
  if mkdir -- "$state_directory" 2>/dev/null; then
    chmod 700 "$state_directory"
  elif [ ! -d "$state_directory" ] || [ -L "$state_directory" ] \
    || [ "$(oxid_standalone_file_mode "$state_directory")" != 700 ]; then
    echo "Standalone state directory creation raced with an unsafe path." >&2
    return 1
  fi
}

oxid_standalone_process_start() {
  local pid="$1" proc_start proc_stat
  if [ -r "/proc/$pid/stat" ]; then
    proc_stat="$(cat "/proc/$pid/stat" 2>/dev/null)"
    proc_start="$(oxid_standalone_process_start_from_proc_stat "$proc_stat" || true)"
    if [ -n "$proc_start" ]; then
      printf 'proc:%s\n' "$proc_start"
      return 0
    fi
  fi
  proc_start="$(ps -p "$pid" -o lstart= 2>/dev/null | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  [ -n "$proc_start" ] && printf 'lstart:%s\n' "$proc_start"
}

oxid_standalone_process_start_from_proc_stat() {
  local proc_stat="$1" after_command process_start
  case "$proc_stat" in
    *') '*) after_command="${proc_stat##*) }" ;;
    *) return 1 ;;
  esac
  process_start="$(printf '%s\n' "$after_command" | awk '{print $20}')"
  [[ "$process_start" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$process_start"
}

oxid_standalone_cleanup_lease_artifacts() {
  local state_directory="$1" artifact
  for artifact in "$state_directory"/.startup-lease-candidate-* "$state_directory"/.startup-lease-stale-*; do
    [ -e "$artifact" ] || continue
    if [ ! -f "$artifact" ] || [ -L "$artifact" ]; then
      continue
    fi
    if find "$artifact" -mmin +1440 -print -quit 2>/dev/null | grep -q .; then
      rm -f -- "$artifact"
    fi
  done
}

oxid_standalone_release_lease() {
  local state_directory="$1" session_id="$2" lease_id="$3"
  local lease_directory="$state_directory/startup-lease"
  local lease_record="$lease_directory/owner.json"

  if oxid_standalone_regular_file "$lease_record" && jq -e \
    --arg session "$session_id" --arg lease "$lease_id" \
    '(.schema == "oxid-standalone-lease-v2" or .schema == "oxid-standalone-lease-v3")
      and .session == $session and .lease == $lease' \
    "$lease_record" >/dev/null 2>&1; then
    rm -f -- "$lease_record"
  fi
}

oxid_standalone_acquire_lease() {
  local state_directory="$1" session_id="$2" lease_id="$3"
  local lease_directory="$state_directory/startup-lease"
  local lease_record="$lease_directory/owner.json"
  local candidate owner_pid owner_start owner_schema owner_is_live current_start quarantine attempt

  oxid_standalone_cleanup_lease_artifacts "$state_directory"
  for attempt in 1 2 3; do
    if mkdir -- "$lease_directory" 2>/dev/null; then
      chmod 700 "$lease_directory"
    elif [ ! -d "$lease_directory" ] || [ -L "$lease_directory" ] \
      || [ "$(oxid_standalone_file_mode "$lease_directory")" != 700 ]; then
      echo "Standalone lease directory is unsafe; refusing mutation." >&2
      return 1
    fi
    candidate="$state_directory/.startup-lease-candidate-$lease_id"
    (set -C; : >"$candidate") 2>/dev/null || {
      echo "Standalone lease candidate already exists; refusing mutation." >&2
      return 1
    }
    chmod 600 "$candidate"
    owner_start="$(oxid_standalone_process_start "$$" || true)"
    [ -n "$owner_start" ] || {
      rm -f -- "$candidate"
      echo "Cannot determine standalone lease process identity." >&2
      return 1
    }
    jq -cn --arg session "$session_id" --arg lease "$lease_id" \
      --argjson pid "$$" --arg processStart "$owner_start" \
      '{schema:"oxid-standalone-lease-v3",session:$session,lease:$lease,pid:$pid,processStart:$processStart}' \
      >"$candidate"
    if ln -- "$candidate" "$lease_record" 2>/dev/null; then
      rm -f -- "$candidate"
      return 0
    fi
    rm -f -- "$candidate"

    if ! oxid_standalone_regular_file "$lease_record" || ! jq -e \
      '(.schema == "oxid-standalone-lease-v2" or .schema == "oxid-standalone-lease-v3")
        and (.session | type == "string") and (.lease | type == "string")
        and (.pid | type == "number") and (.pid > 0)
        and (.processStart | type == "string") and (.processStart | length > 0)' \
      "$lease_record" >/dev/null 2>&1; then
      echo "Standalone startup lease ownership is ambiguous; refusing mutation." >&2
      return 1
    fi
    owner_pid="$(jq -r '.pid' "$lease_record")"
    owner_start="$(jq -r '.processStart' "$lease_record")"
    owner_schema="$(jq -r '.schema' "$lease_record")"
    current_start="$(oxid_standalone_process_start "$owner_pid" || true)"
    owner_is_live=0
    if [ -n "$current_start" ]; then
      if [ "$owner_schema" = "oxid-standalone-lease-v3" ] && [ "$current_start" = "$owner_start" ]; then
        owner_is_live=1
      elif [ "$owner_schema" = "oxid-standalone-lease-v2" ]; then
        case "$current_start" in
          lstart:*) [ "${current_start#lstart:}" = "$owner_start" ] && owner_is_live=1 ;;
          proc:*) owner_is_live=1 ;;
        esac
      fi
    fi
    if [ "$owner_is_live" -eq 1 ]; then
      jq -cn --arg owner "$(jq -r '.session[0:12]' "$lease_record")" \
        '{schema:"oxid-standalone-lease-v2",state:"contention",ownerPrefix:$owner}' >&2
      return 2
    fi

    quarantine="$state_directory/.startup-lease-stale-$lease_id"
    if mv -- "$lease_record" "$quarantine" 2>/dev/null; then
      rm -f -- "$quarantine"
    fi
  done
  echo "Standalone startup lease changed repeatedly; retry later." >&2
  return 2
}
