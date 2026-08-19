#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

failure_log="$(mktemp)"
trap 'rm -f "$failure_log"' EXIT

# The incoming profile is presentation-only but still must never be selectable
# against the fail-closed production composition.
if cargo check -p oxid-app --no-default-features \
  --features desktop,ui-profile-dev >"$failure_log" 2>&1; then
  echo "ui-profile-dev compiled without an explicit standalone composition" >&2
  exit 1
fi
if ! rg -q 'ui-profile-dev requires an explicit standalone composition' "$failure_log"; then
  echo "ui-profile-dev failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,ui-profile-demo >"$failure_log" 2>&1; then
  echo "ui-profile-demo compiled without standalone-development" >&2
  exit 1
fi
if ! rg -q 'ui-profile-demo requires standalone-development' "$failure_log"; then
  echo "ui-profile-demo failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-native-custody,ui-profile-demo >"$failure_log" 2>&1; then
  echo "ui-profile-demo compiled with native standalone custody" >&2
  exit 1
fi
if ! rg -q 'ui-profile-demo requires standalone-development' "$failure_log"; then
  echo "native standalone rejected ui-profile-demo for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,ui-profile-dev,ui-profile-demo >"$failure_log" 2>&1; then
  echo "ui-profile-dev and ui-profile-demo compiled together" >&2
  exit 1
fi
if ! rg -q 'select at most one non-user UI profile' "$failure_log"; then
  echo "combined dev/demo profiles failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-tailnet >"$failure_log" 2>&1; then
  echo "standalone-tailnet compiled without the development mobile composition" >&2
  exit 1
fi
if ! rg -q \
  'standalone-tailnet requires standalone-development on iOS or Android' \
  "$failure_log"; then
  echo "standalone-tailnet failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-native-custody,standalone-tailnet \
  >"$failure_log" 2>&1; then
  echo "standalone-tailnet compiled with native custody" >&2
  exit 1
fi
if ! rg -q \
  'standalone-tailnet requires standalone-development on iOS or Android' \
  "$failure_log"; then
  echo "native custody rejected standalone-tailnet for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

# Inspect the actual normal release binary, not only Cargo feature metadata.
# The stable marker is emitted into every developer-profile UI and must be
# absent from a distributed default artifact.
cargo build -p oxid-app --release
release_binary="target/release/oxid-app"
if [[ ! -f "$release_binary" ]]; then
  echo "normal release binary was not produced at $release_binary" >&2
  exit 1
fi
if rg -a -q 'OXID_UI_PROFILE_DEVELOPMENT' "$release_binary"; then
  echo "normal release binary contains the developer-profile marker" >&2
  exit 1
fi
if rg -a -q \
  'OXID_UI_PROFILE_DEMO|OXID_DEMO_BOOTSTRAP_DRAWER|Oxid Demo Wallet|Run full demo setup' \
  "$release_binary"; then
  echo "normal release binary contains demo-profile code or fixture markers" >&2
  exit 1
fi
if rg -a -q 'OXID_STANDALONE_TAILNET_PROFILE' "$release_binary"; then
  echo "normal release binary contains the standalone tailnet profile" >&2
  exit 1
fi

echo "UI profile compile guards and dev/demo/tailnet release exclusion passed."
