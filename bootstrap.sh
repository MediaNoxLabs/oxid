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

readonly nix_daemon_profile_bin="/nix/var/nix/profiles/default/bin"
nix_nested_profile_bin=""
if [[ "${OXID_BOOTSTRAP_NIX_PROFILE_BIN:-}" == "$nix_daemon_profile_bin" ]]; then
  nix_nested_profile_bin="$nix_daemon_profile_bin"
fi
unset OXID_BOOTSTRAP_NIX_PROFILE_BIN
if ! command -v nix >/dev/null 2>&1 && [[ -x "$nix_daemon_profile_bin/nix" ]]; then
  export PATH="$nix_daemon_profile_bin:$PATH"
  nix_nested_profile_bin="$nix_daemon_profile_bin"
fi
if ! command -v nix >/dev/null 2>&1; then
  echo "Nix is required; install it with flakes enabled before bootstrapping Oxid." >&2
  exit 1
fi

# `nix develop --command` replaces PATH with the development shell's tools.
# Retain the standard daemon profile only when this entrypoint used it to find
# Nix, so repository commands launched from that shell can invoke Nix again.
nix_develop_command() {
  exec nix develop --command bash -c '
    profile_bin="$1"
    shift
    if [[ -n "$profile_bin" ]]; then export PATH="$profile_bin:$PATH"; fi
    exec "$@"
  ' bootstrap-devshell "$nix_nested_profile_bin" "$@"
}

case "${1:-}" in
  "")
    exec nix develop
    ;;
  --pi)
    shift
    pi_arguments=("$@")
    dev_loop_requested=false
    for ((index = 0; index < ${#pi_arguments[@]}; index += 1)); do
      argument="${pi_arguments[$index]}"
      print_value=""
      if [[ "$argument" == "--print" ]] && ((index + 1 < ${#pi_arguments[@]})); then
        print_value="${pi_arguments[$((index + 1))]}"
      elif [[ "$argument" == --print=* ]]; then
        print_value="${argument#--print=}"
      fi
      if [[ "$print_value" == /dev-loop* ]]; then
        dev_loop_requested=true
        break
      fi
    done
    if [[ "$dev_loop_requested" == true ]]; then
      if ! command -v node >/dev/null 2>&1; then
        echo "Node.js is required outside the Nix shell to resolve an initial /dev-loop worktree." >&2
        exit 1
      fi
      pi_cwd="$(node "$repo_root/scripts/loop/bootstrap-dev-loop.mjs" --repo-root "$repo_root" -- "$@")" || exit $?
      if [[ "$pi_cwd" != "$repo_root" ]]; then
        canonical_bootstrap="$pi_cwd/bootstrap.sh"
        if [[ ! -x "$canonical_bootstrap" ]]; then
          echo "resolved canonical worktree has no executable bootstrap: $canonical_bootstrap" >&2
          exit 1
        fi
        export OXID_BOOTSTRAP_NIX_PROFILE_BIN="$nix_nested_profile_bin"
        exec "$canonical_bootstrap" --pi "$@"
      fi
    fi
    nix_develop_command bash -c '
      repo_root="$1"
      shift
      cd "$repo_root"
      node scripts/factory/audit-pi.mjs --config-only --enforce-config || {
        echo "Pi startup audit failed. If user-subagent-policy is red, run ./bootstrap.sh --configure-pi; otherwise fix the reported control, then retry ./bootstrap.sh --pi." >&2
        exit 1
      }
      bash scripts/check-pi-devshell.sh || {
        echo "Pi runtime smoke failed; resolve the reported package/resource problem before starting an agent." >&2
        exit 1
      }
      exec pi "$@"
    ' bootstrap-pi "$repo_root" "$@"
    ;;
  --check)
    shift
    if (( $# != 0 )); then
      echo "--check does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    nix_develop_command bash -c '
      set -e
      just factory-smoke
      node scripts/git-hooks/check-github-web-flow-key.mjs
    '
    ;;
  --audit-pi)
    shift
    if (( $# != 0 )); then
      echo "--audit-pi does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    nix_develop_command node scripts/factory/audit-pi.mjs
    ;;
  --configure-pi)
    shift
    if (( $# != 0 )); then
      echo "--configure-pi does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    nix_develop_command node scripts/factory/pi-policy.mjs apply --execute
    ;;
  --configure-git)
    shift
    if (( $# != 0 )); then
      echo "--configure-git does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    nix_develop_command node scripts/git-hooks/configure.mjs apply --execute
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
    nix_develop_command "$@"
    ;;
  *)
    echo "unknown bootstrap argument: $1" >&2
    usage >&2
    exit 2
    ;;
esac
