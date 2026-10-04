#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Resolve standalone ownership state outside launcher-scoped temporary roots.
# The Git common directory is shared by managed worktrees, so a later shell can
# validate the same receipt without broadening Docker ownership.
oxid_standalone_state_directory() {
  local repository_root="$1"
  local state_directory git_common_directory

  if [ -n "${OXID_STANDALONE_STATE_DIR:-}" ]; then
    state_directory="$OXID_STANDALONE_STATE_DIR"
  else
    git_common_directory="$(
      git -C "$repository_root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null
    )" || {
      echo "Cannot resolve durable standalone state outside a Git checkout; set OXID_STANDALONE_STATE_DIR to an absolute private directory." >&2
      return 1
    }
    state_directory="${git_common_directory%/}/oxid/standalone"
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
