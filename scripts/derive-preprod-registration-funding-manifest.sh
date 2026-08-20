#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ "${OXID_ENABLE_LIVE_PREPROD_E2E:-}" != "1" ]]; then
  echo "Set OXID_ENABLE_LIVE_PREPROD_E2E=1 to authorize deterministic preprod address derivation." >&2
  exit 1
fi
if [[ ! "${OXID_PREPROD_MASTER_SEED_HEX:-}" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "OXID_PREPROD_MASTER_SEED_HEX must be supplied as 32-byte hexadecimal without logging it." >&2
  exit 1
fi
if [[ ! "${OXID_PREPROD_E2E_CASE_INDEX:-}" =~ ^(0|[1-9][0-9]*)$ ]]; then
  echo "OXID_PREPROD_E2E_CASE_INDEX must be a canonical non-negative decimal integer." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "$(git -C "$repository_root" status --porcelain)" ]]; then
  echo "The preprod funding manifest requires a clean worktree so its commit is exact." >&2
  exit 1
fi

export OXID_PREPROD_E2E_COMMIT
OXID_PREPROD_E2E_COMMIT="$(git -C "$repository_root" rev-parse --verify HEAD)"

test_output=""
if ! test_output="$(
  cd "$repository_root"
  cargo test --quiet -p oxid-composition --lib \
    standalone_funding_tests::preprod_deterministic_funding_manifest_exposes_public_addresses_only \
    -- --ignored --exact --nocapture 2>&1
)"; then
  printf '%s\n' "$test_output" >&2
  exit 1
fi

manifest="$(
  printf '%s\n' "$test_output" | sed -n \
    '/^OXID_PREPROD_FUNDING_MANIFEST_V1$/,/^OXID_PREPROD_FUNDING_MANIFEST_END$/p'
)"
if [[ "$(printf '%s\n' "$manifest" | wc -l | tr -d ' ')" != "13" ]] \
  || [[ "$(printf '%s\n' "$manifest" | head -n 1)" != "OXID_PREPROD_FUNDING_MANIFEST_V1" ]] \
  || [[ "$(printf '%s\n' "$manifest" | tail -n 1)" != "OXID_PREPROD_FUNDING_MANIFEST_END" ]]; then
  echo "The preprod funding manifest did not match its closed public schema." >&2
  exit 1
fi

printf '%s\n' "$manifest"
