#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
readonly LIFECYCLE="$ROOT/scripts/portal-consumer-lifecycle.sh"

fail() {
  printf 'portal-consumer-lifecycle-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -f "$LIFECYCLE" ] || fail lifecycle
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

printf 'portal-consumer-lifecycle-contract: PASS pinned-image-tags=0.1.0 tailnet-private-mock=true\n'
