#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v rg >/dev/null 2>&1; then
  echo "rg is required; run this check from 'nix develop'." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ui_source_root="${OXID_UI_SOURCE_ROOT:-$repo_root/crates/ui-dioxus/src}"
stylesheet="${OXID_UI_STYLESHEET:-$repo_root/crates/ui-dioxus/assets/styles.css}"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT

ui_sources=()
while IFS= read -r -d '' source; do
  ui_sources+=("$source")
done < <(find "$ui_source_root" -type f -name '*.rs' -print0)

if [[ ${#ui_sources[@]} -eq 0 ]]; then
  echo "No Dioxus Rust sources found under $ui_source_root." >&2
  exit 1
fi

rg --no-filename -o 'class: "[^"]+"' "${ui_sources[@]}" |
  sed -E 's/^class: "([^"]+)"$/\1/' |
  tr ' ' '\n' |
  sed '/^$/d' |
  rg -v '[{}]' |
  sort -u >"$scratch/used"

rg --no-filename -o '\.[A-Za-z_][A-Za-z0-9_-]*' "$stylesheet" |
  sed 's/^\.//' |
  sort -u >"$scratch/defined"

missing="$(comm -23 "$scratch/used" "$scratch/defined")"
if [[ -n "$missing" ]]; then
  echo "Static Dioxus class literals without stylesheet selectors:" >&2
  echo "$missing" >&2
  exit 1
fi

echo "Static Dioxus class literals have stylesheet selectors."
