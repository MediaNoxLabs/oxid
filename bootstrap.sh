#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

usage() {
  printf '%s\n' \
    "Usage: ./bootstrap.sh [--pi [PI_ARGS...]]" \
    "       ./bootstrap.sh --check" \
    "       ./bootstrap.sh --audit-pi" \
    "       ./bootstrap.sh --configure-pi" \
    "       ./bootstrap.sh --configure-git" \
    "       ./bootstrap.sh -- COMMAND [ARGS...]" \
    "" \
    "With no arguments, enter the pinned Nix development shell." \
    "Use --pi to start Pi, --check to validate factory integrations," \
    "--audit-pi to inspect constitutional readiness, --configure-pi to" \
    "install the bounded user-level pi-subagents policy, --configure-git" \
    "to install repository-local contribution hooks and signing defaults, or" \
    "-- to run one command inside the development shell."
}

if ! command -v nix >/dev/null 2>&1; then
  echo "Nix is required; install it with flakes enabled before bootstrapping Oxid." >&2
  exit 1
fi

case "${1:-}" in
  "")
    exec nix develop
    ;;
  --pi)
    shift
    exec nix develop --command bash -c '
      node scripts/factory/audit-pi.mjs --config-only --enforce-config || {
        echo "Pi startup audit failed. If user-subagent-policy is red, run ./bootstrap.sh --configure-pi; otherwise fix the reported control, then retry ./bootstrap.sh --pi." >&2
        exit 1
      }
      exec pi "$@"
    ' bootstrap-pi "$@"
    ;;
  --check)
    shift
    if (( $# != 0 )); then
      echo "--check does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command just factory-smoke
    ;;
  --audit-pi)
    shift
    if (( $# != 0 )); then
      echo "--audit-pi does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command node scripts/factory/audit-pi.mjs
    ;;
  --configure-pi)
    shift
    if (( $# != 0 )); then
      echo "--configure-pi does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command node scripts/factory/pi-policy.mjs apply --execute
    ;;
  --configure-git)
    shift
    if (( $# != 0 )); then
      echo "--configure-git does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command node scripts/git-hooks/configure.mjs apply --execute
    ;;
  --help|-h)
    usage
    ;;
  --)
    shift
    if (( $# == 0 )); then
      echo "-- requires a command" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command "$@"
    ;;
  *)
    echo "unknown bootstrap argument: $1" >&2
    usage >&2
    exit 2
    ;;
esac
