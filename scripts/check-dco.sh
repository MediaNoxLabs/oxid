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

while IFS= read -r commit; do
  [ -n "$commit" ] || continue
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
