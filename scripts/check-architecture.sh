#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required; run this check from 'nix develop'." >&2
  exit 1
fi

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --no-deps --format-version 1 >"$metadata_file"

check_workspace_dependencies() {
  local package="$1"
  shift
  local allowed=("$@")
  local dependency
  local permitted

  if ! jq -e --arg package "$package" '.packages[] | select(.name == $package)' "$metadata_file" >/dev/null; then
    echo "Architecture check is missing workspace package '$package'." >&2
    exit 1
  fi

  while IFS= read -r dependency; do
    [ -n "$dependency" ] || continue
    permitted=false
    for candidate in "${allowed[@]}"; do
      if [ "$dependency" = "$candidate" ]; then
        permitted=true
        break
      fi
    done
    if ! $permitted; then
      echo "$package must not depend on workspace package $dependency" >&2
      exit 1
    fi
  done < <(
    jq -r --arg package "$package" '
      .packages[]
      | select(.name == $package)
      | .dependencies[]
      | select(.kind != "dev")
      | select(.path != null)
      | .name
    ' "$metadata_file" | sort -u
  )
}

check_no_external_dependencies() {
  local package="$1"
  local external_count
  external_count="$(
    jq -r --arg package "$package" '
      [
        .packages[]
        | select(.name == $package)
        | .dependencies[]
        | select(.source != null)
      ]
      | length
    ' "$metadata_file"
  )"
  if [ "$external_count" != "0" ]; then
    echo "$package must not depend directly on external crates" >&2
    exit 1
  fi
}

check_workspace_dependencies oxid-foundation
check_workspace_dependencies oxid-wallet-domain oxid-foundation
check_workspace_dependencies oxid-identity-domain oxid-foundation
check_workspace_dependencies oxid-credential-domain oxid-foundation
check_workspace_dependencies oxid-protocol-domain oxid-foundation
check_workspace_dependencies oxid-platform-ports oxid-foundation
check_workspace_dependencies oxid-presentation-domain oxid-foundation
check_workspace_dependencies oxid-passport-vault-domain
check_workspace_dependencies oxid-wallet-application \
  oxid-foundation oxid-platform-ports oxid-wallet-domain
check_workspace_dependencies oxid-identity-application \
  oxid-foundation oxid-identity-domain
check_workspace_dependencies oxid-credential-application \
  oxid-credential-domain oxid-foundation
check_workspace_dependencies oxid-protocol-application \
  oxid-foundation oxid-protocol-domain
check_workspace_dependencies oxid-presentation-application \
  oxid-foundation oxid-presentation-domain
check_workspace_dependencies oxid-passport-vault-application \
  oxid-foundation oxid-passport-vault-domain oxid-platform-ports
check_workspace_dependencies oxid-adapter-storage-memory \
  oxid-credential-application oxid-credential-domain oxid-foundation \
  oxid-identity-application oxid-identity-domain \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-storage-dev \
  oxid-foundation oxid-platform-ports oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-storage-json \
  oxid-foundation oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-platform-system \
  oxid-foundation oxid-platform-ports
check_workspace_dependencies oxid-adapter-midnight \
  oxid-foundation oxid-platform-ports oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-did-midnight \
  oxid-identity-application oxid-identity-domain oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-storage-identity-json \
  oxid-identity-application oxid-identity-domain
check_workspace_dependencies oxid-adapter-storage-credential-json \
  oxid-credential-application oxid-credential-domain oxid-foundation
check_workspace_dependencies oxid-adapter-vc-midnight \
  oxid-credential-application oxid-credential-domain oxid-foundation \
  oxid-identity-application oxid-identity-domain oxid-platform-ports \
  oxid-presentation-application oxid-presentation-domain
check_workspace_dependencies oxid-adapter-openid4vci \
  oxid-credential-application oxid-identity-application oxid-platform-ports \
  oxid-protocol-application oxid-protocol-domain
check_workspace_dependencies oxid-adapter-openid4vp \
  oxid-credential-application oxid-platform-ports \
  oxid-presentation-application oxid-presentation-domain
check_workspace_dependencies oxid-adapter-siopv2 \
  oxid-identity-application oxid-platform-ports \
  oxid-protocol-application oxid-protocol-domain
check_workspace_dependencies oxid-adapter-passport-vault \
  oxid-adapter-vc-midnight oxid-credential-application oxid-credential-domain \
  oxid-passport-vault-application oxid-passport-vault-domain oxid-platform-ports
check_workspace_dependencies oxid-ui-dioxus \
  oxid-credential-application oxid-identity-application oxid-identity-domain \
  oxid-passport-vault-application oxid-presentation-application \
  oxid-protocol-application oxid-wallet-application
check_workspace_dependencies oxid-composition \
  oxid-adapter-did-midnight oxid-adapter-openid4vci oxid-adapter-siopv2 \
  oxid-adapter-openid4vp oxid-adapter-passport-vault \
  oxid-adapter-platform-system \
  oxid-adapter-storage-credential-json oxid-adapter-storage-json \
  oxid-adapter-storage-identity-json oxid-adapter-vc-midnight \
  oxid-adapter-storage-memory oxid-adapter-storage-dev oxid-adapter-midnight \
  oxid-credential-application oxid-identity-application \
  oxid-passport-vault-application oxid-presentation-application \
  oxid-protocol-application \
  oxid-wallet-application
check_workspace_dependencies oxid-app oxid-composition oxid-ui-dioxus
check_workspace_dependencies oxid-headless \
  oxid-composition oxid-credential-application \
  oxid-identity-application oxid-identity-domain \
  oxid-passport-vault-application oxid-passport-vault-domain \
  oxid-presentation-application oxid-protocol-application \
  oxid-wallet-application oxid-wallet-domain

unsafe_sources="$(rg -l '\bunsafe\b' apps crates --glob '*.rs' || true)"
if [ "$unsafe_sources" != "crates/adapters/storage-json/src/lib.rs" ]; then
  echo "Unsafe Rust is permitted only in the reviewed Android profile-path boundary." >&2
  if [ -n "$unsafe_sources" ]; then
    echo "$unsafe_sources" >&2
  fi
  exit 1
fi

check_no_external_dependencies oxid-foundation
check_no_external_dependencies oxid-wallet-domain
check_no_external_dependencies oxid-identity-domain
check_no_external_dependencies oxid-credential-domain
check_no_external_dependencies oxid-protocol-domain
check_no_external_dependencies oxid-platform-ports
check_no_external_dependencies oxid-presentation-domain
check_no_external_dependencies oxid-passport-vault-domain
check_no_external_dependencies oxid-wallet-application
check_no_external_dependencies oxid-identity-application
check_no_external_dependencies oxid-credential-application
check_no_external_dependencies oxid-protocol-application
check_no_external_dependencies oxid-presentation-application
check_no_external_dependencies oxid-passport-vault-application

echo "Architecture dependency rules passed."
