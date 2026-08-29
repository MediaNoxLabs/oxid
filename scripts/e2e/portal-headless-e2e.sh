#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly PORTAL_PROVENANCE_SHA256="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly EVIDENCE="${OXID_PORTAL_EVIDENCE_PATH:-$REPO_ROOT/target/portal-headless-e2e/evidence.json}"
readonly RUN_TREE="${TMPDIR:-/tmp}/oxid-portal-source-$$"
readonly RAW_LOG="${TMPDIR:-/tmp}/oxid-portal-headless-$$.log"
readonly SOURCE_INPUT="${OXID_PORTAL_SOURCE_REPOSITORY:-$PORTAL_REMOTE}"

cleanup() {
  local status=$? portal_state
  portal_state="$(dirname -- "$EVIDENCE")/runtime/portal-state"
  if [ -f "$portal_state/owner-receipt.json" ] && [ -d "$RUN_TREE" ]; then
    PORTAL_INTEGRATION_CHECKOUT="$RUN_TREE" \
    OXID_PORTAL_CONSUMER_STATE_DIR="$portal_state" \
      "$REPO_ROOT/scripts/portal-consumer-lifecycle.sh" down \
      >>"$RAW_LOG" 2>&1 || status=1
  fi
  rm -rf -- "$RUN_TREE"
  if [ "$status" -ne 0 ] && [ "${OXID_PORTAL_KEEP_FAILURE_LOG:-0}" = 1 ]; then
    chmod 600 "$RAW_LOG" 2>/dev/null || true
    printf 'portal-headless-e2e: private failure log retained\n' >&2
  else
    rm -f -- "$RAW_LOG"
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

fail() {
  printf 'portal-headless-e2e: FAIL phase=%s\n' "$1" >&2
  exit 1
}

for command_name in cargo docker git grep jq nix shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
docker info >/dev/null 2>&1 || fail docker
[ -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
readonly OXID_HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
readonly OXID_TREE="$(git -C "$REPO_ROOT" rev-parse HEAD^{tree})"
[[ "$OXID_HEAD" =~ ^[0-9a-f]{40}$ ]] || fail oxid-head
[[ "$OXID_TREE" =~ ^[0-9a-f]{40}$ ]] || fail oxid-tree

umask 077
: >"$RAW_LOG"
if ! git clone --no-checkout "$SOURCE_INPUT" "$RUN_TREE" >>"$RAW_LOG" 2>&1; then
  fail source-clone
fi
git -C "$RUN_TREE" remote set-url origin "$PORTAL_REMOTE"
if ! git -C "$RUN_TREE" fetch origin integration >>"$RAW_LOG" 2>&1; then
  fail source-fetch
fi
[ "$(git -C "$RUN_TREE" rev-parse FETCH_HEAD^{commit})" = "$PORTAL_COMMIT" ] || fail portal-commit
[ "$(git -C "$RUN_TREE" rev-parse FETCH_HEAD^{tree})" = "$PORTAL_TREE" ] || fail portal-tree
git -C "$RUN_TREE" checkout --detach "$PORTAL_COMMIT" >>"$RAW_LOG" 2>&1
[ -z "$(git -C "$RUN_TREE" status --porcelain --untracked-files=all)" ] || fail source-dirty
provenance_path="crates/issuer-integration/fixtures/openid4vci-final/provenance.json"
[ "$(git -C "$RUN_TREE" show "$PORTAL_COMMIT:$provenance_path" | shasum -a 256 | awk '{print $1}')" = "$PORTAL_PROVENANCE_SHA256" ] || fail portal-provenance
[ -x "$RUN_TREE/scripts/tailscale-https-profile.sh" ] || fail tailscale-profile
grep -F 'DiditHttpAdapter::new(&config.didit_api_key, &config.didit_api_base_url)' \
  "$RUN_TREE/crates/issuer-bin/src/lib.rs" >/dev/null || fail portal-rust-didit-adapter
grep -F 'DigitalPassportRustAdapter::new(' "$RUN_TREE/crates/issuer-bin/src/lib.rs" \
  >/dev/null || fail portal-rust-issuer
grep -F 'default      = [ "rust" ]' "$RUN_TREE/crates/issuer-bin/Cargo.toml" \
  >/dev/null || fail portal-rust-default-feature
grep -F 'cargoBuildCommand = "cargo build --release --package laceid-issuer-bin";' \
  "$RUN_TREE/nix/packages/issuer/issuer.nix" >/dev/null || fail portal-rust-image-build
grep -F 'DIDIT_API_BASE_URL: http://smocker:8080' \
  "$RUN_TREE/docker/docker-compose.yml" >/dev/null || fail portal-supported-mock
grep -F 'mock/didit.yml' "$RUN_TREE/justfile" >/dev/null || fail portal-mock-loader
grep -F 'sessionStorage.setItem(CREDENTIAL_OFFER_URI_KEY, data.credentialOfferUri);' \
  "$RUN_TREE/crates/issuer-http/web/issue/index.html" >/dev/null || fail portal-offer-capture
grep -F 'var credentialOfferUri = sessionStorage.getItem(CREDENTIAL_OFFER_URI_KEY);' \
  "$RUN_TREE/crates/issuer-http/web/issue/complete.html" >/dev/null || fail portal-offer-display
grep -F 'qr.addData(offerUri);' "$RUN_TREE/crates/issuer-http/web/issue/complete.html" \
  >/dev/null || fail portal-offer-qr
grep -F 'DIDIT_API_BASE_URL: http://smocker:8080' \
  "$REPO_ROOT/scripts/portal-consumer-stack.yml" >/dev/null || fail consumer-mock-route
if ! PORTAL_INTEGRATION_CHECKOUT="$RUN_TREE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$(dirname -- "$EVIDENCE")/runtime/portal-state" \
    "$REPO_ROOT/scripts/portal-consumer-lifecycle.sh" prerequisite \
      >>"$RAW_LOG" 2>&1; then
  fail shared-midnight-prerequisite
fi

rm -f -- "$EVIDENCE"
if ! PORTAL_INTEGRATION_TREE="$RUN_TREE" \
  PORTAL_CONSUMER_LIFECYCLE="$REPO_ROOT/scripts/portal-consumer-lifecycle.sh" \
  OXID_PORTAL_EVIDENCE_PATH="$EVIDENCE" \
  OXID_PORTAL_EVIDENCE_HEAD="$OXID_HEAD" \
  OXID_PORTAL_EVIDENCE_TREE="$OXID_TREE" \
  cargo test --manifest-path "$REPO_ROOT/Cargo.toml" -p oxid-headless \
    --test portal_live_flow \
    lace_portal_mock_flow_issues_to_same_headless_process_and_restores \
    -- --ignored --exact >>"$RAW_LOG" 2>&1; then
  fail live-flow
fi

[ -f "$EVIDENCE" ] || fail evidence
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail cleanup
if ! PORTAL_INTEGRATION_CHECKOUT="$RUN_TREE" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$(dirname -- "$EVIDENCE")/runtime/portal-state" \
    "$REPO_ROOT/scripts/portal-consumer-lifecycle.sh" prerequisite \
      >>"$RAW_LOG" 2>&1; then
  fail shared-midnight-postcondition
fi
if grep -Eqi \
  'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|Alice|Example|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|proof.?jwt|capability|seed|"(route|did|claim|grant|token|nonce|credential|proof|private|log|pid|timestamp|path)"[[:space:]]*:' \
  "$EVIDENCE"; then
  fail evidence-denylist
fi
jq -e \
  --arg head "$OXID_HEAD" --arg oxid_tree "$OXID_TREE" \
  --arg commit "$PORTAL_COMMIT" --arg portal_tree "$PORTAL_TREE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" '
  keys == [
    "acceptance",
    "diditProviderMode",
    "headlessIndexerCurrentCursor",
    "headlessIndexerTargetCursor",
    "issuerImplementation",
    "midnightInteractionProven",
    "nodeInteractionProven",
    "oxid",
    "portal",
    "portalServiceExercised",
    "proofServerInteractionProven",
    "schema"
  ]
  and .schema == "oxid-phase1-lace-portal-journey-v1"
  and .oxid == {head:$head,tree:$oxid_tree}
  and .portal == {
    integrationCommit:$commit,
    integrationTree:$portal_tree,
    provenanceSha256:$provenance
  }
  and .portalServiceExercised == true
  and .issuerImplementation == "lace-id-portal-rust"
  and .diditProviderMode == "lace-smocker"
  and .midnightInteractionProven == "oxid-headless-indexer-sync"
  and .nodeInteractionProven == false
  and .proofServerInteractionProven == false
  and (.headlessIndexerCurrentCursor | numbers)
  and (.headlessIndexerTargetCursor | numbers)
  and .headlessIndexerCurrentCursor == .headlessIndexerTargetCursor
  and (.acceptance | keys) == [
    "digitalPassportVerified",
    "encryptedPersistence",
    "exactQrOfferUrlRouted",
    "explicitConsent",
    "freshReverification",
    "issuerCallsBlockedBeforeConsent",
    "issuerDidBootstrappedAndResolved",
    "laceDiditMockExercised",
    "listing",
    "managedAuthentication",
    "noExternalProviderCall",
    "pendingIssuancePreservedAcrossSync",
    "restartRestoration",
    "sameProcessIssuanceAndSync",
    "separateJubjubBinding"
  ]
  and (.acceptance | to_entries | all(.value == true))' \
  "$EVIDENCE" >/dev/null || fail evidence-schema
printf 'portal-headless-e2e: PASS evidence=%s\n' "${EVIDENCE#"$REPO_ROOT/"}"
