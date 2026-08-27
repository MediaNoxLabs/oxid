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
blob_base="https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr"
adr_0104_permalink_base="https://github.com/MediaNoxLabs/oxid/blob/233eda9d6c80e8554d83b4b1160b224da5a5ed65/docs/adr"

{
  echo "# Decision records"
  echo
  echo "> Regenerated at build time from [\`docs/adr/README.md\`]($blob_base/README.md)."
  echo
  # Rewrite relative ADR links (NNNN-slug.md) to absolute GitHub blob URLs
  # and demote the source file's H1 so the chapter keeps a single title.
  sed -E \
    -e 's/^# /## /' \
    -e "s|\\]\\((([0-9]{4})[A-Za-z0-9./_-]*\\.md)\\)|](${blob_base}/\\1)|g" \
    -e "s|${blob_base}/(0104-[A-Za-z0-9._/-]*\\.md)|${adr_0104_permalink_base}/\\1|g" \
    "$adr_index"
} > "$catalog"

mdbook build "$site_dir"
echo "Site built at $site_dir/book (index: $site_dir/book/index.html)."
