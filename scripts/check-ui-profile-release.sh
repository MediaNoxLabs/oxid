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

# The application guards test oxid-app's own feature names, so a dependency
# feature path (oxid-ui-dioxus/ui-profile-*) once compiled the profile into an
# otherwise production-composed binary without reaching them. The guards in
# crates/ui-dioxus/src/profile_guard.rs close that path; prove both variants
# stay rejected.
for dependency_profile in ui-profile-dev ui-profile-demo; do
  if cargo check -p oxid-app --no-default-features \
    --features "desktop,oxid-ui-dioxus/$dependency_profile" >"$failure_log" 2>&1; then
    echo "oxid-ui-dioxus/$dependency_profile compiled through the dependency feature path" >&2
    exit 1
  fi
  if ! rg -q 'a non-user UI profile must be selected through oxid-app' "$failure_log"; then
    echo "oxid-ui-dioxus/$dependency_profile failed for an unexpected reason" >&2
    sed -n '1,120p' "$failure_log" >&2
    exit 1
  fi
done

# The same bypass with a standalone composition present must also be rejected:
# authority comes from the application crate, never from the composition
# feature being incidentally correct.
if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,oxid-ui-dioxus/ui-profile-demo \
  >"$failure_log" 2>&1; then
  echo "the dependency feature path compiled alongside standalone-development" >&2
  exit 1
fi
if ! rg -q 'a non-user UI profile must be selected through oxid-app' "$failure_log"; then
  echo "the dependency feature path failed for an unexpected reason" >&2
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
  --features desktop,standalone-local >"$failure_log" 2>&1; then
  echo "standalone-local compiled without standalone-development" >&2
  exit 1
fi
if ! rg -q 'standalone-local requires standalone-development' "$failure_log"; then
  echo "standalone-local failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,standalone-native-custody,standalone-local \
  >"$failure_log" 2>&1; then
  echo "standalone-local compiled with native custody" >&2
  exit 1
fi
if ! rg -q \
  'select exactly one standalone custody feature|standalone-local is incompatible with native custody' \
  "$failure_log"; then
  echo "native custody rejected standalone-local for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,standalone-local,standalone-tailnet \
  >"$failure_log" 2>&1; then
  echo "standalone-local and standalone-tailnet compiled together" >&2
  exit 1
fi
if ! rg -q 'select at most one live standalone route profile' "$failure_log"; then
  echo "combined local/tailnet profiles failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,standalone-local

# The Portal dependency feature owns trigger and mobile HTTP behavior, so it
# must not compile without the application-profile authority forwarded only by
# oxid-app/standalone-portal. Prove both direct and app dependency-feature
# paths fail closed before checking the app's guarded profile combinations.
for bypass in composition application; do
  if [ "$bypass" = composition ]; then
    bypass_command=(cargo check -p oxid-composition --features mobile-portal)
  else
    bypass_command=(cargo check -p oxid-app --no-default-features --features desktop,oxid-composition/mobile-portal)
  fi
  if "${bypass_command[@]}" >"$failure_log" 2>&1; then
    echo "mobile-portal compiled through the $bypass dependency-feature path" >&2
    exit 1
  fi
  if ! rg -q 'mobile-portal must be selected through oxid-app/standalone-portal' "$failure_log"; then
    echo "the $bypass mobile-portal bypass failed for an unexpected reason" >&2
    sed -n '1,120p' "$failure_log" >&2
    exit 1
  fi
done

composition_portal_members="$(awk '
  /^mobile-portal = \[/ { capture=1; next }
  capture && /^\]/ { exit }
  capture { gsub(/[",[:space:]]/, ""); if (length) print }
' crates/composition/Cargo.toml | sort)"
expected_composition_portal_members="$(printf '%s\n' \
  oxid-adapter-identity-ingress/loopback-test-offer-trigger \
  oxid-adapter-openid4vci/portal-http-mobile | sort)"
if [ "$composition_portal_members" != "$expected_composition_portal_members" ]; then
  echo "oxid-composition/mobile-portal feature wiring is not exact" >&2
  exit 1
fi
app_portal_members="$(awk '
  /^standalone-portal = \[/ { capture=1; next }
  capture && /^\]/ { exit }
  capture { gsub(/[",[:space:]]/, ""); if (length) print }
' apps/oxid/Cargo.toml | sort)"
expected_app_portal_members="$(printf '%s\n' \
  mobile \
  oxid-composition/app-profile-authority \
  oxid-composition/mobile-portal \
  standalone-development \
  standalone-local | sort)"
if [ "$app_portal_members" != "$expected_app_portal_members" ] ||
  [ "$(rg -cF '"oxid-composition/app-profile-authority"' apps/oxid/Cargo.toml)" -ne 1 ]; then
  echo "oxid-app/standalone-portal authority wiring is not exact" >&2
  exit 1
fi

# This strict/static gate must execute the trigger failure, worker-bound, and
# one-item reservation tests rather than merely compiling their feature.
cargo test -p oxid-adapter-identity-ingress --features loopback-test-offer-trigger

# Portal is a separate mobile-only test profile. Host/desktop, tailnet, and
# native-custody combinations must fail before they can select composition.
if cargo check -p oxid-composition --features mobile-portal,app-profile-authority >"$failure_log" 2>&1; then
  echo "mobile-portal compiled directly for a non-mobile host" >&2
  exit 1
fi
if ! rg -q 'mobile-portal is available only on iOS and Android' "$failure_log"; then
  echo "direct mobile-portal rejection failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi
if cargo check -p oxid-app --no-default-features \
  --features desktop,oxid-composition/mobile-portal,oxid-composition/app-profile-authority \
  >"$failure_log" 2>&1; then
  echo "mobile-portal compiled through the app dependency feature path" >&2
  exit 1
fi
if ! rg -q 'mobile-portal is available only on iOS and Android' "$failure_log"; then
  echo "app dependency mobile-portal rejection failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi
if cargo check -p oxid-app --no-default-features \
  --features standalone-portal >"$failure_log" 2>&1; then
  echo "standalone-portal compiled for a non-mobile host" >&2
  exit 1
fi
if ! rg -q 'standalone-portal is available only on iOS and Android|mobile-portal is available only on iOS and Android' "$failure_log"; then
  echo "standalone-portal host rejection failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi

for conflicting_profile in standalone-tailnet standalone-native-custody; do
  if cargo check -p oxid-app --no-default-features \
    --features "standalone-portal,$conflicting_profile" >"$failure_log" 2>&1; then
    echo "standalone-portal compiled with $conflicting_profile" >&2
    exit 1
  fi
  if ! rg -q 'standalone-portal is incompatible with tailnet and native custody|mobile-portal is available only on iOS and Android' "$failure_log"; then
    echo "standalone-portal/$conflicting_profile failed for an unexpected reason" >&2
    sed -n '1,120p' "$failure_log" >&2
    exit 1
  fi
done

# The adapter feature is intentionally inert on WASM: its mobile-only optional
# HTTP dependencies and Portal module are not selected for the browser target.
cargo check -p oxid-adapter-openid4vci --target wasm32-unknown-unknown \
  --features portal-http-mobile

if cargo check -p oxid-app --no-default-features \
  --features desktop,standalone-development,standalone-local,ui-profile-demo \
  >"$failure_log" 2>&1; then
  echo "ui-profile-demo compiled with the local live-stack profile" >&2
  exit 1
fi
if ! rg -q \
  'ui-profile-demo requires deterministic standalone-development routes' \
  "$failure_log"; then
  echo "ui-profile-demo rejected the local profile for an unexpected reason" >&2
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
if rg -a -q 'OXID_STANDALONE_PORTAL_PROFILE' "$release_binary"; then
  echo "normal release binary contains the standalone Portal profile" >&2
  exit 1
fi
if rg -a -q \
  'OXID_STANDALONE_LOCAL_PROFILE|ws://127\.0\.0\.1:8088/api/v4/graphql/ws|http://127\.0\.0\.1:8088/api/v4/graphql|ws://127\.0\.0\.1:9944|http://127\.0\.0\.1:6300' \
  "$release_binary"; then
  echo "normal release binary contains the standalone local profile or its routes" >&2
  exit 1
fi
if rg -a -q \
  'OXID_STANDALONE_FUNDER_SEED_HEX|OXID_ENABLE_LIVE_STANDALONE_FUNDING|Ephemeral funded recipient|Standalone funding authority|Ephemeral shielded recipient|Standalone shielded funding authority' \
  "$release_binary"; then
  echo "normal release binary contains the standalone funding harness" >&2
  exit 1
fi
if rg -a -q \
  'OXID_PREPROD_MASTER_SEED_HEX|OXID_ENABLE_LIVE_PREPROD_E2E|OXID_PREPROD_E2E_CASE_INDEX|OXID_PREPROD_E2E_STATE_DIR|OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY|OXID_PREPROD_FUNDING_MANIFEST_V[12]|OXID_PREPROD_FUNDING_OBSERVATION_V1|Preprod E2E wallet A|Preprod E2E wallet B|oxid-preprod-registration-e2e-2026-08|lace-proof-pub\.preprod\.midnight\.network' \
  "$release_binary"; then
  echo "normal release binary contains the preprod registration funding harness" >&2
  exit 1
fi

echo "UI profile compile guards and dev/demo/local/tailnet release exclusion passed."
