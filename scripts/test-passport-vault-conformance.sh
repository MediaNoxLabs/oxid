#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

require_directory() {
  local variable="$1"
  local value="${!variable:-}"
  if [[ -z "$value" || ! -d "$value" ]]; then
    printf 'passport-vault-conformance: missing directory prerequisite %s\n' "$variable" >&2
    exit 2
  fi
}

require_executable() {
  local variable="$1"
  local value="${!variable:-}"
  if [[ -z "$value" || ! -x "$value" ]]; then
    printf 'passport-vault-conformance: missing executable prerequisite %s\n' "$variable" >&2
    exit 2
  fi
}

require_directory OXID_PASSPORT_VAULT_ARTIFACTS_DIR
require_executable OXID_PASSPORT_VAULT_COMPOSER

cargo test -p oxid-adapter-passport-vault --lib \
  compact_artifacts::tests::packaged_artifacts_authenticate_and_resolve_only_wallet_circuits_when_configured \
  -- --ignored --exact

cargo test -p oxid-adapter-passport-vault --lib \
  compact_composer_conformance::packaged_composer_emits_a_rust_compatible_unproven_call_when_configured \
  -- --ignored --exact

cargo test -p oxid-composition --lib \
  passport_vault::tests::standalone_managed_claim_composes_and_settles_through_the_native_stack \
  -- --ignored --exact

printf '%s\n' 'passport-vault-conformance: PASS tests=3 prerequisites=nix-packaged'
