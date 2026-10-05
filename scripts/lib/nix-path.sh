#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

oxid_admit_daemon_nix_path() {
  local daemon_profile_bin="${1:-/nix/var/nix/profiles/default/bin}"

  case "$daemon_profile_bin" in
    /*) ;;
    *) return 2 ;;
  esac

  if ! command -v nix >/dev/null 2>&1 \
    && [ -x "$daemon_profile_bin/nix" ]; then
    export PATH="$daemon_profile_bin:$PATH"
  fi
}
