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
      echo "Cannot resolve durable standalone state outside a Git checkout." >&2
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
  if [ -L "$state_directory" ] || { [ -e "$state_directory" ] && [ ! -d "$state_directory" ]; }; then
    echo "Standalone state directory must be a real directory, not a symlink." >&2
    return 1
  fi
  printf '%s\n' "$state_directory"
}

oxid_standalone_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ]
}
