#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
readonly LIFECYCLE="$ROOT/scripts/portal-consumer-lifecycle.sh"
readonly SERVICES_LIFECYCLE="$ROOT/scripts/e2e/portal-services-lifecycle.sh"

fail() {
  printf 'portal-consumer-lifecycle-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -f "$LIFECYCLE" ] || fail lifecycle
[ -x "$SERVICES_LIFECYCLE" ] || fail services-lifecycle-wrapper
for mapping in \
  "midnight-did-resolver-image) image_id=\"\$(docker image inspect --format '{{.Id}}' midnight-did-resolver:0.1.0" \
  "did-manager-image) image_id=\"\$(docker image inspect --format '{{.Id}}' laceid-did-manager:0.1.0" \
  "issuer-image) image_id=\"\$(docker image inspect --format '{{.Id}}' laceid-issuer:0.1.0"; do
  grep -qF "$mapping" "$LIFECYCLE" || fail image-tag
done
if grep -qE '(midnight-did-resolver|laceid-did-manager|laceid-issuer):local' "$LIFECYCLE"; then
  fail stale-local-tag
fi

for tailnet_contract in \
  'PORTAL_TAILNET_MOCK_STATE_DIR' \
  'tailnet_mock_state_valid' \
  'tailnet-mock-transform.mjs' \
  '--validate "$TAILNET_MOCK_STATE" "$PORTAL_ISSUER_URL"' \
  '--data-binary "@$mock_state"'; do
  grep -qF -- "$tailnet_contract" "$LIFECYCLE" || fail tailnet-private-mock
 done

for preparation_contract in \
  'prerequisite|prepare|prepared-status|up|status|down|services-up|services-status|services-stop' \
  'oxid-portal-consumer-prepared-v1' \
  'prepare-checkpoint.json' \
  'prepared-receipt.json' \
  'midnight-did-resolver-image did-manager-image issuer-image' \
  'localCacheHit:$cacheHit' \
  'prepareDurationSeconds:$duration' \
  'PORTAL_CONSUMER_PREPARED_RECEIPT' \
  'prepared_receipt_valid "$EXTERNAL_PREPARED_RECEIPT" complete'; do
  grep -qF -- "$preparation_contract" "$LIFECYCLE" || fail resumable-preparation
done

for services_contract in \
  'run_services_up()' \
  'run_services_status()' \
  'run_services_stop()' \
  'compose up -d --wait --wait-timeout 600 smocker did-resolver did-manager issuer' \
  'compose stop --timeout 30 smocker did-resolver did-manager issuer' \
  'oxid-portal-consumer-services-status-v1'; do
  grep -qF -- "$services_contract" "$LIFECYCLE" || fail services-lifecycle
done

for wrapper_contract in \
  'portal-consumer-lifecycle.sh' \
  'exec "$ROOT/scripts/portal-consumer-lifecycle.sh" "${1:-}"'; do
  grep -qF -- "$wrapper_contract" "$SERVICES_LIFECYCLE" || fail services-lifecycle-wrapper
done

services_body="$(sed -n '/run_services_up()/,/run_down()/p' "$LIFECYCLE")"
if grep -qE 'build_image|compose down|adb |pm clear' <<<"$services_body"; then
  fail services-mutation-boundary
fi

printf 'portal-consumer-lifecycle-contract: PASS pinned-image-tags=0.1.0 tailnet-private-mock=true resumable-preparation=true services-no-build-install-launch-reset=true\n'
