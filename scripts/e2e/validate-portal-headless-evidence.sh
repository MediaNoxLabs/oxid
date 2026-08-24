#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly HELPER_COMMIT="8915760a4523d282fa07d45a48b7f58e4287bb54"
readonly HELPER_TREE="1317e109cf0792c0e1d7c8f9e2b8857251f6e92d"
readonly INTEGRATION_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly INTEGRATION_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly PROVENANCE_SHA="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"

if [ "$#" -ne 2 ]; then
  echo "usage: validate-portal-headless-evidence.sh <evidence.json> <expected-oxid-head>" >&2
  exit 2
fi

evidence="$1"
expected_head="$2"
fail() {
  printf 'portal-headless-evidence: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[[ "$expected_head" =~ ^[0-9a-f]{40}$ ]] || fail expected-head
[ -f "$evidence" ] && [ ! -L "$evidence" ] || fail evidence-file
[ "$(wc -c <"$evidence" | tr -d ' ')" -le 16384 ] || fail evidence-size
command -v jq >/dev/null 2>&1 || fail missing-jq
command -v rg >/dev/null 2>&1 || fail missing-rg

jq -e \
  --arg head "$expected_head" \
  --arg helperCommit "$HELPER_COMMIT" \
  --arg helperTree "$HELPER_TREE" \
  --arg integrationCommit "$INTEGRATION_COMMIT" \
  --arg integrationTree "$INTEGRATION_TREE" \
  --arg prHead "$PR_HEAD" \
  --arg profileSource "$PROFILE_SOURCE" \
  --arg provenance "$PROVENANCE_SHA" '
    type == "object"
    and keys == ["acceptance", "oxid", "portal", "schema"]
    and .schema == "oxid-portal-headless-evidence-v1"
    and (.oxid | type == "object" and keys == ["head"] and .head == $head)
    and (.portal | type == "object"
      and keys == ["helperCommit", "helperTree", "integrationCommit", "integrationTree", "prHead", "profileSourceCommit", "provenanceSha256"]
      and .helperCommit == $helperCommit
      and .helperTree == $helperTree
      and .integrationCommit == $integrationCommit
      and .integrationTree == $integrationTree
      and .prHead == $prHead
      and .profileSourceCommit == $profileSource
      and .provenanceSha256 == $provenance)
    and (.acceptance | type == "object"
      and keys == ["confirmationRequired", "encryptedPersistence", "exactBundleImported", "managedAuthenticationProof", "mockKycApproved", "newProcessRestore", "refusalWithoutSecretCalls", "replayRejected", "reverified", "separateJubjubAssertionBinding", "sharedMidnightIdentityUnchanged"]
      and all(.[]; . == true))
  ' "$evidence" >/dev/null || fail evidence-schema

if rg -qi \
  'openid-credential-offer|access[_-]?token|pre-authorized|c_nonce|eyJ|did:|https?://|AB1234567|John|Doe|private.?parts|signed.?bytes|detached.?proof|portal-offer-capability|authorization:[[:space:]]*bearer' \
  "$evidence"; then
  fail secret-sentinel
fi

printf 'portal-headless-evidence: PASS file=%s\n' "$evidence"
