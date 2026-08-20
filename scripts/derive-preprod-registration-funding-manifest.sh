#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
set +x

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

preprod_master_seed_hex="$OXID_PREPROD_MASTER_SEED_HEX"
unset OXID_PREPROD_MASTER_SEED_HEX

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "$(git -C "$repository_root" status --porcelain)" ]]; then
  echo "The preprod funding manifest requires a clean worktree so its commit is exact." >&2
  exit 1
fi

export OXID_PREPROD_E2E_COMMIT
OXID_PREPROD_E2E_COMMIT="$(git -C "$repository_root" rev-parse --verify HEAD)"

build_output=""
if ! build_output="$(
  cd "$repository_root"
  cargo test --locked --no-run --message-format=json -p oxid-composition --lib 2>&1
)"; then
  printf '%s\n' "$build_output" >&2
  exit 1
fi
if [[ -n "$(git -C "$repository_root" status --porcelain)" ]] \
  || [[ "$(git -C "$repository_root" rev-parse --verify HEAD)" != "$OXID_PREPROD_E2E_COMMIT" ]]; then
  echo "The worktree or HEAD changed while building the manifest helper; refusing unbound output." >&2
  exit 1
fi
test_executable="$(
  printf '%s\n' "$build_output" \
    | awk '/"name":"oxid_composition"/ && /"profile":\{[^}]*"test":true/ && /"executable":"/ { line = $0; sub(/^.*"executable":"/, "", line); sub(/".*$/, "", line); print line }' \
    | tail -n 1
)"
if [[ -z "$test_executable" || ! -x "$test_executable" ]]; then
  echo "Cargo did not produce the expected Oxid composition test executable." >&2
  exit 1
fi

test_output=""
if ! test_output="$(
  OXID_PREPROD_MASTER_SEED_HEX="$preprod_master_seed_hex" \
    "$test_executable" \
    standalone_funding_tests::preprod_deterministic_funding_manifest_exposes_public_addresses_only \
    --ignored --exact --nocapture 2>&1
)"; then
  printf '%s\n' "$test_output" >&2
  exit 1
fi

manifest="$(
  printf '%s\n' "$test_output" | sed -n \
    '/^OXID_PREPROD_FUNDING_MANIFEST_V2$/,/^OXID_PREPROD_FUNDING_MANIFEST_END$/p'
)"
expected_keys="$({
  printf '%s\n' \
    commit \
    network \
    caseIndex \
    walletA.accountIndex \
    walletA.addressIndex \
    walletA.nightUnshieldedAddress \
    walletA.nightShieldedAddress \
    walletA.unshieldedNightRequirement \
    walletA.shieldedNightRequirement \
    walletA.expectedEligibleUnshieldedOutputCount \
    walletA.expectedShieldedNoteCount \
    walletB.accountIndex \
    walletB.addressIndex \
    walletB.nightUnshieldedAddress \
    walletB.nightShieldedAddress \
    walletB.expectedUnshieldedNightAtomicUnits \
    walletB.expectedShieldedNightAtomicUnits \
    walletB.expectedEligibleUnshieldedOutputCount \
    walletB.expectedShieldedNoteCount \
    transfer.policy
})"
observed_keys="$(
  printf '%s\n' "$manifest" \
    | sed '1d;$d' \
    | sed 's/=.*//'
)"
if [[ "$(printf '%s\n' "$manifest" | wc -l | tr -d ' ')" != "22" ]] \
  || [[ "$(printf '%s\n' "$manifest" | head -n 1)" != "OXID_PREPROD_FUNDING_MANIFEST_V2" ]] \
  || [[ "$(printf '%s\n' "$manifest" | tail -n 1)" != "OXID_PREPROD_FUNDING_MANIFEST_END" ]] \
  || [[ "$observed_keys" != "$expected_keys" ]]; then
  echo "The preprod funding manifest did not match its closed public schema." >&2
  exit 1
fi

printf '%s\n' "$manifest"
