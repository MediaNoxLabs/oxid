#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

usage() {
  printf '%s\n' \
    "Usage: ./bootstrap.sh [--pi [PI_ARGS...]]" \
    "       ./bootstrap.sh --check" \
    "       ./bootstrap.sh --audit-pi" \
    "       ./bootstrap.sh --configure-pi" \
    "       ./bootstrap.sh -- COMMAND [ARGS...]" \
    "" \
    "With no arguments, enter the pinned Nix development shell." \
    "Use --pi to start Pi, --check to validate the Pi integration," \
    "--audit-pi to inspect constitutional readiness, --configure-pi to" \
    "install the bounded user-level pi-subagents policy, or" \
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
    nix develop --command node scripts/factory/audit-pi.mjs --config-only --enforce-config
    exec nix develop --command pi "$@"
    ;;
  --check)
    shift
    if (( $# != 0 )); then
      echo "--check does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    exec nix develop --command just pi-smoke
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
