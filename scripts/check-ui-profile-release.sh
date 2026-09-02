#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

failure_log="$(mktemp)"
feature_graph_log="$(mktemp)"
trap 'rm -f "$failure_log" "$feature_graph_log"' EXIT

load_feature_graph() {
  local label="$1"
  local features="$2"
  local target="${3:-}"
  local cargo_tree_args=(-p oxid-app --edges normal --prefix none --format '{p}|{f}')
  if [[ -n "$features" ]]; then
    cargo_tree_args+=(--no-default-features --features "$features")
  fi
  if [[ -n "$target" ]]; then
    cargo_tree_args+=(--target "$target")
  fi
  if ! cargo tree "${cargo_tree_args[@]}" >"$feature_graph_log"; then
    echo "$label feature graph could not be resolved" >&2
    exit 1
  fi
}

resolved_features() {
  local label="$1"
  local package="$2"
  local line
  local matches
  matches="$(rg "^${package} v[^|]*\\|" "$feature_graph_log" || true)"
  matches="$(printf '%s\n' "$matches" | sed -E 's/ \(\*\)$//' | LC_ALL=C sort -u)"
  if [[ -z "$matches" ]]; then
    echo "$label feature graph does not contain $package" >&2
    exit 1
  fi
  if [[ "$(printf '%s\n' "$matches" | wc -l | tr -d ' ')" != "1" ]]; then
    echo "$label feature graph contains ambiguous versions of $package" >&2
    exit 1
  fi
  line="$matches"
  printf '%s\n' "${line#*|}"
}

has_feature() {
  local features="$1"
  local feature="$2"
  [[ ",$features," == *",$feature,"* ]]
}

assert_public_fixture_feature_absent() {
  local label="$1"
  local features="$2"
  local target="${3:-}"
  load_feature_graph "$label" "$features" "$target"
  local composition_features
  local storage_dev_features
  local ui_features
  composition_features="$(resolved_features "$label" oxid-composition)"
  storage_dev_features="$(resolved_features "$label" oxid-adapter-storage-dev)"
  ui_features="$(resolved_features "$label" oxid-ui-dioxus)"
  if has_feature "$composition_features" standalone-development; then
    echo "$label oxid-app release profile enables public standalone development custody" >&2
    exit 1
  fi
  if has_feature "$composition_features" preprod-observation ||
    has_feature "$ui_features" preprod-observation; then
    echo "$label oxid-app release profile enables owner-root PreProd recovery" >&2
    exit 1
  fi
  if [[ "$label" != bare* ]]; then
    if has_feature "$composition_features" standalone-readiness ||
      has_feature "$ui_features" standalone-deployment-profile; then
      echo "$label oxid-app release profile enables standalone deployment discovery" >&2
      exit 1
    fi
  fi
  if has_feature "$ui_features" public-standalone-genesis; then
    echo "$label oxid-app release profile enables the public-genesis warning UI" >&2
    exit 1
  fi
  if has_feature "$storage_dev_features" development-fixture; then
    echo "$label oxid-app release profile enables arbitrary development fixture custody" >&2
    exit 1
  fi
}

assert_preprod_observation_feature_present() {
  local label="$1"
  local target="$2"
  load_feature_graph "$label" "preprod-observation" "$target"
  local composition_features
  local storage_dev_features
  local ui_features
  composition_features="$(resolved_features "$label" oxid-composition)"
  storage_dev_features="$(resolved_features "$label" oxid-adapter-storage-dev)"
  ui_features="$(resolved_features "$label" oxid-ui-dioxus)"
  if ! has_feature "$composition_features" preprod-observation ||
    ! has_feature "$ui_features" preprod-observation; then
    echo "$label does not enable both PreProd observation boundaries" >&2
    exit 1
  fi
  if has_feature "$composition_features" standalone-development ||
    has_feature "$storage_dev_features" development-fixture; then
    echo "$label mixes PreProd recovery with development fixture custody" >&2
    exit 1
  fi
}

assert_public_fixture_feature_present() {
  local label="$1"
  local features="$2"
  local target="${3:-}"
  load_feature_graph "$label" "$features" "$target"
  local composition_features
  local storage_dev_features
  local ui_features
  composition_features="$(resolved_features "$label" oxid-composition)"
  storage_dev_features="$(resolved_features "$label" oxid-adapter-storage-dev)"
  ui_features="$(resolved_features "$label" oxid-ui-dioxus)"
  if ! has_feature "$composition_features" standalone-development; then
    echo "$label does not enable the bounded public-genesis composition capability" >&2
    exit 1
  fi
  if ! has_feature "$ui_features" public-standalone-genesis; then
    echo "$label does not enable the public-genesis warning UI" >&2
    exit 1
  fi
  if ! has_feature "$storage_dev_features" development-fixture; then
    echo "$label does not enable the bounded development fixture custody adapter" >&2
    exit 1
  fi
  if [[ "$label" != "standalone development" ]]; then
    if ! has_feature "$composition_features" standalone-readiness ||
      ! has_feature "$ui_features" standalone-deployment-profile; then
      echo "$label does not expose the bounded standalone deployment profile" >&2
      exit 1
    fi
  fi
}

mode="${1:-all}"
case "$mode" in
  all|--guards|--artifact) ;;
  *)
    echo "usage: $0 [--guards|--artifact]" >&2
    exit 64
    ;;
esac

# The incoming profile is presentation-only but still must never be selectable
# against the fail-closed production composition.
if [[ "$mode" != "--artifact" ]]; then
assert_public_fixture_feature_absent "default" ""
assert_public_fixture_feature_absent "desktop" "desktop"
assert_public_fixture_feature_absent "iOS mobile" "mobile" "aarch64-apple-ios"
assert_public_fixture_feature_absent "Android mobile" "mobile" "aarch64-linux-android"
assert_public_fixture_feature_absent "web" "web" "wasm32-unknown-unknown"
# Bare transport selectors are deliberately invalid: without explicit custody
# they must neither enable the fixture nor compile. The negative cargo checks
# below prove the expected diagnostic; these graph checks prove no authority was
# granted before compilation rejects the incomplete profile.
assert_public_fixture_feature_absent \
  "bare iOS standalone local route" "mobile,standalone-local" "aarch64-apple-ios"
assert_public_fixture_feature_absent \
  "bare Android standalone Tailnet route" "mobile,standalone-tailnet" "aarch64-linux-android"
assert_public_fixture_feature_absent \
  "iOS native mobile" "mobile,standalone-native-custody" "aarch64-apple-ios"
assert_public_fixture_feature_absent \
  "Android native mobile" "mobile,standalone-native-custody" "aarch64-linux-android"
assert_public_fixture_feature_absent \
  "iOS native proving" "mobile,standalone-native-proving-artifacts" "aarch64-apple-ios"
assert_public_fixture_feature_absent \
  "Android native proving" "mobile,standalone-native-proving-artifacts" "aarch64-linux-android"
assert_public_fixture_feature_absent \
  "iOS native mobile developer UI" \
  "mobile,standalone-native-custody,ui-profile-dev" "aarch64-apple-ios"
assert_public_fixture_feature_absent \
  "Android native proving developer UI" \
  "mobile,standalone-native-proving-artifacts,ui-profile-dev" "aarch64-linux-android"
assert_public_fixture_feature_present \
  "standalone development" "standalone-development"
assert_public_fixture_feature_present \
  "iOS standalone local" \
  "mobile,standalone-development,standalone-local" "aarch64-apple-ios"
assert_public_fixture_feature_present \
  "Android standalone Tailnet" \
  "mobile,standalone-development,standalone-tailnet" "aarch64-linux-android"
assert_public_fixture_feature_present \
  "iOS standalone Portal" "standalone-portal" "aarch64-apple-ios"
assert_public_fixture_feature_present \
  "Android standalone Portal Tailnet" "standalone-portal-tailnet" "aarch64-linux-android"
assert_preprod_observation_feature_present \
  "iOS PreProd observation" "aarch64-apple-ios"
assert_preprod_observation_feature_present \
  "Android PreProd observation" "aarch64-linux-android"
if cargo check -p oxid-app --no-default-features \
  --features preprod-observation >"$failure_log" 2>&1; then
  echo "preprod-observation compiled for a non-mobile host" >&2
  exit 1
fi
if ! rg -q 'preprod-observation is available only on iOS and Android' "$failure_log"; then
  echo "preprod-observation host rejection failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi
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

# The physical launcher owns service selection. Keep the current laptop
# MagicDNS lookup and the complete route set in one process; the app receives
# no runtime discovery input.
for launcher_contract in \
  'status="$(tailscale status --json)"' \
  "tailnet_dns_name=\"\$(jq -r '.Self.DNSName | rtrimstr(\".\")' <<<\"\$status\")\"" \
  'export OXID_BUILD_MIDNIGHT_INDEXER_WS_URL="wss://$tailnet_dns_name:8443/api/v4/graphql/ws"' \
  'export OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL="https://$tailnet_dns_name:8443/api/v4/graphql"' \
  'export OXID_BUILD_MIDNIGHT_NODE_WS_URL="wss://$tailnet_dns_name:10000"' \
  'export OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL="https://$tailnet_dns_name"' \
  'exec "$repository_root/scripts/run-android-emulator.sh"'; do
  if ! rg -qF "$launcher_contract" scripts/run-android-tailnet.sh; then
    echo "physical Tailnet launcher contract drifted: $launcher_contract" >&2
    exit 1
  fi
done

if [ "$(uname -s)-$(uname -m)" = "Darwin-arm64" ]; then
  cargo check -p oxid-app --no-default-features --features desktop-portal-test
else
  if cargo check -p oxid-app --no-default-features \
    --features desktop-portal-test >"$failure_log" 2>&1; then
    echo "desktop-portal-test compiled outside ARM64 macOS" >&2
    exit 1
  fi
  if ! rg -q 'desktop-portal-test is available only on ARM64 macOS' "$failure_log"; then
    echo "desktop-portal-test failed for an unexpected reason" >&2
    sed -n '1,120p' "$failure_log" >&2
    exit 1
  fi
fi

# The Portal dependency feature is a low-level mobile capability, not proof of
# caller provenance. Direct selection remains runtime-inert because only the
# app-owned standalone-portal branch calls the explicit Portal constructor; the
# identity-ingress unit suite below proves the default constructor rejects the trigger.

if ! rg -qxF 'standalone-development = ["oxid-adapter-storage-dev/development-fixture"]' crates/composition/Cargo.toml; then
  echo "oxid-composition/standalone-development must enable only bounded fixture custody" >&2
  exit 1
fi
composition_fixture_name="$(sed -n 's/^pub(super) const PUBLIC_STANDALONE_PROFILE_NAME: &str = "\([^"]*\)";$/\1/p' crates/composition/src/standalone_genesis.rs)"
ui_fixture_name="$(sed -n 's/^const PUBLIC_STANDALONE_PROFILE_NAME: &str = "\([^"]*\)";$/\1/p' crates/ui-dioxus/src/lib.rs)"
if [[ -z "$composition_fixture_name" || "$composition_fixture_name" != "$ui_fixture_name" ]]; then
  echo "public standalone fixture profile names drifted between composition and UI" >&2
  exit 1
fi
app_standalone_development_members="$(awk '
  /^standalone-development = \[/ { capture=1; next }
  capture && /^\]/ { exit }
  capture { gsub(/[",[:space:]]/, ""); if (length) print }
' apps/oxid/Cargo.toml | sort)"
expected_app_standalone_development_members="$(printf '%s\n' \
  oxid-composition/standalone-development \
  oxid-ui-dioxus/public-standalone-genesis | sort)"
if [ "$app_standalone_development_members" != "$expected_app_standalone_development_members" ]; then
  echo "oxid-app/standalone-development feature wiring is not exact" >&2
  exit 1
fi

composition_portal_members="$(awk '
  /^mobile-portal = \[/ { capture=1; next }
  capture && /^\]/ { exit }
  capture { gsub(/[",[:space:]]/, ""); if (length) print }
' crates/composition/Cargo.toml | sort)"
expected_composition_portal_members="$(printf '%s\n' \
  dep:oxid-adapter-mobile-native \
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
  oxid-composition/mobile-portal \
  standalone-development \
  standalone-local | sort)"
if [ "$app_portal_members" != "$expected_app_portal_members" ] ||
  rg -qF '"oxid-composition/app-profile-authority"' apps/oxid/Cargo.toml; then
  echo "oxid-app/standalone-portal authority wiring is not exact" >&2
  exit 1
fi

cargo test -p oxid-app portal_profile_authority::tests
portal_authority_scratch="$(mktemp -d)"
trap 'rm -rf -- "$portal_authority_scratch"' EXIT
for pair in \
  ios_simulator:aarch64-apple-ios-sim \
  ios_simulator:x86_64-apple-ios \
  android_qemu:aarch64-linux-android \
  android_qemu:x86_64-linux-android; do
  platform="${pair%%:*}"
  target="${pair#*:}"
  authority="$portal_authority_scratch/$platform-$target.json"
  ./scripts/e2e/write-portal-profile-authority.sh "$platform" "$target" "$authority"
  expected="{\"platform\":\"$platform\",\"profile\":\"standalone-local-development-portal\",\"schema\":\"oxid-app-profile-authority-v2\",\"target\":\"$target\"}"
  [ "$(cat "$authority")" = "$expected" ] || {
    echo "Portal profile authority writer drifted for $pair" >&2
    exit 1
  }
  [ "$(LC_ALL=C ls -ld "$authority" | cut -c2-10)" = "rw-------" ] || {
    echo "Portal profile authority is not owner-private for $pair" >&2
    exit 1
  }
done
if ./scripts/e2e/write-portal-profile-authority.sh \
  ios_simulator aarch64-apple-ios "$portal_authority_scratch/physical-ios.json" \
  >/dev/null 2>&1 || \
  ./scripts/e2e/write-portal-profile-authority.sh \
    android_qemu armv7-linux-androideabi "$portal_authority_scratch/physical-android.json" \
    >/dev/null 2>&1; then
  echo "Portal profile authority admitted a physical/unauthorized target" >&2
  exit 1
fi
rm -rf -- "$portal_authority_scratch"
trap - EXIT
android_qemu_guard_line="$(grep -nF 'ro.kernel.qemu' scripts/run-android-emulator.sh | tail -1 | cut -d: -f1)"
for runtime_authority_marker in \
  verify_android_portal_virtual_device_profile \
  verify_android_qemu_profile \
  virtualDeviceProfileJson \
  'fun oxidVirtualDeviceProfileJson()' \
  'hardware == "ranchu"' \
  'hardware == "goldfish"'; do
  rg -qF "$runtime_authority_marker" apps/oxid/src/main.rs crates/composition/src \
    crates/adapters/mobile-native-plugin/src/lib.rs \
    apps/oxid/android/MainActivity.kt \
    crates/adapters/mobile-native-plugin/android/src/main/kotlin/io/medianox/oxid/mobile/OxidMobilePlugin.kt || {
    echo "Android Portal runtime authority is missing: $runtime_authority_marker" >&2
    exit 1
  }
done
android_authority_line="$(grep -nF 'write-portal-profile-authority.sh' scripts/run-android-emulator.sh | cut -d: -f1)"
if ! [[ "$android_qemu_guard_line" =~ ^[0-9]+$ && "$android_authority_line" =~ ^[0-9]+$ ]] || \
  [ "$android_qemu_guard_line" -ge "$android_authority_line" ]; then
  echo "Android Portal authority must be created only after live QEMU validation" >&2
  exit 1
fi
ios_simulator_guard_line="$(grep -nF 'OXID_IOS_DEVICE does not identify an installed simulator' scripts/run-ios-simulator.sh | cut -d: -f1)"
ios_authority_line="$(grep -nF 'write-portal-profile-authority.sh' scripts/run-ios-simulator.sh | cut -d: -f1)"
if ! [[ "$ios_simulator_guard_line" =~ ^[0-9]+$ && "$ios_authority_line" =~ ^[0-9]+$ ]] || \
  [ "$ios_simulator_guard_line" -ge "$ios_authority_line" ]; then
  echo "iOS Portal authority must be created only after live Simulator validation" >&2
  exit 1
fi
for ios_portal_build_input in \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256 \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_PATH \
  OXID_BUILD_PORTAL_PROFILE_AUTHORITY_SHA256; do
  rg -qF "$ios_portal_build_input" scripts/run-ios-simulator.sh || {
    echo "iOS Portal build input is missing: $ios_portal_build_input" >&2
    exit 1
  }
done

# This strict/static gate must execute the trigger failure, worker-bound, and
# one-item reservation tests rather than merely compiling their feature.
cargo test -p oxid-adapter-identity-ingress --features loopback-test-offer-trigger

# Portal is a separate mobile-only test profile. Host/desktop, tailnet, and
# native-custody combinations must fail before they can select composition.
if cargo check -p oxid-composition --features mobile-portal >"$failure_log" 2>&1; then
  echo "mobile-portal compiled directly for a non-mobile host" >&2
  exit 1
fi
if ! rg -q 'mobile-portal is available only on iOS and Android' "$failure_log"; then
  echo "direct mobile-portal rejection failed for an unexpected reason" >&2
  sed -n '1,120p' "$failure_log" >&2
  exit 1
fi
if cargo check -p oxid-app --no-default-features \
  --features desktop,oxid-composition/mobile-portal \
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
if ! rg -q 'standalone-portal requires repository virtual-device profile authority|standalone-portal is available only on iOS and Android|mobile-portal is available only on iOS and Android' "$failure_log"; then
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
  if ! rg -q 'standalone-portal requires repository virtual-device profile authority|standalone-portal is incompatible with tailnet and native custody|mobile-portal is available only on iOS and Android' "$failure_log"; then
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
fi

# Inspect the actual normal release binary, not only Cargo feature metadata.
# The stable marker is emitted into every developer-profile UI and must be
# absent from a distributed default artifact.
if [[ "$mode" != "--guards" ]]; then
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
if rg -a -q 'OXID_STANDALONE_DEPLOYMENT_PROFILE' "$release_binary"; then
  echo "normal release binary contains the standalone deployment projection" >&2
  exit 1
fi
if rg -a -q 'OXID_STANDALONE_PORTAL_PROFILE' "$release_binary"; then
  echo "normal release binary contains the standalone Portal profile" >&2
  exit 1
fi
if rg -a -q \
  'OXID_PUBLIC_STANDALONE_GENESIS_WALLET|Public genesis wallet capability|publicly spendable test authority|Oxid Demo Wallet' \
  "$release_binary"; then
  echo "normal release binary contains the public standalone genesis warning or marker" >&2
  exit 1
fi
if rg -a -q \
  'OXID_DESKTOP_PORTAL_TEST_PROFILE|portal-offer\.capability|Oxid Desktop Test' \
  "$release_binary"; then
  echo "normal release binary contains the ARM64 desktop test profile" >&2
  exit 1
fi
standalone_local_release_values=(
  'OXID_STANDALONE_LOCAL_PROFILE'
  'ws://127.0.0.1:8088/api/v4/graphql/ws' # nosemgrep: javascript.lang.security.detect-insecure-websocket.detect-insecure-websocket
  'http://127.0.0.1:8088/api/v4/graphql'
  'ws://127.0.0.1:9944' # nosemgrep: javascript.lang.security.detect-insecure-websocket.detect-insecure-websocket
  'http://127.0.0.1:6300'
)
for forbidden_value in "${standalone_local_release_values[@]}"; do
  if rg -a -F -q -- "$forbidden_value" "$release_binary"; then
    echo "normal release binary contains the standalone local profile or its routes" >&2
    exit 1
  fi
done
if rg -a -q \
  'OXID_STANDALONE_FUNDER_SEED_HEX|OXID_ENABLE_LIVE_STANDALONE_FUNDING|Ephemeral funded recipient|Standalone funding authority|Ephemeral shielded recipient|Standalone shielded funding authority' \
  "$release_binary"; then
  echo "normal release binary contains the standalone funding harness" >&2
  exit 1
fi
if rg -a -q \
  'OXID_PREPROD_MASTER_SEED_HEX|OXID_ENABLE_LIVE_PREPROD_E2E|OXID_PREPROD_E2E_CASE_INDEX|OXID_PREPROD_E2E_STATE_DIR|OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY|OXID_PREPROD_FUNDING_MANIFEST_V[12]|OXID_PREPROD_FUNDING_OBSERVATION_V1|Preprod E2E wallet A|Preprod E2E wallet B|oxid-preprod-registration-e2e-2026-08|lace-proof-pub\.preprod\.midnight\.network|Recover existing PreProd wallet|Midnight wallet root \(64 lowercase hex characters\)' \
  "$release_binary"; then
  echo "normal release binary contains the preprod registration funding harness" >&2
  exit 1
fi
fi

case "$mode" in
  --guards) echo "UI profile compile guards passed." ;;
  --artifact) echo "UI dev/demo/local/tailnet release exclusion passed." ;;
  all) echo "UI profile compile guards and dev/demo/local/tailnet release exclusion passed." ;;
esac
