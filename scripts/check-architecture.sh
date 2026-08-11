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
check_workspace_dependencies oxid-platform-ports oxid-foundation
check_workspace_dependencies oxid-wallet-application \
  oxid-foundation oxid-platform-ports oxid-wallet-domain
check_workspace_dependencies oxid-adapter-storage-memory \
  oxid-foundation oxid-wallet-application oxid-wallet-domain
check_workspace_dependencies oxid-adapter-platform-system \
  oxid-foundation oxid-platform-ports
check_workspace_dependencies oxid-ui-dioxus oxid-wallet-application
check_workspace_dependencies oxid-composition \
  oxid-adapter-platform-system oxid-adapter-storage-memory \
  oxid-ui-dioxus oxid-wallet-application
check_workspace_dependencies oxid-app oxid-composition oxid-ui-dioxus

check_no_external_dependencies oxid-foundation
check_no_external_dependencies oxid-wallet-domain
check_no_external_dependencies oxid-platform-ports
check_no_external_dependencies oxid-wallet-application

echo "Architecture dependency rules passed."
