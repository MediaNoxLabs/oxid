#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/standalone-compose-ownership.sh
source "$ROOT/scripts/lib/standalone-compose-ownership.sh"

EXPECTED_COMPOSE="/private/oxid/canonical-compose.yml"
EXPECTED_WORKDIR="/private/oxid"
IDS='["indexer-id","node-id","proof-id"]'
MIXED=0

docker() {
  local id="${*: -1}"
  local format="$3"
  local service
  case "$id" in
    indexer-id) service=indexer ;;
    node-id) service=node ;;
    proof-id) service=proof-server ;;
    *) return 1 ;;
  esac
  case "$format" in
    *compose.service*) printf '%s\n' "$service" ;;
    *project.config_files*)
      if [ "$MIXED" = 1 ] && [ "$id" = node-id ]; then
        printf '%s\n' '/private/foreign/canonical-compose.yml'
      else
        printf '%s\n' "$EXPECTED_COMPOSE"
      fi
      ;;
    *project.working_dir*)
      if [ "$MIXED" = 1 ] && [ "$id" = node-id ]; then
        printf '%s\n' '/private/foreign'
      else
        printf '%s\n' "$EXPECTED_WORKDIR"
      fi
      ;;
    *compose.project*) printf '%s\n' 'oxid-standalone' ;;
    *) return 1 ;;
  esac
}

oxid_validate_standalone_compose_ownership "$EXPECTED_COMPOSE" "$EXPECTED_WORKDIR" "$IDS"

MIXED=1
if oxid_validate_standalone_compose_ownership "$EXPECTED_COMPOSE" "$EXPECTED_WORKDIR" "$IDS"; then
  printf '%s\n' 'standalone-compose-ownership-contract: FAIL mixed origin accepted' >&2
  exit 1
fi

MIXED=0
if oxid_validate_standalone_compose_ownership "$EXPECTED_COMPOSE" "$EXPECTED_WORKDIR" '["indexer-id","node-id","node-id"]'; then
  printf '%s\n' 'standalone-compose-ownership-contract: FAIL duplicate service accepted' >&2
  exit 1
fi

printf '%s\n' 'standalone-compose-ownership-contract: PASS exact service set accepted mixed origin rejected duplicate service rejected'
