set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

check:
    ./run.sh --light --strict

full:
    ./run.sh --strict

fmt:
    cargo fmt --all

test:
    cargo test --workspace

coverage:
    ./run.sh coverage --strict

lint:
    cargo clippy --workspace --all-targets -- -D warnings

architecture:
    ./scripts/check-architecture.sh

sources:
    ./scripts/check-midnight-sources.sh

pi-smoke:
    ./scripts/check-pi-devshell.sh

factory-smoke:
    ./scripts/check-pi-devshell.sh
    node scripts/git-hooks/configure.mjs check

run:
    cargo run -p oxid-app

headless:
    cargo run -p oxid-headless

portal-headless-e2e:
    ./scripts/e2e/portal-headless-e2e.sh

# Run the canonical macOS laptop lane and require same-head evidence from both existing harnesses.
portal-macos-laptop-e2e:
    just portal-headless-e2e
    just portal-desktop-e2e
    jq -s -e \
      --arg head "$(git rev-parse HEAD)" \
      --arg tree "$(git rev-parse 'HEAD^{tree}')" \
      'length == 2 and all(.[]; .oxid == {head:$head,tree:$tree})' \
      target/portal-headless-e2e/evidence.json \
      target/portal-desktop-e2e/evidence.json
    echo "portal-macos-laptop-e2e: PASS evidence=target/portal-headless-e2e/evidence.json,target/portal-desktop-e2e/evidence.json"

# Run the owner-invoked ARM64-Darwin Dioxus Portal journey.
portal-desktop-e2e:
    ./scripts/e2e/portal-desktop-e2e.sh

# Start the virtual-mobile Portal issuer, resolver, offer endpoint, and authenticated manifest.
portal-virtual-mobile-stack:
    ./scripts/e2e/portal-virtual-mobile-stack.sh

# Verify the real virtual-mobile endpoints, single-use offer, manifest, and exact cleanup.
portal-virtual-mobile-stack-contract:
    ./scripts/e2e/portal-virtual-mobile-stack.sh --contract-test

# Verify pinned Portal image tags agree with the checked-out image archives.
portal-consumer-lifecycle-contract:
    ./scripts/e2e/portal-consumer-lifecycle.test.sh

# Serve one externally prepared offer to the isolated virtual-mobile loopback endpoint.
portal-virtual-mobile-offer-harness:
    node ./scripts/e2e/portal-virtual-mobile-offer-harness.mjs

# Verify isolated offer port ownership, authentication, and replay rejection.
portal-virtual-mobile-offer-harness-contract:
    node ./scripts/e2e/portal-virtual-mobile-offer-harness.mjs --contract-test

# Drive one tailnet-origin vector contract through the Rust and JavaScript gates.
portal-tailnet-origin-contract:
    cargo test -p oxid-adapter-identity-ingress --features tailnet-test-offer-trigger tailnet_offer_profile_accepts_only_shared_contract_origins
    node --test ./scripts/e2e/tailnet-origin-policy.test.mjs

# Prove the pinned Portal browser journey stays on one temporary Tailnet HTTPS origin.
portal-tailnet-browser-e2e:
    ./scripts/e2e/portal-tailnet-browser-e2e.sh

# Verify physical Portal evidence is derived from exact measured results.
portal-android-evidence-contract:
    node --test ./scripts/e2e/portal-android-evidence.test.mjs

# Verify Android issue-error waits for its post-failure review state before proxy restoration.
portal-android-flow-contract:
    node --test ./tests/mobile/android-portal-flow.test.mjs

# Run strict Portal issuance, encrypted restart, and fresh reverification on a discovered physical Android device.
android-portal-tailnet-physical-smoke:
    ./scripts/test-android-portal-tailnet-physical.sh

# Start a fresh, owner-invoked physical Android Portal QR demo; it is not E2E evidence.
portal-tailnet-manual-start:
    ./scripts/test-android-portal-tailnet-physical.sh manual-start

# Report only receipt-supervised manual-demo readiness; this never reveals payloads.
portal-tailnet-manual-status:
    ./scripts/test-android-portal-tailnet-physical.sh manual-status

# Stop one receipt-supervised manual demo and restore its exact prior Serve baseline.
portal-tailnet-manual-stop:
    ./scripts/test-android-portal-tailnet-physical.sh manual-stop

# Verify exact-sequence process ownership and bounded process-group cleanup without Android or Docker.
android-portal-avd-safety-contract:
    ./scripts/e2e/android-avd-process-ownership.test.sh

# Verify disposable-simulator selection, receipt identity, and bounded exact cleanup without a simulator.
ios-portal-simulator-safety-contract:
    ./scripts/e2e/ios-simulator-ownership.test.sh

# Verify the shared closed virtual-mobile evidence schema, derivation, redaction, and publication.
portal-virtual-mobile-evidence-contract:
    node --test ./scripts/e2e/portal-virtual-mobile-evidence.test.mjs

# Build and exercise the packaged Portal profile on one explicit owned Android QEMU AVD.
android-portal-exact-sequence-avd:
    @timeout --preserve-status -k 180s 14400s ./scripts/test-android-portal-exact-sequence-avd.sh

# Build and exercise the packaged Portal profile on one newly created disposable iOS Simulator.
ios-portal-exact-sequence-simulator:
    @timeout -k 30s 7200s ./scripts/test-ios-portal-exact-sequence-simulator.sh

# Preflight both virtual targets, prequalify shared macOS behavior, then run iOS before Android.
portal-mobile-simulators-e2e:
    @mkdir -p tmp/issue-213
    @./scripts/test-ios-portal-exact-sequence-simulator.sh --preflight >tmp/issue-213/aggregate-ios-preflight.log 2>&1 || { printf '%s\n' 'portal-mobile-simulators-e2e: FAIL phase=ios-preflight' >&2; exit 1; }
    @./scripts/test-android-portal-exact-sequence-avd.sh --preflight >tmp/issue-213/aggregate-android-preflight.log 2>&1 || { printf '%s\n' 'portal-mobile-simulators-e2e: FAIL phase=android-preflight' >&2; exit 1; }
    @timeout -k 30s 7200s just portal-macos-laptop-e2e >tmp/issue-213/aggregate-macos.log 2>&1 || { printf '%s\n' 'portal-mobile-simulators-e2e: FAIL phase=macos-prequalification' >&2; exit 1; }
    @timeout -k 30s 7200s ./scripts/test-ios-portal-exact-sequence-simulator.sh
    @timeout --preserve-status -k 180s 14400s ./scripts/test-android-portal-exact-sequence-avd.sh
    @jq -s -e --arg head "$(git rev-parse HEAD)" --arg tree "$(git rev-parse 'HEAD^{tree}')" 'length == 4 and all(.[]; .oxid == {head:$head,tree:$tree}) and (.[2].platform.kind == "ios_simulator") and (.[3].platform.kind == "android_emulator")' target/portal-headless-e2e/evidence.json target/portal-desktop-e2e/evidence.json target/ios-portal-exact-sequence-simulator/evidence.json target/android-portal-exact-sequence-avd/evidence.json >/dev/null
    @echo "portal-mobile-simulators-e2e: PASS evidence=target/ios-portal-exact-sequence-simulator/evidence.json,target/android-portal-exact-sequence-avd/evidence.json"

standalone-recovery-smoke:
    cargo test -p oxid-composition standalone_composition_recovers_a_complete_wallet_into_a_fresh_instance

standalone-up:
    ./scripts/standalone-up.sh local

standalone-funded-finality:
    ./scripts/test-standalone-funded-finality.sh

standalone-funded-shielded-finality:
    ./scripts/test-standalone-funded-shielded-finality.sh

preprod-registration-funding-manifest:
    ./scripts/derive-preprod-registration-funding-manifest.sh

preprod-registration-observe:
    ./scripts/observe-preprod-registration-funding.sh

preprod-registration-e2e:
    ./scripts/test-preprod-registration-e2e.sh

standalone-phone-up:
    ./scripts/standalone-up.sh phone

standalone-down:
    ./scripts/standalone-down.sh

ios-run:
    ./scripts/run-ios-simulator.sh

ios-standalone-local:
    OXID_STANDALONE_NETWORK_PROFILE=local ./scripts/run-ios-simulator.sh

ios-dev:
    OXID_UI_PROFILE=dev ./scripts/run-ios-simulator.sh

ios-demo:
    OXID_UI_PROFILE=demo ./scripts/run-ios-simulator.sh

ui-profile-release:
    ./scripts/check-ui-profile-release.sh

ios-smoke:
    ./scripts/test-ios-profile-flow.sh

ios-standalone-local-smoke:
    ./scripts/test-ios-standalone-local.sh

ios-dev-smoke:
    ./scripts/test-ios-developer-profile.sh

ios-demo-smoke:
    ./scripts/test-ios-demo-profile.sh

ios-backup-smoke:
    ./scripts/test-ios-backup-flow.sh

ios-native-custody-smoke:
    ./scripts/test-ios-native-custody.sh

android-run:
    ./scripts/run-android-emulator.sh

android-standalone-local:
    OXID_STANDALONE_NETWORK_PROFILE=local ./scripts/run-android-emulator.sh

android-dev:
    OXID_UI_PROFILE=dev ./scripts/run-android-emulator.sh

android-demo:
    OXID_UI_PROFILE=demo ./scripts/run-android-emulator.sh

android-phone:
    ./scripts/run-android-tailnet.sh

android-phone-ingress mode:
    ./scripts/test-android-identity-ingress-physical.sh {{quote(mode)}}

android-dev-smoke:
    ./scripts/test-android-developer-profile.sh

android-demo-smoke:
    ./scripts/test-android-demo-profile.sh

android-smoke:
    ./scripts/test-android-profile-flow.sh

android-standalone-local-smoke:
    ./scripts/test-android-standalone-local.sh

android-backup-smoke:
    ./scripts/test-android-backup-flow.sh

android-native-custody-smoke:
    ./scripts/test-android-native-custody.sh

nix-check:
    nix flake check --print-build-logs

presentation-compact-artifacts:
    nix build .#presentation-compact-artifacts --print-build-logs

clean:
    ./run.sh clean

docs-site:
    ./scripts/build-docs-site.sh
