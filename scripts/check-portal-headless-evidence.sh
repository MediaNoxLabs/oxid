#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
validator="$repository_root/scripts/e2e/validate-portal-headless-evidence.sh"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
head="0123456789abcdef0123456789abcdef01234567"
valid="$scratch/valid.json"

jq -cn \
  --arg head "$head" \
  '{
    schema:"oxid-portal-headless-evidence-v1",
    oxid:{head:$head},
    portal:{
      integrationCommit:"925ec8d04882eabd4ac7b784c70fc2f0c152faae",
      integrationTree:"58b4597524f88a0ae2253439a44dab0dc60cbb6f",
      prHead:"9c82db23eabe8b6d758b2731f2225910ea627c14",
      profileSourceCommit:"76e8edf394a4cb37ca822037272d543c68f25f71",
      provenanceSha256:"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
    },
    acceptance:{
      confirmationRequired:true,
      encryptedPersistence:true,
      exactBundleImported:true,
      managedAuthenticationProof:true,
      mockKycApproved:true,
      newProcessRestore:true,
      refusalWithoutSecretCalls:true,
      replayRejected:true,
      reverified:true,
      separateJubjubAssertionBinding:true
    }
  }' >"$valid"
"$validator" "$valid" "$head" >/dev/null

assert_rejected() {
  local candidate="$1"
  if "$validator" "$candidate" "$head" >/dev/null 2>&1; then
    echo "Headless evidence validator accepted an invalid fixture: $candidate" >&2
    exit 1
  fi
}

jq '.extra = true' "$valid" >"$scratch/extra.json"
assert_rejected "$scratch/extra.json"
jq '.acceptance.reverified = false' "$valid" >"$scratch/false.json"
assert_rejected "$scratch/false.json"
jq '.oxid.head = "ffffffffffffffffffffffffffffffffffffffff"' "$valid" >"$scratch/wrong-head.json"
assert_rejected "$scratch/wrong-head.json"
jq '.portal.integrationCommit = "ffffffffffffffffffffffffffffffffffffffff"' "$valid" >"$scratch/wrong-pin.json"
assert_rejected "$scratch/wrong-pin.json"
jq '.secret = "Authorization: Bearer not-public"' "$valid" >"$scratch/secret.json"
assert_rejected "$scratch/secret.json"

workflow="$repository_root/.github/workflows/ci.yml"
job="$(awk '
  /^  portal-headless-e2e:/ { capture=1 }
  capture && /^  [A-Za-z0-9_-]+:/ && $0 !~ /^  portal-headless-e2e:/ { exit }
  capture { print }
' "$workflow")"
for marker in \
  'timeout-minutes: 45' \
  'OXID_EVIDENCE_HEAD: ${{ github.event.pull_request.head.sha || github.sha }}' \
  'ref: ${{ env.OXID_EVIDENCE_HEAD }}' \
  '"$OXID_EVIDENCE_HEAD"' \
  'actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803' \
  'ref: 925ec8d04882eabd4ac7b784c70fc2f0c152faae' \
  'PORTAL_SOURCE_TOKEN: ${{ secrets.PORTAL_SOURCE_TOKEN }}' \
  'if [ -z "${PORTAL_SOURCE_TOKEN:-}" ]; then' \
  'Required repository secret PORTAL_SOURCE_TOKEN is not configured' \
  'token: ${{ secrets.PORTAL_SOURCE_TOKEN }}' \
  'persist-credentials: false' \
  'PORTAL_SOURCE_TREE: ${{ github.workspace }}/portal-source' \
  'nix develop --command just portal-headless-e2e' \
  'validate-portal-headless-evidence.sh' \
  'actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02' \
  'path: target/portal-headless-e2e/evidence.json' \
  'if-no-files-found: error' \
  'retention-days: 7' \
  'OXID_PORTAL_KEEP_FAILURE_LOG: "0"'; do
  grep -qF "$marker" <<<"$job" || {
    echo "Hosted Portal headless job is missing contract marker: $marker" >&2
    exit 1
  }
done
[ "$(grep -cF 'token: ${{ secrets.PORTAL_SOURCE_TOKEN }}' <<<"$job")" -eq 1 ] || {
  echo "Hosted Portal headless job must use the private source token exactly once." >&2
  exit 1
}
[ "$(grep -cF 'path: target/portal-headless-e2e/evidence.json' <<<"$job")" -eq 1 ] || {
  echo "Hosted Portal headless job must upload exactly one sanitized evidence path." >&2
  exit 1
}
if grep -qF '/Users/' scripts/e2e/portal-headless-e2e.sh || \
  ! grep -qF '+$INTEGRATION_COMMIT:refs/oxid-evidence/portal-integration' \
    scripts/e2e/portal-headless-e2e.sh; then
  echo "Portal headless reproduction must use an explicit source path and immutable integration ref." >&2
  exit 1
fi
printf 'Portal headless evidence publication schema and secret sentinel passed.\n'
