#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
validator="$repository_root/scripts/e2e/validate-portal-headless-evidence.sh"
workflow_validator="$repository_root/scripts/e2e/validate-portal-workflow-placement.sh"
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
      helperCommit:"f7732be01171cf6a376ec0dd043f517e3f6fcf6b",
      helperTree:"96accf0da80992c3b247458c3b21f22ee9db1d68",
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
      separateJubjubAssertionBinding:true,
      sharedMidnightIdentityUnchanged:true
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
jq '.note = "Authorization: Bearer not-public"' "$valid" >"$scratch/private.json"
assert_rejected "$scratch/private.json"

"$workflow_validator" "$repository_root/.github/workflows" >/dev/null
for marker in \
  'OXID_PORTAL_EVIDENCE_PATH' \
  'OXID_PORTAL_EVIDENCE_HEAD' \
  'validate-portal-headless-evidence.sh' \
  'capture_shared_snapshot' \
  'verify_shared_snapshot' \
  'sharedMidnightIdentityUnchanged'; do
  rg -qF "$marker" scripts/e2e/portal-headless-e2e.sh || {
    echo "Local headless Portal harness is missing marker: $marker" >&2
    exit 1
  }
done
for marker in \
  'trap cleanup EXIT' \
  "trap 'exit 130' INT" \
  "trap 'exit 143' TERM" \
  'trap - EXIT' \
  "trap '' INT TERM"; do
  rg -qF "$marker" scripts/e2e/portal-headless-e2e.sh || {
    echo "Headless Portal signal cleanup marker is missing: $marker" >&2
    exit 1
  }
done
if rg -qF 'rm -f "$EVIDENCE"' scripts/e2e/portal-headless-e2e.sh; then
  echo "Headless Portal harness must preserve prior evidence until atomic replacement." >&2
  exit 1
fi
signal_probe="$scratch/headless-signal-probe.sh"
awk '
  /^cleanup\(\) \{/ { capture=1 }
  capture { print }
  capture && /^}$/ { capture=0 }
  /^trap cleanup EXIT$/ || /^trap '\''exit 130'\'' INT$/ || /^trap '\''exit 143'\'' TERM$/ { print }
' scripts/e2e/portal-headless-e2e.sh >"$signal_probe"
set +e
RAW_LOG="$scratch/headless-signal.log" bash -c '
  set -euo pipefail
  stack_started=0
  worktree_created=0
  RUN_TREE="$1/run-tree"
  SOURCE_TREE="$1/source"
  RAW_LOG="$RAW_LOG"
  source "$2"
  kill -TERM "$$"
  exit 99
' _ "$scratch" "$signal_probe" >/dev/null 2>&1
signal_status=$?
set -e
[ "$signal_status" = 143 ] || {
  echo "Headless Portal TERM cleanup did not preserve conventional status 143." >&2
  exit 1
}
printf 'Portal headless evidence schema, immutable pins, secret sentinel, local-only execution, and no-hosted-upload boundary passed.\n'
