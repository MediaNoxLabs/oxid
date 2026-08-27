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

run:
    cargo run -p oxid-app

headless:
    cargo run -p oxid-headless

portal-headless-e2e:
    ./scripts/e2e/portal-headless-e2e.sh

# Start the virtual-mobile Portal issuer, resolver, offer endpoint, and authenticated manifest.
portal-virtual-mobile-stack:
    ./scripts/e2e/portal-virtual-mobile-stack.sh

# Verify the real virtual-mobile endpoints, single-use offer, manifest, and exact cleanup.
portal-virtual-mobile-stack-contract:
    ./scripts/e2e/portal-virtual-mobile-stack.sh --contract-test

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

# Verify physical Portal evidence is derived from exact measured results.
portal-android-evidence-contract:
    node --test ./scripts/e2e/portal-android-evidence.test.mjs

# Run strict Portal issuance, encrypted restart, and fresh reverification on a discovered physical Android device.
android-portal-tailnet-physical-smoke:
    ./scripts/test-android-portal-tailnet-physical.sh

# Verify exact-sequence process ownership and bounded process-group cleanup without Android or Docker.
android-portal-avd-safety-contract:
    ./scripts/e2e/android-avd-process-ownership.test.sh

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
