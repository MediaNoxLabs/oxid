#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ "${OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E:-}" != "1" ]]; then
  echo "Set OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E=1 to authorize the two-wallet NIGHT round trip." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
"$repository_root/scripts/standalone-status.sh" local >/dev/null

timeout --preserve-status -k 30s 1500s \
  cargo test -p oxid-headless --features standalone-faucet \
    --test standalone_faucet_live \
    two_fresh_wallets_complete_a_night_round_trip_and_reconcile_history \
    -- --ignored --exact --nocapture
