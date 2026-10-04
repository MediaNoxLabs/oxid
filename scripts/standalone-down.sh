#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for required_command in docker git jq ln openssl ps sed shasum stat; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "Required command '$required_command' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/standalone-compose-ownership.sh
source "$repository_root/scripts/lib/standalone-compose-ownership.sh"
# shellcheck source=lib/standalone-state.sh
source "$repository_root/scripts/lib/standalone-state.sh"
state_directory="$(oxid_standalone_state_directory "$repository_root")"
environment_file="$state_directory/canonical-indexer.env"
serve_marker="$state_directory/tailscale-serve-owned"
compose_file="$state_directory/canonical-compose.yml"
owner_receipt="$state_directory/owner-receipt.json"
session_id="$(printf '%s' "$repository_root" | shasum -a 256 | awk '{print $1}')"
lease_id="$(openssl rand -hex 16)"

umask 077
oxid_standalone_prepare_state_directory "$state_directory"
release_startup_lease() {
  oxid_standalone_release_lease "$state_directory" "$session_id" "$lease_id"
}
trap release_startup_lease EXIT
oxid_standalone_acquire_lease "$state_directory" "$session_id" "$lease_id"

if ! oxid_standalone_regular_file "$owner_receipt" || \
  ! oxid_standalone_regular_file "$compose_file"; then
  echo "Standalone ownership is not proven; preserving resources." >&2
  exit 1
fi
current_ids="$(docker ps -a --filter label=com.docker.compose.project=oxid-standalone --format '{{.ID}}' | sort | jq -Rsc 'split("\n") | map(select(length > 0))')"
if ! oxid_validate_standalone_compose_ownership \
  "$compose_file" "$(dirname -- "$compose_file")" "$current_ids"; then
  echo "Standalone resources have mixed or foreign Compose ownership; preserving them." >&2
  exit 1
fi
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

compose_environment_file="$environment_file"
if [ -L "$compose_environment_file" ]; then
  echo "Standalone environment ownership is symlinked; preserving resources." >&2
  exit 1
fi
if [ ! -f "$compose_environment_file" ]; then
  compose_environment_file=/dev/null
fi
export OXID_STANDALONE_ENV_FILE="$compose_environment_file"

if [ -f "$serve_marker" ]; then
  if ! command -v tailscale >/dev/null 2>&1; then
    echo "Tailscale Serve was configured by Oxid, but the CLI is unavailable." >&2
    exit 1
  fi
  tailscale serve reset
  rm -f "$serve_marker"
fi

docker compose -p oxid-standalone -f "$compose_file" down --remove-orphans
rm -f -- "$owner_receipt" "$environment_file" "$compose_file"

echo "Oxid standalone services and owned Tailscale Serve routes are stopped."
echo "Generated standalone credentials and canonical ownership files were removed."
