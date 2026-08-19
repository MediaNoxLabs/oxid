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

run:
    cargo run -p oxid-app

headless:
    cargo run -p oxid-headless

standalone-recovery-smoke:
    cargo test -p oxid-composition standalone_composition_recovers_a_complete_wallet_into_a_fresh_instance

ios-run:
    ./scripts/run-ios-simulator.sh

ios-dev:
    OXID_UI_PROFILE=dev ./scripts/run-ios-simulator.sh

ui-profile-release:
    ./scripts/check-ui-profile-release.sh

ios-smoke:
    ./scripts/test-ios-profile-flow.sh

ios-dev-smoke:
    ./scripts/test-ios-developer-profile.sh

ios-backup-smoke:
    ./scripts/test-ios-backup-flow.sh

ios-native-custody-smoke:
    ./scripts/test-ios-native-custody.sh

android-run:
    ./scripts/run-android-emulator.sh

android-dev:
    OXID_UI_PROFILE=dev ./scripts/run-android-emulator.sh

android-dev-smoke:
    ./scripts/test-android-developer-profile.sh

android-smoke:
    ./scripts/test-android-profile-flow.sh

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
