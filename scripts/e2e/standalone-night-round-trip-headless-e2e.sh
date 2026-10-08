#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ "${OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E:-}" != "1" ]]; then
  echo "Set OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E=1 to authorize the two-wallet NIGHT round trip." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
"$repository_root/scripts/standalone-status.sh" local >/dev/null

# The six sequential funding, registration, and transfer submissions may each
# use a ten-minute proof and five-minute submission window. Add the two bounded
# DUST windows, balance/finality polling, compilation, and cleanup headroom.
# Ordinary local runs complete far sooner. GNU timeout provides an additional
# process backstop when available; the Rust harness retains bounded operation,
# reconciliation, and child-shutdown deadlines on macOS hosts without it.
scenario=(
  env -u OXID_MIDNIGHT_PROVING_CACHE_DIR
  cargo test -p oxid-headless
  --features standalone-faucet,development-movement-approval-fixture
  --test standalone_faucet_live
  two_fresh_wallets_complete_a_night_round_trip_and_reconcile_history
  -- --ignored --exact --nocapture
)
if command -v timeout >/dev/null 2>&1; then
  timeout --preserve-status -k 30s 8400s "${scenario[@]}"
elif command -v gtimeout >/dev/null 2>&1; then
  gtimeout --preserve-status -k 30s 8400s "${scenario[@]}"
else
  "${scenario[@]}"
fi
