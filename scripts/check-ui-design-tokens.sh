#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v rg >/dev/null 2>&1; then
  echo "rg is required; run this check from 'nix develop'." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
stylesheet="$repo_root/crates/ui-dioxus/assets/styles.css"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT

required_tokens='surface-0 surface-1 surface-2 surface-3 surface-4 surface-raised surface-sheet
text-strong text text-soft text-muted
accent accent-alt on-accent positive warning critical info
family-assets family-identity family-vault line line-strong
font-display font-title font-body font-label font-caption font-numeral
space-1 space-2 space-3 space-4 space-5 space-6 space-7 space-8
radius-card radius-control radius-pill
motion-fast motion-base motion-slow shadow-card shadow-sheet'

for token in $required_tokens; do
  if ! rg --quiet --fixed-strings -- "--${token}:" "$stylesheet"; then
    echo "Required semantic UI token is missing: --${token}" >&2
    exit 1
  fi
done

for token in \
  brand-dark-surface-0 brand-dark-surface-1 brand-dark-surface-2 \
  brand-dark-surface-3 brand-dark-surface-4 brand-dark-text-strong \
  brand-dark-text brand-dark-text-soft brand-dark-text-muted \
  brand-light-surface-0 brand-light-surface-1 brand-light-surface-2 \
  brand-light-surface-3 brand-light-surface-4 brand-light-text-strong \
  brand-light-text brand-light-text-soft brand-light-text-muted; do
  if ! rg --quiet --fixed-strings -- "--${token}:" "$stylesheet"; then
    echo "Required dark/light brand token is missing: --${token}" >&2
    exit 1
  fi
done

# Raw palette values are permitted only in the explicit token definition
# block. Component selectors must consume semantic variables or color-mix.
awk '
  /OXID DESIGN TOKENS START/ { in_tokens = 1; next }
  /OXID DESIGN TOKENS END/ { in_tokens = 0; next }
  !in_tokens { print }
' "$stylesheet" >"$scratch/component.css"

raw_colors="$({
  rg --line-number --pcre2 '#[0-9A-Fa-f]{3,8}\b|rgba?\(' "$scratch/component.css" || true
})"
if [[ -n "$raw_colors" ]]; then
  echo "Raw color literals outside the design-token block:" >&2
  echo "$raw_colors" >&2
  exit 1
fi

legacy_tokens="$({
  rg --line-number --pcre2 -- '--(?:cyan|purple|green|error|text-faint|gradient-primary)\b' "$stylesheet" || true
})"
if [[ -n "$legacy_tokens" ]]; then
  echo "Legacy presentation tokens bypass the semantic vocabulary:" >&2
  echo "$legacy_tokens" >&2
  exit 1
fi

brand_bypass="$({
  rg --line-number --pcre2 -- 'var\(--(?:brand|fixed)-' "$scratch/component.css" || true
})"
if [[ -n "$brand_bypass" ]]; then
  echo "Components must not bypass semantic tokens with brand/fixed primitives:" >&2
  echo "$brand_bypass" >&2
  exit 1
fi

raw_type="$({
  rg --line-number 'font-size:' "$scratch/component.css" |
    rg --invert-match 'font-size:[[:space:]]*var\(--font-' || true
})"
if [[ -n "$raw_type" ]]; then
  echo "Component font sizes must use the six-step type scale:" >&2
  echo "$raw_type" >&2
  exit 1
fi

raw_radius="$({
  rg --line-number 'border-radius:' "$scratch/component.css" |
    rg --invert-match 'border-radius:[[:space:]]*(var\(--radius-|inherit)' || true
})"
if [[ -n "$raw_radius" ]]; then
  echo "Component radii must use the card/control/pill scale:" >&2
  echo "$raw_radius" >&2
  exit 1
fi

raw_motion="$({
  rg --line-number --pcre2 '(?:animation|transition):[^;]*(?:[0-9]+ms|[0-9]+(?:\.[0-9]+)?s)' "$scratch/component.css" || true
})"
if [[ -n "$raw_motion" ]]; then
  echo "Component motion durations must use motion tokens:" >&2
  echo "$raw_motion" >&2
  exit 1
fi

echo "UI brand and semantic token boundaries are complete."
