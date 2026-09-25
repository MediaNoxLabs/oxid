#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Compatibility entrypoint for Portal service-only lifecycle operations.
set -euo pipefail

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
exec "$ROOT/scripts/portal-consumer-lifecycle.sh" "${1:-}"
