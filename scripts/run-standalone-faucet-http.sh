#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
state_directory="$repository_root/target/standalone-faucet-http"

"$repository_root/scripts/standalone-status.sh" local >/dev/null

umask 077
mkdir -p "$state_directory"
chmod 700 "$state_directory"

export OXID_ENABLE_STANDALONE_FAUCET=1
export OXID_PROFILE_STORE_PATH="$state_directory/profiles.json"

echo "Starting standalone faucet at http://127.0.0.1:36301; loopback only." >&2
exec cargo run --quiet -p oxid-headless --features standalone-faucet \
  --bin oxid-standalone-faucet-http
