#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly EXPECTED_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly SOURCE_LOCK="$REPOSITORY_ROOT/fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/source-lock.json"

fail() {
  printf 'portal-source-checkout: FAIL phase=%s\n' "$1" >&2
  exit 1
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: validate-portal-source-checkout.sh <absolute-portal-checkout> [--fetch|--offline]" >&2
  exit 2
fi
source_tree="$1"
mode="${2:---fetch}"
[ "$mode" = "--fetch" ] || [ "$mode" = "--offline" ] || fail arguments
[[ "$source_tree" = /* ]] && [ -d "$source_tree" ] && [ ! -L "$source_tree" ] || fail source-path
source_tree="$(cd -- "$source_tree" && pwd -P)"

for command_name in git jq shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"
done
[ -f "$SOURCE_LOCK" ] && [ ! -L "$SOURCE_LOCK" ] || fail source-lock-file
jq -e '
  type == "object"
  and keys == ["integrationCommit", "integrationTree", "portalPrHead", "profileSourceCommit", "provenancePath", "provenanceSha256", "schema"]
  and .schema == "oxid-portal-source-lock-v2"
  and .integrationCommit == "925ec8d04882eabd4ac7b784c70fc2f0c152faae"
  and .integrationTree == "58b4597524f88a0ae2253439a44dab0dc60cbb6f"
  and .portalPrHead == "9c82db23eabe8b6d758b2731f2225910ea627c14"
  and .profileSourceCommit == "76e8edf394a4cb37ca822037272d543c68f25f71"
  and .provenancePath == "openid4vci-final/provenance.json"
  and .provenanceSha256 == "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
' "$SOURCE_LOCK" >/dev/null || fail source-lock

integration_commit="$(jq -r '.integrationCommit' "$SOURCE_LOCK")"
integration_tree="$(jq -r '.integrationTree' "$SOURCE_LOCK")"
pr_head="$(jq -r '.portalPrHead' "$SOURCE_LOCK")"
profile_source="$(jq -r '.profileSourceCommit' "$SOURCE_LOCK")"
provenance_path="$(jq -r '.provenancePath' "$SOURCE_LOCK")"
provenance_sha="$(jq -r '.provenanceSha256' "$SOURCE_LOCK")"

[ "$(git -C "$source_tree" remote get-url origin 2>/dev/null || true)" = "$EXPECTED_REMOTE" ] || fail source-remote
[ -z "$(git -C "$source_tree" status --porcelain 2>/dev/null || printf invalid)" ] || fail source-dirty
if [ "$mode" = "--fetch" ]; then
  if ! git -C "$source_tree" fetch --no-tags origin \
    "+$integration_commit:refs/oxid-evidence/portal-integration" \
    "+refs/pull/17/head:refs/oxid-evidence/portal-pr-17" \
    "+$profile_source:refs/oxid-evidence/portal-profile-source" \
    >/dev/null 2>&1; then
    fail source-fetch
  fi
fi

[ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-integration^{commit} 2>/dev/null || true)" = "$integration_commit" ] || fail integration-commit
[ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-integration^{tree} 2>/dev/null || true)" = "$integration_tree" ] || fail integration-tree
[ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-pr-17^{commit} 2>/dev/null || true)" = "$pr_head" ] || fail pr-head
[ "$(git -C "$source_tree" rev-parse refs/oxid-evidence/portal-pr-17^{tree} 2>/dev/null || true)" = "$integration_tree" ] || fail pr-tree
[ "$(git -C "$source_tree" rev-parse "$profile_source"^{commit} 2>/dev/null || true)" = "$profile_source" ] || fail profile-source
provenance_file="crates/issuer-integration/fixtures/$provenance_path"
[ "$(git -C "$source_tree" show "$integration_commit:$provenance_file" 2>/dev/null | shasum -a 256 | awk '{print $1}')" = "$provenance_sha" ] || fail provenance
[ -z "$(git -C "$source_tree" status --porcelain 2>/dev/null || printf invalid)" ] || fail source-mutated
printf 'portal-source-checkout: PASS\n'
