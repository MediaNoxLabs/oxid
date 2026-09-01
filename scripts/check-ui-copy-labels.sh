#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v rg >/dev/null 2>&1; then
  echo "rg is required; run this check from 'nix develop'." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ui_source_root="${OXID_UI_SOURCE_ROOT:-$repo_root/crates/ui-dioxus/src}"
labels="${OXID_UI_LABELS:-$repo_root/crates/ui-dioxus/src/labels.rs}"

if [[ ! -f "$labels" ]]; then
  echo "The central Dioxus labeling module is missing." >&2
  exit 1
fi

ui_sources=()
while IFS= read -r -d '' source; do
  if [[ "$source" != "$labels" ]]; then
    ui_sources+=("$source")
  fi
done < <(find "$ui_source_root" -type f -name '*.rs' -print0)

if [[ ${#ui_sources[@]} -eq 0 ]]; then
  echo "No Dioxus Rust sources found under $ui_source_root." >&2
  exit 1
fi

reject() {
  local pattern="$1"
  local message="$2"
  local matches
  matches="$({ rg --line-number --pcre2 "$pattern" "${ui_sources[@]}" || true; })"
  if [[ -n "$matches" ]]; then
    echo "$message" >&2
    echo "$matches" >&2
    exit 1
  fi
}

reject "replace\\([[:space:]]*'_'" \
  "Ad-hoc underscore replacement bypasses the reviewed UI labeling boundary:"
reject '(?i)base units|atomic units' \
  "User-facing asset copy must use exact NIGHT/DUST formatting:"
reject 'event \{current\} of \{target\}' \
  "Synchronization copy must not expose adapter cursors:"
reject '"\{[^"}]*\.(?:state|source|mode|format|status|direction|privacy_tier|operation|outcome|intent|failure_code|reason_code|network|schema_id|curve)\}"' \
  "A machine-valued field is interpolated directly into Dioxus copy:"
reject '\.(?:state|source|mode|format|status|direction|privacy_tier|operation|outcome|intent|failure_code|reason_code)\.replace\(' \
  "A machine-valued field is transformed outside the central label module:"
reject '"\{timestamp\}[[:space:]]*ms"' \
  "Unix milliseconds must use the reviewed UTC formatter:"
reject 'let[[:space:]]+(?:source|mode|status|state|format)[[:space:]]*=[^;]*\.(?:source|mode|status|state|format)\.clone\(\)' \
  "A raw machine-valued alias could bypass label review:"

if rg --quiet --pcre2 'value[[:space:]]*=>[[:space:]]*value' "$labels"; then
  echo "Unknown machine values must never be echoed by the UI label module." >&2
  exit 1
fi

required_values='deterministic_simulation canonical_finalized_replay indexer_supplied_not_proven
outcome_unknown proof_unavailable midnight_compact_vc local_preview_ready
never_synced cancellation_requested partially_applied midnight_cbor_phase1
selective_disclosure predicate_only owner_private_atomic_file
digital-passport:v1 undeployed preprod pinned_contract_layout
node_anchored_indexer finalized_node_replay'

for value in $required_values; do
  if ! rg --quiet --fixed-strings "\"${value}\"" "$labels"; then
    echo "Required user-facing machine value is not covered by labels.rs: ${value}" >&2
    exit 1
  fi
done

for formatter in format_asset_amount format_epoch_millis parse_night_amount; do
  if ! rg --quiet --fixed-strings "fn ${formatter}" "$labels"; then
    echo "Required presentation formatter is missing: ${formatter}" >&2
    exit 1
  fi
done

echo "Dioxus machine values cross the reviewed labeling and formatting boundary."
