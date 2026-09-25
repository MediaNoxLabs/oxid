#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
state_directory="${OXID_STANDALONE_STATE_DIR:-${TMPDIR:-/tmp}/oxid-standalone}"
environment_file="$state_directory/canonical-indexer.env"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$state_directory/canonical-compose.yml"
owner_receipt="$state_directory/owner-receipt.json"
session_id="$(printf '%s' "$repository_root" | shasum -a 256 | awk '{print $1}')"

if [ ! -f "$owner_receipt" ] || ! [ -f "$compose_file" ]; then
  echo "Standalone ownership is not proven; preserving resources." >&2
  exit 1
fi
current_ids="$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --format '{{.ID}}' | sort | jq -Rsc 'split("\n") | map(select(length > 0))')"
jq -e --arg session "$session_id" \
  --arg compose "$(shasum -a 256 "$compose_file" | awk '{print $1}')" \
  --argjson containers "$current_ids" \
  '.schema == "oxid-standalone-owner-v1"
    and .session == $session
    and .composeSha256 == $compose
    and .containerIds == $containers' \
  "$owner_receipt" >/dev/null || {
    echo "Standalone ownership is not proven; preserving resources." >&2
    exit 1
  }

if [ -f "$serve_marker" ]; then
  if ! command -v tailscale >/dev/null 2>&1; then
    echo "Tailscale Serve was configured by Oxid, but the CLI is unavailable." >&2
    exit 1
  fi
  tailscale serve reset
  rm -f "$serve_marker"
fi

compose_environment_file="$environment_file"
if [ ! -f "$compose_environment_file" ]; then
  compose_environment_file=/dev/null
fi
export OXID_STANDALONE_ENV_FILE="$compose_environment_file"
docker compose -p oxid-standalone -f "$compose_file" down --remove-orphans
rm -f -- "$owner_receipt"

echo "Oxid standalone services and owned Tailscale Serve routes are stopped."
echo "Canonical development configuration remains under $state_directory."
