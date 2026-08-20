#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if (($# != 2)); then
  echo "Usage: $0 <base-revision> <head-revision>" >&2
  exit 2
fi

base="$1"
head="$2"
failed=false

# Commit authors whose changes are machine-generated from repository
# configuration rather than contributed work. `dco.yml` already exempts pull
# requests *opened* by these accounts, on the stated grounds that "the
# certification the DCO records does not apply". The same reasoning applies to
# their commits wherever they appear, and it has to hold per commit as well as
# per pull request: once a bot commit lands on an integration branch, every
# later human-authored pull request whose range includes it -- a develop-to-main
# release sync, most obviously -- would otherwise fail a gate nobody can
# satisfy, because rewriting merged history to add a trailer is a worse remedy
# than the finding.
exempt_authors=(
  "dependabot[bot]"
  "renovate[bot]"
)

while IFS= read -r commit; do
  [ -n "$commit" ] || continue
  author_name="$(git show -s --format='%an' "$commit")"
  exempt=false
  for candidate in "${exempt_authors[@]}"; do
    if [ "$author_name" = "$candidate" ]; then
      exempt=true
      break
    fi
  done
  if $exempt; then
    echo "$commit skipped: authored by $author_name, exempt from DCO certification."
    continue
  fi
  identity="$(git show -s --format='%an <%ae>' "$commit")"
  if ! git show -s --format='%B' "$commit" | grep -Fqx "Signed-off-by: $identity"; then
    echo "$commit is missing 'Signed-off-by: $identity'" >&2
    failed=true
  fi
done < <(git rev-list --reverse "$base..$head")

if $failed; then
  exit 1
fi

echo "DCO sign-off check passed."
