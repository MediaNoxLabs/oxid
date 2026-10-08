#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required; run this check from 'nix develop'." >&2
  exit 1
fi

readonly ledger_revision="8655615e7c4cbf3a1187b3203bf72e69dc09b8dc"
readonly ledger_source="git+https://github.com/MediaNoxLabs/midnight-ledger.git?rev=${ledger_revision}"
readonly proofs_revision="532629b044a88473a7175f4a96c2511c91156136"
readonly proofs_source="git+https://github.com/MediaNoxLabs/midnight-zk?rev=${proofs_revision}"
readonly compact_revision="32e314770da10cc62dab30dcfeb340ec4c6bcb64"
readonly compact_source="git+https://github.com/MediaNoxLabs/compact.git?rev=${compact_revision}"
readonly identity_revision="127d4c48541ba0d28a4f34bfb5ffb4af65df7c42"
readonly identity_source="git+https://github.com/MediaNoxLabs/midnight-identity.git?rev=${identity_revision}"

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --locked --format-version 1 >"$metadata_file"

dependency_count=0
while IFS=$'\t' read -r package source path; do
  [ -n "$package" ] || continue
  dependency_count=$((dependency_count + 1))

  case "$package" in
    midnight-ledger|midnight-zswap|midnight-zkir|midnight-onchain-runtime|\
      midnight-serialize|midnight-base-crypto|midnight-coin-structure|\
      midnight-onchain-state|midnight-storage|midnight-transient-crypto|\
      midnight-proof-server)
      expected_source="$ledger_source"
      ;;
    midnight-proofs)
      expected_source="$proofs_source"
      ;;
    midnight-compact-runtime)
      expected_source="$compact_source"
      ;;
    midnight-did-domain|midnight-did-jubjub-schnorr|midnight-did-runtime)
      expected_source="$identity_source"
      ;;
    midnight-circuits|midnight-zk-stdlib|midnight-curves)
      expected_source="git+https://github.com/midnightntwrk/midnight-zk.git?rev="
      ;;
    *)
      continue
      ;;
  esac

  if [ -n "$path" ]; then
    echo "$package must not use a local path dependency ($path)." >&2
    exit 1
  fi

  if [[ "$source" != "$expected_source" ]]; then
    echo "$package must use the reviewed immutable source $expected_source." >&2
    exit 1
  fi

  revision="${source#*?rev=}"
  revision="${revision%%#*}"
  revision="${revision%%&*}"
  if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
    echo "$package must pin a full 40-character Git commit in 'rev' (found '$revision')." >&2
    exit 1
  fi
done < <(
  jq -r '
    [.workspace_members[] as $member | .packages[] | select(.id == $member) | .dependencies[]]
    | .[]
    | select(.name as $name | [
        "midnight-ledger",
        "midnight-zswap",
        "midnight-zkir",
        "midnight-onchain-runtime",
        "midnight-serialize",
        "midnight-base-crypto",
        "midnight-coin-structure",
        "midnight-onchain-state",
        "midnight-storage",
        "midnight-transient-crypto",
        "midnight-proof-server",
        "midnight-proofs",
        "midnight-compact-runtime",
        "midnight-did-domain",
        "midnight-did-jubjub-schnorr",
        "midnight-did-runtime",
        "midnight-circuits",
        "midnight-zk-stdlib",
        "midnight-curves"
      ] | index($name))
    | [.name, (.source // ""), (.path // "")]
    | @tsv
  ' "$metadata_file"
)

ledger_graph_count="$(jq -r --arg source "$ledger_source" '
  [.packages[]
   | select(.name | test("^midnight-(ledger(?:-static)?|zswap|zkir|onchain-.+|storage.*|serialize.*|transient-crypto|base-crypto(?:-derive)?|coin-structure)$"))
   | select(.source | startswith($source))]
  | length
' "$metadata_file")"
if [ "$ledger_graph_count" = "0" ]; then
  echo "The locked graph contains no packages from the reviewed Ledger8 source." >&2
  exit 1
fi

unexpected_ledger_sources="$(jq -r --arg source "$ledger_source" '
  .packages[]
  | select(.name | test("^midnight-(ledger(?:-static)?|zswap|zkir|onchain-.+|storage.*|serialize.*|transient-crypto|base-crypto(?:-derive)?|coin-structure)$"))
  | select((.source | startswith($source)) | not)
  | "\(.name) \(.version) \(.source // "path")"
' "$metadata_file")"
if [ -n "$unexpected_ledger_sources" ]; then
  echo "Locked Ledger-family packages must all resolve from $ledger_source:" >&2
  echo "$unexpected_ledger_sources" >&2
  exit 1
fi

duplicate_ledger_packages="$(jq -r '
  [.packages[]
   | select(.name | test("^midnight-(ledger(?:-static)?|zswap|zkir|onchain-.+|storage.*|serialize.*|transient-crypto|base-crypto(?:-derive)?|coin-structure)$"))]
  | group_by(.name)
  | .[]
  | select(length > 1)
  | map("\(.name) \(.version) \(.source // "path")")
  | join("; ")
' "$metadata_file")"
if [ -n "$duplicate_ledger_packages" ]; then
  echo "Locked graph contains duplicate Ledger-family packages: $duplicate_ledger_packages" >&2
  exit 1
fi

if ! jq -e --arg source "$proofs_source" '
  [.packages[] | select(.name == "midnight-proofs" and (.source | startswith($source)))]
  | length == 1
' "$metadata_file" >/dev/null; then
  echo "midnight-proofs must resolve once from the exact patch declared by the selected Ledger8 workspace." >&2
  exit 1
fi

if ! jq -e --arg compact "$compact_source" --arg identity "$identity_source" '
  ([.packages[]
    | select(.name == "midnight-compact-runtime")
    | select(.source | startswith($compact))] | length == 1)
  and
  ([.packages[]
    | select(.name | test("^midnight-did-(domain|jubjub-schnorr|method|runtime)$"))]
   | length > 0
   and all(.source | startswith($identity)))
' "$metadata_file" >/dev/null; then
  echo "Compact runtime and Midnight DID packages must resolve once from their reviewed immutable sources." >&2
  exit 1
fi

if ! jq -e '
  [.packages[] | select(.name == "midnight-ledger" or .name == "midnight-zswap")]
  | all(.version == "8.2.0-rc.1")
' "$metadata_file" >/dev/null; then
  echo "Ledger9 is out of scope: midnight-ledger and midnight-zswap must remain on 8.2.0-rc.1." >&2
  exit 1
fi

if [ "$dependency_count" = "0" ]; then
  echo "Midnight Git source rules passed (no direct Midnight Cargo packages present)."
else
  echo "Midnight Git source rules passed for $dependency_count direct dependencies and $ledger_graph_count locked Ledger8 packages."
fi
