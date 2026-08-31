#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for override in CAPABILITY_FACADES_INVENTORY CAPABILITY_FACADES_TODAY CAPABILITY_FACADES_TEST_MODE; do
  if [ "${!override+x}" = x ]; then
    echo "Architecture check does not accept $override." >&2
    exit 1
  fi
done
if [ "$#" -ne 0 ]; then
  echo "Architecture check does not accept arguments." >&2
  exit 1
fi

for required_tool in jq rg; do
  if ! command -v "$required_tool" >/dev/null 2>&1; then
    echo "$required_tool is required; run this check from 'nix develop'." >&2
    exit 1
  fi
done

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$script_directory/check-capability-facades.sh"

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --no-deps --format-version 1 >"$metadata_file"

# Every workspace member must appear in exactly these calls; the default-deny
# sweep at the end of this script fails when a member has no allowlist entry.
covered_packages=()

check_workspace_dependencies() {
  # --all-kinds also constrains dev-dependencies. Core crates use it so a
  # test-only dependency cannot quietly point a domain or application crate
  # at an adapter.
  local include_dev=false
  if [ "$1" = "--all-kinds" ]; then
    include_dev=true
    shift
  fi
  local package="$1"
  shift
  local allowed=("$@")
  local dependency
  local permitted

  covered_packages+=("$package")

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
    jq -r --arg package "$package" --argjson include_dev "$include_dev" '
      .packages[]
      | select(.name == $package)
      | .dependencies[]
      | select($include_dev or .kind != "dev")
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

check_workspace_dependencies --all-kinds oxid-foundation
check_workspace_dependencies --all-kinds oxid-wallet-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-identity-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-credential-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-protocol-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-platform-ports oxid-foundation
check_workspace_dependencies --all-kinds oxid-presentation-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-passport-vault-domain
check_workspace_dependencies --all-kinds oxid-diagnostics-application
check_workspace_dependencies --all-kinds oxid-capabilities-application
check_workspace_dependencies --all-kinds oxid-wallet-application \
  oxid-foundation oxid-platform-ports oxid-wallet-domain
check_workspace_dependencies --all-kinds oxid-identity-application \
  oxid-foundation oxid-identity-domain
check_workspace_dependencies --all-kinds oxid-credential-application \
  oxid-credential-domain oxid-foundation
check_workspace_dependencies --all-kinds oxid-protocol-application \
  oxid-foundation oxid-protocol-domain
check_workspace_dependencies --all-kinds oxid-presentation-application \
  oxid-foundation oxid-presentation-domain
check_workspace_dependencies --all-kinds oxid-passport-vault-application \
  oxid-foundation oxid-passport-vault-domain oxid-platform-ports
check_workspace_dependencies oxid-brand-build
check_workspace_dependencies oxid-adapter-storage-memory \
  oxid-credential-application oxid-credential-domain oxid-foundation \
  oxid-identity-application oxid-identity-domain \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-diagnostics-memory \
  oxid-diagnostics-application
check_workspace_dependencies oxid-adapter-deployment-profile
check_workspace_dependencies oxid-adapter-storage-dev \
  oxid-adapter-backup-portable oxid-foundation oxid-platform-ports \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-custody-software \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-backup-portable \
  oxid-foundation oxid-platform-ports oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-backup-complete \
  oxid-adapter-backup-portable oxid-adapter-storage-credential-json \
  oxid-adapter-storage-identity-json oxid-adapter-storage-json \
  oxid-credential-application oxid-credential-domain \
  oxid-identity-application oxid-identity-domain oxid-platform-ports \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-backup-document-mobile \
  oxid-adapter-mobile-native oxid-wallet-application
check_workspace_dependencies oxid-adapter-storage-mobile \
  oxid-adapter-backup-portable oxid-adapter-custody-software \
  oxid-adapter-mobile-native oxid-foundation oxid-platform-ports \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-storage-json \
  oxid-foundation oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-mobile-native
check_workspace_dependencies oxid-adapter-platform-system \
  oxid-adapter-mobile-native oxid-foundation oxid-platform-ports
check_workspace_dependencies oxid-adapter-midnight \
  oxid-diagnostics-application oxid-foundation oxid-platform-ports \
  oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-did-midnight \
  oxid-identity-application oxid-identity-domain oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-identity-ingress \
  oxid-adapter-mobile-native oxid-platform-ports oxid-protocol-application
check_workspace_dependencies oxid-adapter-storage-identity-json \
  oxid-adapter-store-atomic oxid-identity-application oxid-identity-domain
check_workspace_dependencies oxid-adapter-store-atomic
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
  oxid-foundation oxid-passport-vault-application oxid-passport-vault-domain \
  oxid-platform-ports
check_workspace_dependencies oxid-ui-dioxus \
  oxid-capabilities-application oxid-credential-application oxid-diagnostics-application \
  oxid-identity-application oxid-identity-domain \
  oxid-platform-ports \
  oxid-passport-vault-application oxid-presentation-application \
  oxid-protocol-application oxid-wallet-application
check_workspace_dependencies oxid-composition \
  oxid-adapter-backup-complete oxid-adapter-backup-document-mobile \
  oxid-adapter-backup-portable oxid-adapter-deployment-profile \
  oxid-adapter-diagnostics-memory \
  oxid-adapter-did-midnight oxid-adapter-identity-ingress \
  oxid-adapter-openid4vci oxid-adapter-siopv2 \
  oxid-adapter-openid4vp oxid-adapter-passport-vault \
  oxid-adapter-mobile-native oxid-adapter-platform-system \
  oxid-adapter-storage-credential-json oxid-adapter-storage-json \
  oxid-adapter-storage-identity-json oxid-adapter-vc-midnight \
  oxid-adapter-storage-memory oxid-adapter-storage-mobile \
  oxid-adapter-storage-dev oxid-adapter-midnight \
  oxid-credential-application oxid-diagnostics-application oxid-identity-application \
  oxid-passport-vault-application oxid-platform-ports oxid-presentation-application \
  oxid-protocol-application \
  oxid-wallet-application
check_workspace_dependencies oxid-mcp
check_workspace_dependencies oxid-app \
  oxid-adapter-identity-ingress oxid-brand-build oxid-composition oxid-ui-dioxus
check_workspace_dependencies oxid-headless \
  oxid-capabilities-application oxid-composition oxid-credential-application \
  oxid-diagnostics-application oxid-identity-application oxid-identity-domain \
  oxid-passport-vault-application oxid-passport-vault-domain \
  oxid-presentation-application oxid-protocol-application \
  oxid-wallet-application oxid-wallet-domain

# Default-deny: a workspace member without an allowlist entry above is an
# error, so newly added crates cannot bypass the dependency rules by omission.
while IFS= read -r member; do
  member_covered=false
  for candidate in "${covered_packages[@]}"; do
    if [ "$member" = "$candidate" ]; then
      member_covered=true
      break
    fi
  done
  if ! $member_covered; then
    echo "Workspace package '$member' has no architecture allowlist entry; add a check_workspace_dependencies call for it." >&2
    exit 1
  fi
done < <(jq -r '.packages[].name' "$metadata_file" | sort -u)

# Unsafe Rust allowlist: an empty match set is success (the last unsafe block
# was removed); anything beyond the reviewed boundary is a failure.
unsafe_sources="$(rg -l '\bunsafe\b' apps crates --glob '*.rs' || true)"
if [ -n "$unsafe_sources" ] && [ "$unsafe_sources" != "crates/adapters/storage-json/src/lib.rs" ]; then
  echo "Unsafe Rust is permitted only in the reviewed Android profile-path boundary." >&2
  echo "$unsafe_sources" >&2
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
check_no_external_dependencies oxid-diagnostics-application
check_no_external_dependencies oxid-capabilities-application
check_no_external_dependencies oxid-wallet-application
check_no_external_dependencies oxid-identity-application
check_no_external_dependencies oxid-credential-application
check_no_external_dependencies oxid-protocol-application
check_no_external_dependencies oxid-presentation-application
check_no_external_dependencies oxid-passport-vault-application

./scripts/e2e/android-avd-process-ownership.test.sh

echo "Architecture dependency rules passed."
