#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ "${OXID_ENABLE_LIVE_STANDALONE_FUNDING:-}" != "1" ]]; then
  echo "Set OXID_ENABLE_LIVE_STANDALONE_FUNDING=1 to authorize the local funding fixture." >&2
  exit 1
fi
if [[ ! "${OXID_STANDALONE_FUNDER_SEED_HEX:-}" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "OXID_STANDALONE_FUNDER_SEED_HEX must be supplied as 32-byte hexadecimal without logging it." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
"$repository_root/scripts/standalone-up.sh" local

cargo test -p oxid-composition --lib \
  standalone_funding_tests::funded_shielded_finality_survives_adapter_reconstruction_and_consumes_the_input_once \
  -- --ignored --exact --nocapture
