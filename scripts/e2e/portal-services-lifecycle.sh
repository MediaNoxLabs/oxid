#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Compatibility entrypoint for Portal service-only lifecycle operations.
set -euo pipefail

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly MANUAL_ROOT="$ROOT/target/portal-tailnet-manual"
PORTAL_INTEGRATION_CHECKOUT="$MANUAL_ROOT/prepared/portal-source" \
OXID_PORTAL_CONSUMER_STATE_DIR="$MANUAL_ROOT/runtime/portal-consumer" \
  exec "$ROOT/scripts/portal-consumer-lifecycle.sh" "${1:-}"
