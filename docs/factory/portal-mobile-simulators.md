# Portal mobile simulator lane

## Purpose and boundary

This owner-invoked L4 lane runs the authenticated Lace ID Portal scenario in
actual packaged `oxid-app` builds on one newly created iOS Simulator and one
explicit repository-owned Android QEMU AVD. It is development evidence only:
it is not a hosted gate, physical-device, camera, Tailscale, native-custody,
release, live-DIDIT, or performance evidence.

The profile reuses Portal integration commit
`22ae5369b6f939e6b20648f4b85dd993527748ef`, tree
`74d8d1a5b87c160ea554006e47d5f3edc3cd3e10`, deployment schema v3, profile
authority v2, and the existing one-shot loopback capability. The capability
authenticates the app to the listener; plaintext loopback does not authenticate
the listener to the app. Strict routing, explicit consent, issuer trust, proof,
holder binding, and encrypted storage remain the security boundary.

## Prerequisites

- Apple-silicon macOS with an active WindowServer session.
- A full Xcode installation selected by an absolute
  `OXID_XCODE_DEVELOPER_DIR`; do not change host-global `xcode-select`.
- Explicit available iOS runtime and iPhone device-type identifiers.
- Android SDK/platform-tools/emulator and an explicit reviewed QEMU AVD.
- Nix shell, Docker Desktop, Git/network, Cargo/rustup, Node, Java, `jq`,
  `curl`, `shasum`, `timeout`, and XcodeGen.
- Installed `aarch64-apple-ios-sim` and Android Rust target for the selected AVD.
- A clean, committed, locally verifiable signed `HEAD` containing parent issue
  #210 commit `875b5e1c52f3d5699c058b14e256d23c1c3fc41c`.
- Exactly three healthy pre-existing `oxid-standalone` services on ports 6300,
  8088, and 9944; no Portal consumer, virtual stack lock, Portal listeners, or
  stale mobile evidence.
- No online ADB transport. A physical-only, mixed, unrelated-emulator, or wrong
  serial inventory fails before device mutation. A physical device must never
  be used as fallback evidence.

Inspect selectors without mutation:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  /usr/bin/xcrun simctl list runtimes -j
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  /usr/bin/xcrun simctl list devicetypes -j
"$ANDROID_HOME/emulator/emulator" -list-avds
```

## Contracts and canonical execution

Run the simulator-free and pre-mutation contracts first:

```bash
just portal-virtual-mobile-evidence-contract
just ios-portal-simulator-safety-contract
just android-portal-avd-safety-contract
just portal-virtual-mobile-offer-harness-contract
just portal-virtual-mobile-stack-contract
cargo test -p oxid-adapter-identity-ingress \
  --features loopback-test-offer-trigger
```

The official owner command is:

```bash
OXID_XCODE_DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
OXID_IOS_RUNTIME_ID='<explicit-reviewed-runtime-id>' \
OXID_IOS_DEVICE_TYPE_ID='<explicit-reviewed-iphone-device-type-id>' \
OXID_ANDROID_AVD='<explicit-reviewed-avd>' \
just portal-mobile-simulators-e2e
```

The aggregate performs read-only iOS and Android preflights, runs
`just portal-macos-laptop-e2e`, stops before mobile if shared prequalification
fails, runs iOS before Android, and stops before Android on any iOS or cleanup
failure. Finally it requires headless, desktop, iOS, and Android evidence to
name one current `HEAD` and `HEAD^{tree}`. The individual owner commands are:

```bash
just ios-portal-exact-sequence-simulator
just android-portal-exact-sequence-avd
```

Do not retry a platform automatically after an offer is armed. Resolve the
failure, prove cleanup, remove no ambiguous owner state, and start a fresh
complete run with fresh app data and offers.

## Scenario and evidence

Each platform runs, in order: cold route, holder preparation and authenticated
holder-DID synchronization, refusal, malformed metadata, unavailable protocol,
protocol timeout, issuance failure, successful issuance, encrypted-store
inspection, real process death, restart, listing, and fresh reverification.
Metadata preview calls are expected. Before explicit consent, token, nonce,
credential, and issuer-resolution deltas must all remain zero.

Successful issuance requires exact counters, one valid record, hidden sensitive
claims, the `OXIDVC01` envelope header, a 32-byte development key, and a
plaintext denylist pass. Restart is performed without uninstall, app-data
reset, keychain reset, or data deletion; the old process must be absent, the new
generation must differ, and Reverify must add exactly one resolver request and
success before the fresh `Credential reverification applied` marker appears.

Closed evidence is mode `0600`, published exclusively only after exact cleanup:

```text
target/ios-portal-exact-sequence-simulator/evidence.json
target/android-portal-exact-sequence-avd/evidence.json
```

Both use `oxid-portal-virtual-mobile-evidence-v1`. They contain only the Oxid
head/tree, reviewed Portal pins, schema versions, coarse virtual-platform facts,
artifact digest, standardized scenario/counter results, derived booleans, and
cleanup acceptance. They exclude simulator UDIDs and names, ADB serials and AVD
names, DIDs, URLs, offers, grants, tokens, nonces, credentials, claims, proofs,
capabilities, paths, PIDs, and timestamps. Private build sources, DerivedData,
XCTest results, and logs are removed before publication.

## Ownership and timeouts

iOS creates a uniquely named simulator and stores the exact returned UDID in an
owner-private receipt. Every operation is `simctl <operation> <receipt-UDID>`;
existing booted simulators are ignored. Cleanup revalidates runtime, device type,
name, UDID, receipt mode, and receipt filesystem identity before shutdown and
deletion. It never selects the first booted simulator, accepts keep-failed mode,
or kills shared CoreSimulator processes.

Android uses fixed port 5562/serial `emulator-5562`, launches the selected AVD as
a directly supervised read-only no-snapshot child, verifies QEMU and exact AVD
identity, scopes every ADB operation to that serial, and removes only its package
and mappings. Ambiguous process, mapping, package, evidence, listener, lock, or
receipt state is preserved for owner review and suppresses evidence.

Hard ceilings are 15–30 seconds for control/ADB/simctl operations, five minutes
for virtual-target boot, 60 minutes for Portal readiness, 75 minutes per
packaged build, ten minutes for the UI journey, three minutes for cleanup, two
hours per platform or macOS prequalification, and six hours for the aggregate.
These are safety limits, not performance measurements.
