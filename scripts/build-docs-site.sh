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

if ! command -v mdbook >/dev/null 2>&1; then
  echo "mdbook is required; run this target from 'nix develop .#docs' (or the default shell)." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
site_dir="$repo_root/docs/site"
adr_index="$repo_root/docs/adr/README.md"
catalog="$site_dir/src/adr-catalog.md"
node "$repo_root/scripts/docs/generate-adr-catalog.mjs" \
  --index "$adr_index" --output "$catalog"

mdbook build "$site_dir"
echo "Site built at $site_dir/book (index: $site_dir/book/index.html)."
