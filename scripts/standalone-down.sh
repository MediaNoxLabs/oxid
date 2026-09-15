#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
state_directory="$repository_root/target/standalone"
environment_file="$state_directory/indexer.env"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$repository_root/scripts/standalone-stack.yml"

if [ -f "$serve_marker" ]; then
  if ! command -v tailscale >/dev/null 2>&1; then
    echo "Tailscale Serve was configured by Oxid, but the CLI is unavailable." >&2
    exit 1
  fi
  tailscale serve reset
  rm -f "$serve_marker"
fi

# `cargo clean` removes target/standalone while Docker can still retain the
# exact named project. Compose needs a readable env-file merely to parse the
# topology during `down`, so use an empty file when the generated secrets have
# already been removed. The explicit command remains scoped to oxid-standalone.
compose_environment_file="$environment_file"
if [ ! -f "$compose_environment_file" ]; then
  compose_environment_file=/dev/null
fi
export OXID_STANDALONE_ENV_FILE="$compose_environment_file"
docker compose -p oxid-standalone -f "$compose_file" down --remove-orphans

echo "Oxid standalone services and owned Tailscale Serve routes are stopped."
echo "Generated development indexer configuration remains under target/standalone."
