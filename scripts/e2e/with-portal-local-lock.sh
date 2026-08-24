#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

fail() {
  printf 'portal-local-lock: FAIL phase=%s\n' "$1" >&2
  exit 1
}

if [ "$#" -lt 3 ] || [ "$2" != "--" ]; then
  echo "usage: with-portal-local-lock.sh <absolute-lock-file> -- <command> [args...]" >&2
  exit 2
fi
lock_file="$1"
shift 2
[[ "$lock_file" = /* ]] || fail lock-path
lock_parent="$(dirname -- "$lock_file")"
[ -d "$lock_parent" ] && [ ! -L "$lock_parent" ] || fail lock-parent
if [ -e "$lock_file" ] || [ -L "$lock_file" ]; then
  [ -f "$lock_file" ] && [ ! -L "$lock_file" ] || fail lock-file
fi

umask 077
if command -v flock >/dev/null 2>&1; then
  exec flock -n "$lock_file" "$@"
fi
if command -v lockf >/dev/null 2>&1; then
  exec lockf -t 0 "$lock_file" "$@"
fi
fail lock-provider
