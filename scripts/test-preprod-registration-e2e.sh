#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
set +x
umask 077

if [[ "${OXID_ENABLE_LIVE_PREPROD_E2E:-}" != "1" ]]; then
  echo "Set OXID_ENABLE_LIVE_PREPROD_E2E=1 to authorize the funded PreProd write test." >&2
  exit 1
fi
if [[ "${OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY:-}" != "1" ]]; then
  echo "Set OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY=1 after accepting that the public test prover receives private proof inputs over TLS." >&2
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
git_common_dir="$(git -C "$repository_root" rev-parse --git-common-dir)"
if [[ "$git_common_dir" != /* ]]; then
  git_common_dir="$repository_root/$git_common_dir"
fi
git_common_dir="$(cd "$git_common_dir" && pwd -P)"
case_marker_root="$git_common_dir/oxid-state/preprod-registration-e2e"
case_marker="$case_marker_root/case-${OXID_PREPROD_E2E_CASE_INDEX}.started"
if [[ -e "$case_marker" ]]; then
  echo "This PreProd case index was already started locally. Do not clear the marker or retry a possibly broadcast case; select a fresh funded case index." >&2
  exit 1
fi
OXID_ENABLE_LIVE_PREPROD_E2E=1 \
OXID_PREPROD_MASTER_SEED_HEX="$OXID_PREPROD_MASTER_SEED_HEX" \
OXID_PREPROD_E2E_CASE_INDEX="$OXID_PREPROD_E2E_CASE_INDEX" \
  "$repository_root/scripts/observe-preprod-registration-funding.sh"

preprod_master_seed_hex="$OXID_PREPROD_MASTER_SEED_HEX"
unset OXID_PREPROD_MASTER_SEED_HEX

if [[ -n "$(git -C "$repository_root" status --porcelain)" ]]; then
  echo "The funded PreProd test requires a clean worktree so its evidence is bound to one commit." >&2
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
  echo "The worktree or HEAD changed while building the funded helper; refusing an unbound write." >&2
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

mkdir -p "$case_marker_root"
if ! mkdir "$case_marker" 2>/dev/null; then
  echo "This PreProd case index was already started locally. Do not clear the marker or retry a possibly broadcast case; select a fresh funded case index." >&2
  exit 1
fi
export OXID_PREPROD_E2E_STATE_DIR="$case_marker/state"

cd "$repository_root"
OXID_PREPROD_MASTER_SEED_HEX="$preprod_master_seed_hex" \
  "$test_executable" \
  standalone_funding_tests::preprod_funded_registration_observes_dust_and_spends_shielded_night \
  --ignored --exact --nocapture
