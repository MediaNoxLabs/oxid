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

# Read-only weekly inventory for review-derived controlled debt.
follow-up-audit:
    node scripts/github/audit-follow-up-debt.mjs --repo MediaNoxLabs/oxid

run:
    cargo run -p oxid-app

desktop-build:
    cargo build -p oxid-app

desktop-run:
    cargo run -p oxid-app

# Live standalone replay uses optimized cryptography without release behavior.
desktop-live-build:
    cargo build --profile desktop-live -p oxid-app --no-default-features --features desktop,standalone-development,standalone-local

desktop-live-run:
    cargo run --profile desktop-live -p oxid-app --no-default-features --features desktop,standalone-development,standalone-local

desktop-proof-benchmark-build:
    cargo build -p oxid-app --no-default-features --features desktop,developer-proof-benchmark

desktop-proof-benchmark-run:
    cargo run -p oxid-app --no-default-features --features desktop,developer-proof-benchmark

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

# Owner-invoked ARM64 macOS rendered developer pager smoke; captures are private.
developer-pager-desktop-e2e:
    ./scripts/e2e/developer-pager-desktop-e2e.sh

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

# Prove the Tailnet KYC mount preserves the exact upstream Smocker request path.
portal-tailnet-route-contract:
    node --test ./scripts/e2e/tailnet-mock-route.test.mjs

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

# Prepare or resume the three exact pinned Portal images without requiring a phone or Tailnet.
portal-tailnet-manual-prepare:
    ./scripts/test-android-portal-tailnet-physical.sh manual-prepare

# Verify the exact pinned Portal source and prepared image receipt without starting Tailnet or a device.
portal-tailnet-manual-prepared-status:
    ./scripts/test-android-portal-tailnet-physical.sh manual-prepared-status

# Start a state-preserving demo from a completed preparation receipt; it is not E2E evidence.
portal-tailnet-manual-start:
    ./scripts/test-android-portal-tailnet-physical.sh manual-start

# Report only receipt-supervised manual-demo readiness; this never reveals payloads.
portal-tailnet-manual-status:
    ./scripts/test-android-portal-tailnet-physical.sh manual-status

# Explicitly clear only Oxid application data on one authorized physical device.
portal-tailnet-manual-reset:
    ./scripts/test-android-portal-tailnet-physical.sh manual-reset

# Stop one receipt-supervised manual demo and restore its exact prior Serve baseline.
portal-tailnet-manual-stop:
    ./scripts/test-android-portal-tailnet-physical.sh manual-stop

# Statically validate the first repository-owned Taskflow without enabling its mutating Pi extension.
taskflow-portal-tailnet-verify:
    node ./scripts/factory/taskflow-static.mjs verify ./.pi/taskflows/flows/demos/portal-tailnet-prepare.json

# Render the zero-token bound arguments, phase order, and maximum agent-call count.
taskflow-portal-tailnet-plan:
    node ./scripts/factory/taskflow-static.mjs plan ./.pi/taskflows/flows/demos/portal-tailnet-prepare.json '{"mode":"prepare-only"}'

# Render the reviewable Mermaid DAG and static verification report.
taskflow-portal-tailnet-compile:
    node ./scripts/factory/taskflow-static.mjs compile ./.pi/taskflows/flows/demos/portal-tailnet-prepare.json

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

standalone-status:
    ./scripts/standalone-status.sh local

standalone-public-balances:
    OXID_ENABLE_LIVE_STANDALONE_BALANCES=1 cargo test -p oxid-composition --features standalone-development standalone_funding_tests::public_standalone_genesis_balances_are_exact -- --ignored --exact

standalone-funded-finality:
    ./scripts/test-standalone-funded-finality.sh

standalone-faucet:
    ./scripts/run-standalone-faucet.sh

standalone-faucet-http:
    ./scripts/run-standalone-faucet-http.sh

standalone-faucet-headless-e2e:
    ./scripts/e2e/standalone-faucet-headless-e2e.sh

standalone-night-round-trip-headless-e2e:
    ./scripts/e2e/standalone-night-round-trip-headless-e2e.sh

standalone-faucet-http-headless-e2e:
    ./scripts/e2e/standalone-faucet-http-headless-e2e.sh

# Owner-invoked private HTTPS discovery/funding lifecycle; never starts a phone.
standalone-faucet-tailnet-start:
    ./scripts/standalone-faucet-tailnet.sh start

standalone-faucet-tailnet-status:
    ./scripts/standalone-faucet-tailnet.sh status

standalone-faucet-tailnet-stop:
    ./scripts/standalone-faucet-tailnet.sh stop

standalone-faucet-tailnet-accept:
    ./scripts/standalone-faucet-tailnet.sh accept

standalone-faucet-tailnet-lifecycle-test:
    node --test ./scripts/e2e/standalone-faucet-tailnet-lifecycle.test.mjs

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

# Receipt-scoped Tailnet preparation for issue #556. It reuses the existing
# stack and faucet boundaries and never replaces unrelated Serve configuration.
standalone-tailnet-round-trip-start:
    ./scripts/standalone-tailnet-round-trip.sh start

standalone-tailnet-round-trip-status:
    ./scripts/standalone-tailnet-round-trip.sh status

standalone-tailnet-round-trip-stop:
    ./scripts/standalone-tailnet-round-trip.sh stop

standalone-down:
    ./scripts/standalone-down.sh

ios-run:
    ./scripts/run-ios-simulator.sh

ios-build:
    ./scripts/run-ios-simulator.sh build

ios-deploy:
    ./scripts/run-ios-simulator.sh deploy

ios-standalone-local:
    OXID_STANDALONE_NETWORK_PROFILE=local ./scripts/run-ios-simulator.sh

# Uses only the private receipt created by standalone-tailnet-round-trip-start.
ios-standalone-tailnet:
    OXID_STANDALONE_NETWORK_PROFILE=tailnet ./scripts/run-ios-simulator.sh

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

# Prove selected-realm recovery on one explicitly selected, receipt-owned simulator.
ios-wallet-lifecycle-simulator:
    @timeout -k 30s 1800s ./scripts/test-ios-wallet-lifecycle-simulator.sh

android-run:
    ./scripts/run-android-emulator.sh

android-build:
    ./scripts/run-android-emulator.sh build

# Build an arm64 release candidate and write a private static-check receipt.
# This command never selects, boots, installs to, or launches an Android target.
android-release-build:
    env -u RUSTC_WRAPPER ./scripts/build-android-release-candidate.sh

android-deploy:
    ./scripts/run-android-emulator.sh deploy

# Smoke the exact receipt-bound artifact produced by `just android-release-build`.
android-smoke-prebuilt apk="target/android-release-candidate/oxid-app-arm64-v8a-release.apk" receipt="target/android-release-candidate/receipt.json":
    ./scripts/test-android-profile-flow.sh --apk {{quote(apk)}} --receipt {{quote(receipt)}}

# Inspect an existing APK only; this does not build, install, or start Android.
# Override apk= with the exact release artifact selected for milestone evidence.
android-verify-16k apk="target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk":
    node ./scripts/android-verify-16k.mjs {{quote(apk)}}

android-standalone-local:
    OXID_STANDALONE_NETWORK_PROFILE=local ./scripts/run-android-emulator.sh

android-dev:
    OXID_UI_PROFILE=dev ./scripts/run-android-emulator.sh

android-demo:
    OXID_UI_PROFILE=demo ./scripts/run-android-emulator.sh

android-phone:
    ./scripts/run-android-tailnet.sh

# Owner-invoked physical/mobile read-only PreProd recovery. Seed material is
# entered only in the native application and never crosses this launcher.
android-preprod-observe:
    OXID_MOBILE_CUSTODY=native OXID_PREPROD_OBSERVATION=1 ./scripts/run-android-emulator.sh

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
