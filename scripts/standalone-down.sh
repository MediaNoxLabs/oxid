#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
export LC_ALL=C
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state_directory="${OXID_STANDALONE_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/oxid/standalone}"
environment_file="$state_directory/indexer.env"
owner_receipt="$state_directory/oxid-standalone.owner.receipt"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$repository_root/scripts/standalone-stack.yml"
project_ids() { docker ps -a --filter 'label=com.docker.compose.project=oxid-standalone' --quiet 2>/dev/null | sort; }
if [ ! -e "$owner_receipt" ] && [ ! -L "$owner_receipt" ]; then
  echo "Oxid standalone is attached or already stopped; no owner cleanup was performed."
  exit 0
fi
[ -f "$owner_receipt" ] && [ ! -L "$owner_receipt" ] || { echo "Unsafe standalone owner receipt." >&2; exit 1; }
ids="$(project_ids)"
schema="$(sed -n '1p' "$owner_receipt")"; owner="$(sed -n '2p' "$owner_receipt")"
project="$(sed -n '3p' "$owner_receipt")"; digest="$(sed -n '4p' "$owner_receipt")"; receipt_ids="$(sed -n '5,$p' "$owner_receipt")"
expected="$(shasum -a 256 "$compose_file" | awk '{print $1}')"
[ "$schema" = oxid-standalone-owner-v1 ] && [ "$owner" = ordinary ] && [ "$project" = oxid-standalone ] &&
  [ "$digest" = "$expected" ] && [ "$receipt_ids" = "$ids" ] || { echo "Standalone ownership could not be proven; no cleanup was performed." >&2; exit 1; }
if [ -f "$serve_marker" ]; then
  command -v tailscale >/dev/null 2>&1 || { echo "Owned Tailscale marker exists but CLI is unavailable." >&2; exit 1; }
  tailscale serve reset
  rm -f "$serve_marker"
fi
if [ -n "$ids" ]; then
  [ -f "$environment_file" ] && [ ! -L "$environment_file" ] || { echo "Owned indexer environment is unavailable." >&2; exit 1; }
  OXID_STANDALONE_ENV_FILE="$environment_file" docker compose -p oxid-standalone -f "$compose_file" down --timeout 30
fi
rm -f "$owner_receipt"
echo "Oxid standalone exact-owner services and routes are stopped; private configuration remains outside worktrees."
