#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required; run this check from 'nix develop'." >&2
  exit 1
fi

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --no-deps --format-version 1 >"$metadata_file"

dependency_count=0
while IFS=$'\t' read -r package source path; do
  [ -n "$package" ] || continue
  dependency_count=$((dependency_count + 1))

  case "$package" in
    midnight-ledger|midnight-zswap|midnight-zkir|midnight-onchain-runtime|\
      midnight-serialize|midnight-base-crypto|midnight-coin-structure|\
      midnight-onchain-state|midnight-storage|midnight-transient-crypto|\
      midnight-proof-server)
      expected_source="git+https://github.com/midnightntwrk/midnight-ledger.git?rev="
      ;;
    midnight-proofs|midnight-circuits|midnight-zk-stdlib|midnight-curves)
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

  if [[ "$source" != "$expected_source"* ]]; then
    echo "$package must use $expected_source<full-commit-sha>." >&2
    exit 1
  fi

  revision="${source#"$expected_source"}"
  revision="${revision%%#*}"
  revision="${revision%%&*}"
  if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
    echo "$package must pin a full 40-character Git commit in 'rev' (found '$revision')." >&2
    exit 1
  fi
done < <(
  jq -r '
    .packages[].dependencies[]
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
        "midnight-circuits",
        "midnight-zk-stdlib",
        "midnight-curves"
      ] | index($name))
    | [.name, (.source // ""), (.path // "")]
    | @tsv
  ' "$metadata_file"
)

if [ "$dependency_count" = "0" ]; then
  echo "Midnight Git source rules passed (no M2 dependencies selected)."
else
  echo "Midnight Git source rules passed for $dependency_count direct dependencies."
fi
