#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly BASE="f113a8c1a44d45415b4bf9765dbb7a3411ac8499"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly STATE_DIR="$REPO_ROOT/target/portal-headless-e2e"
readonly EVIDENCE="$STATE_DIR/evidence.json"
readonly JOURNEY="$STATE_DIR/journey.json"
readonly LOCK_DIR="$STATE_DIR/run.lock"
readonly RAW_LOG="$STATE_DIR/private-test.log"
readonly SNAPSHOT_BEFORE="$STATE_DIR/docker-before.txt"
readonly SNAPSHOT_AFTER="$STATE_DIR/docker-after.txt"
readonly COMPOSE_FILE="$REPO_ROOT/scripts/standalone-stack.yml"
readonly MOCK_CONTRACT="$REPO_ROOT/apps/oxid-headless/tests/portal_live_flow.rs"
readonly TEMPORARY_EVIDENCE="$STATE_DIR/evidence.tmp"
lock_owned=0

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  rm -f -- "$JOURNEY" "$RAW_LOG" "$SNAPSHOT_BEFORE" "$SNAPSHOT_AFTER" \
    "$TEMPORARY_EVIDENCE"
  rm -rf -- "$STATE_DIR/runtime"
  if [ "$lock_owned" = 1 ]; then
    rmdir -- "$LOCK_DIR" 2>/dev/null || status=1
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

fail() {
  printf 'portal-headless-e2e: FAIL phase=%s\n' "$1" >&2
  exit 1
}

portal_project_absent() {
  [ -z "$(docker ps -a --quiet \
    --filter label=com.docker.compose.project=oxid-portal-consumer)" ]
}

snapshot_standalone() {
  local destination=$1 containers services
  containers="$(docker ps --quiet \
    --filter label=com.docker.compose.project=oxid-standalone)" || return 1
  [ "$(printf '%s\n' "$containers" | awk 'NF { count += 1 } END { print count + 0 }')" = 3 ] \
    || return 1
  : >"$destination"
  while IFS= read -r container; do
    [ -n "$container" ] || continue
    docker inspect --format \
      '{{.Id}}|{{index .Config.Labels "com.docker.compose.service"}}|{{.Image}}' \
      "$container" >>"$destination" || return 1
  done <<EOF
$containers
EOF
  sort -t '|' -k2,2 -o "$destination" "$destination"
  services="$(cut -d '|' -f 2 "$destination")"
  [ "$services" = $'indexer\nnode\nproof-server' ] || return 1
}

probe_standalone() {
  curl --fail --silent --show-error --max-time 5 \
    http://127.0.0.1:9944/health >/dev/null || return 1
  curl --fail --silent --show-error --max-time 5 \
    -H 'content-type: application/json' \
    --data '{"query":"query OxidPhase1V3 { block { height } }"}' \
    http://127.0.0.1:8088/api/v3/graphql \
    | jq -e '.data.block.height | numbers' >/dev/null || return 1
  curl --fail --silent --show-error --max-time 5 \
    -H 'content-type: application/json' \
    --data '{"query":"query OxidPhase1V4 { block { height } }"}' \
    http://127.0.0.1:8088/api/v4/graphql \
    | jq -e '.data.block.height | numbers' >/dev/null || return 1
  curl --fail --silent --show-error --max-time 5 \
    http://127.0.0.1:6300/ready >/dev/null || return 1
}

for command_name in awk cargo cmp curl cut docker git grep jq shasum sort; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
docker info >/dev/null 2>&1 || fail docker-unavailable
mkdir -p -- "$STATE_DIR"
chmod 700 "$STATE_DIR"
if ! mkdir -- "$LOCK_DIR" 2>/dev/null; then
  fail concurrent-run
fi
lock_owned=1
umask 077
rm -f -- "$EVIDENCE" "$JOURNEY" "$RAW_LOG" "$SNAPSHOT_BEFORE" "$SNAPSHOT_AFTER" \
  "$TEMPORARY_EVIDENCE"
: >"$RAW_LOG"

[ -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=all)" ] || fail candidate-dirty
readonly HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
readonly TREE="$(git -C "$REPO_ROOT" rev-parse HEAD^{tree})"
[[ "$HEAD" =~ ^[0-9a-f]{40}$ ]] || fail candidate-head
[[ "$TREE" =~ ^[0-9a-f]{40}$ ]] || fail candidate-tree
[ "$(git -C "$REPO_ROOT" rev-parse HEAD^)" = "$BASE" ] || fail candidate-base
[ "$(git -C "$REPO_ROOT" rev-list --count "$BASE..$HEAD")" = 1 ] || fail candidate-range
portal_project_absent || fail unexpected-portal-project

if ! cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p oxid-headless \
  >>"$RAW_LOG" 2>&1; then
  fail headless-build
fi
readonly HEADLESS_BINARY="$REPO_ROOT/target/debug/oxid-headless"
[ -x "$HEADLESS_BINARY" ] || fail headless-binary
readonly HEADLESS_SHA="$(shasum -a 256 "$HEADLESS_BINARY" | awk '{print $1}')"
readonly COMPOSE_SHA="$(shasum -a 256 "$COMPOSE_FILE" | awk '{print $1}')"
readonly MOCK_SHA="$(shasum -a 256 "$MOCK_CONTRACT" | awk '{print $1}')"
[[ "$HEADLESS_SHA$COMPOSE_SHA$MOCK_SHA" =~ ^[0-9a-f]{192}$ ]] || fail artifact-digest

snapshot_standalone "$SNAPSHOT_BEFORE" || fail standalone-identity
probe_standalone || fail standalone-readiness
readonly SNAPSHOT_SHA="$(shasum -a 256 "$SNAPSHOT_BEFORE" | awk '{print $1}')"
readonly IMAGE_IDS="$(jq -Rn \
  '[inputs | split("|") | {key:.[1], value:.[2]}] | from_entries' \
  <"$SNAPSHOT_BEFORE")"

if ! OXID_PHASE1_CANDIDATE_HEAD="$HEAD" \
  OXID_PHASE1_JOURNEY_PATH="$JOURNEY" \
  cargo test --manifest-path "$REPO_ROOT/Cargo.toml" -p oxid-headless \
    --test portal_live_flow \
    local_mock_issuer_and_same_headless_process_use_standalone_indexer \
    -- --ignored --exact >>"$RAW_LOG" 2>&1; then
  fail live-flow
fi
[ -f "$JOURNEY" ] || fail journey-missing
[ "$(shasum -a 256 "$HEADLESS_BINARY" | awk '{print $1}')" = "$HEADLESS_SHA" ] \
  || fail binary-drift
portal_project_absent || fail unexpected-portal-project
snapshot_standalone "$SNAPSHOT_AFTER" || fail standalone-post-identity
cmp -s "$SNAPSHOT_BEFORE" "$SNAPSHOT_AFTER" || fail docker-ownership-drift
probe_standalone || fail standalone-post-readiness

jq -e '
  keys == ["acceptance","headlessIndexerHeight","independentIndexerHeight","schema"]
  and .schema == "oxid-phase1-local-headless-journey-v1"
  and (.headlessIndexerHeight | numbers)
  and (.independentIndexerHeight | numbers)
  and (([.headlessIndexerHeight - .independentIndexerHeight,
          .independentIndexerHeight - .headlessIndexerHeight] | max) <= 4)
  and (.acceptance | keys) == [
    "encryptedPersistence",
    "explicitConsent",
    "issuerCallsBlockedBeforeConsent",
    "listing",
    "managedAuthentication",
    "pendingIssuancePreservedAcrossSync",
    "restartRestoration",
    "reverification",
    "sameProcessIssuanceAndSync",
    "separateJubjubBinding",
    "verifiedImport"
  ]
  and (.acceptance | to_entries | all(.value == true))
' "$JOURNEY" >/dev/null || fail journey-schema

jq -n \
  --arg base "$BASE" \
  --arg head "$HEAD" \
  --arg tree "$TREE" \
  --arg binary "$HEADLESS_SHA" \
  --arg compose "$COMPOSE_SHA" \
  --arg mock "$MOCK_SHA" \
  --arg snapshot "$SNAPSHOT_SHA" \
  --argjson images "$IMAGE_IDS" \
  --slurpfile journey "$JOURNEY" \
  '{
    acceptance:($journey[0].acceptance + {
      noDiditDependency:true,
      unchangedDockerOwnership:true
    }),
    artifacts:{
      headlessBinarySha256:$binary,
      mockContractSha256:$mock,
      standaloneComposeSha256:$compose,
      standaloneImageIds:$images,
      standaloneSnapshotSha256:$snapshot
    },
    git:{base:$base,head:$head,tree:$tree},
    headlessIndexerHeight:$journey[0].headlessIndexerHeight,
    independentIndexerHeight:$journey[0].independentIndexerHeight,
    issuerImplementation:"oxid-owned-http-mock",
    midnightInteractionProven:"indexer-sync",
    nodeInteractionProven:false,
    portalServiceExercised:false,
    proofServerInteractionProven:false,
    schema:"oxid-phase1-local-headless-evidence-v1"
  }' >"$TEMPORARY_EVIDENCE"

if grep -Eqi \
  'openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|Alice|Example|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|proof.?jwt|capability|seed|"(route|did|claim|grant|token|nonce|credential|proof|private|log|pid|timestamp|path)"[[:space:]]*:' \
  "$TEMPORARY_EVIDENCE"; then
  fail evidence-denylist
fi
jq -e \
  --arg base "$BASE" --arg head "$HEAD" --arg tree "$TREE" \
  --arg binary "$HEADLESS_SHA" --arg compose "$COMPOSE_SHA" \
  --arg mock "$MOCK_SHA" --arg snapshot "$SNAPSHOT_SHA" '
  keys == [
    "acceptance",
    "artifacts",
    "git",
    "headlessIndexerHeight",
    "independentIndexerHeight",
    "issuerImplementation",
    "midnightInteractionProven",
    "nodeInteractionProven",
    "portalServiceExercised",
    "proofServerInteractionProven",
    "schema"
  ]
  and .schema == "oxid-phase1-local-headless-evidence-v1"
  and .git == {base:$base,head:$head,tree:$tree}
  and (.artifacts | keys) == [
    "headlessBinarySha256",
    "mockContractSha256",
    "standaloneComposeSha256",
    "standaloneImageIds",
    "standaloneSnapshotSha256"
  ]
  and .artifacts.headlessBinarySha256 == $binary
  and .artifacts.standaloneComposeSha256 == $compose
  and .artifacts.mockContractSha256 == $mock
  and .artifacts.standaloneSnapshotSha256 == $snapshot
  and (.artifacts.standaloneImageIds | keys) == ["indexer","node","proof-server"]
  and .issuerImplementation == "oxid-owned-http-mock"
  and .portalServiceExercised == false
  and .midnightInteractionProven == "indexer-sync"
  and .nodeInteractionProven == false
  and .proofServerInteractionProven == false
  and (.headlessIndexerHeight | numbers)
  and (.independentIndexerHeight | numbers)
  and (([.headlessIndexerHeight - .independentIndexerHeight,
          .independentIndexerHeight - .headlessIndexerHeight] | max) <= 4)
  and (.acceptance | keys) == [
    "encryptedPersistence",
    "explicitConsent",
    "issuerCallsBlockedBeforeConsent",
    "listing",
    "managedAuthentication",
    "noDiditDependency",
    "pendingIssuancePreservedAcrossSync",
    "restartRestoration",
    "reverification",
    "sameProcessIssuanceAndSync",
    "separateJubjubBinding",
    "unchangedDockerOwnership",
    "verifiedImport"
  ]
  and (.acceptance | to_entries | all(.value == true))
' "$TEMPORARY_EVIDENCE" >/dev/null || fail evidence-schema
mv -f -- "$TEMPORARY_EVIDENCE" "$EVIDENCE"
printf 'portal-headless-e2e: PASS evidence=%s\n' "${EVIDENCE#"$REPO_ROOT/"}"
