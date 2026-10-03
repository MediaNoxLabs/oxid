#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Builds the documentation site under docs/site/book.
#
# The ADR catalog chapter is regenerated from docs/adr/README.md — the
# authoritative index — with its relative links rewritten to the GitHub
# blob URLs, so the site never carries a second, drifting copy of the
# catalog.

set -euo pipefail

for command in mdbook node; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required; run this target from 'nix develop .#docs' (or the default shell)." >&2
    exit 1
  fi
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
site_dir="$repo_root/docs/site"
adr_index="$repo_root/docs/adr/README.md"
catalog="$site_dir/src/adr-catalog.md"
node "$repo_root/scripts/docs/generate-adr-catalog.mjs" \
  --index "$adr_index" --output "$catalog"

mdbook build "$site_dir"
if ! grep -qF 'href="adr-catalog.html#adr-0078"' "$site_dir/book/portable-custody-kdf.html" \
  || ! grep -qF 'id="adr-0078"' "$site_dir/book/adr-catalog.html"; then
  echo "Rendered portable-custody ADR-0078 link or target is missing." >&2
  exit 1
fi
echo "Site built at $site_dir/book (index: $site_dir/book/index.html)."
