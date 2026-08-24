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

portal-headless-e2e stack_env_file:
    ./scripts/e2e/portal-headless-e2e.sh {{quote(stack_env_file)}}

# Runs the real landed Portal through the two native mobile test frameworks in
# a fixed sequence. Never parallelize these platform suites.
portal-mobile-smoke stack_env_file:
    STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-ios-portal-flow.sh
    STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-android-portal-flow.sh

ios-portal-smoke stack_env_file:
    STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-ios-portal-flow.sh

android-portal-smoke stack_env_file:
    STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/test-android-portal-flow.sh

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

# Complete local-only retained Portal evidence: headless, iOS Portal + standard
# smoke, then Android Portal + standard smoke, all at one immutable Oxid head.
portal-local-conformance stack_env_file:
    STACK_ENV_FILE={{quote(stack_env_file)}} ./scripts/e2e/portal-local-conformance.sh {{quote(stack_env_file)}}

# Start/attach the reviewed shared Midnight owner, then start Portal-only services.
local-headless-up stack_env_file:
    ./scripts/local-headless.sh up {{quote(stack_env_file)}}

# Report one closed status document for the two exact owner projects.
local-headless-status stack_env_file:
    ./scripts/local-headless.sh status {{quote(stack_env_file)}}

# Run strict live headless issuance against an already-ready shared environment.
local-headless-test stack_env_file:
    ./scripts/local-headless.sh test {{quote(stack_env_file)}}

# Stop Portal first; stop Midnight only with the exact private owner receipt.
local-headless-down stack_env_file:
    ./scripts/local-headless.sh down {{quote(stack_env_file)}}
