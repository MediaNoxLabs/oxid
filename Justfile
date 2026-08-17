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

ios-run:
    ./scripts/run-ios-simulator.sh

ios-smoke:
    ./scripts/test-ios-profile-flow.sh

ios-native-custody-smoke:
    ./scripts/test-ios-native-custody.sh

android-run:
    ./scripts/run-android-emulator.sh

android-smoke:
    ./scripts/test-android-profile-flow.sh

android-native-custody-smoke:
    ./scripts/test-android-native-custody.sh

nix-check:
    nix flake check --print-build-logs

presentation-compact-artifacts:
    nix build .#presentation-compact-artifacts --print-build-logs

clean:
    ./run.sh clean
