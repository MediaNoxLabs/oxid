# Maestro mobile pilot (local only)

This is an opt-in black-box pilot for simulated debug builds. It complements and does not replace the existing Android CDP and iOS XCTest suites. It is not GitHub CI.

## Reproducible invocation

Maestro is supplied by the pinned `nixpkgs` input as `.#maestro` (currently 2.8.0); no global installation or download is used. `tests/maestro/inventory.json` is the closed scenario inventory. Build/install the simulated app through the repository launcher, then run one inventory-owned flow at a time:

```sh
OXID_IOS_DEVICE=<receipt-owned-udid> \
  ./scripts/run-maestro-ios.sh --composition demo --flow home-receive-send-blocked
OXID_ANDROID_DEVICE=emulator-<port> \
  ./scripts/run-maestro-android.sh --composition demo --flow home-receive-send-blocked
```

The wrappers reject unlisted flows, composition mismatches, and unsupported platforms. iOS is the canonical local lane. Its lock serializes receipt reuse, deployment, and Maestro against one explicit simulator. The Android wrapper stays selector-compatible, refuses anything except an `emulator-*` serial, and is deferred to the final platform pass. Never use a physical phone.

The iOS lease records its PID, host, start time, worktree, flow, and a random owner token. A live local owner remains serialized; a demonstrably dead local PID is recovered automatically. Remote, malformed, or permission-ambiguous ownership fails closed and prints the exact token-bound cleanup command. Run that command only after checking the reported owner; it can remove only the current `.maestro-lane.lock` lease and refuses a changed token. Acquisition, waiting, recovery, ambiguity, cleanup, and release are exposed as `factory-metrics` outcomes.

For the canonical 375-point iOS evidence lane, let the repository own the
simulator lifecycle instead of supplying an ambient device:

```sh
./bootstrap.sh -- ./scripts/test-ios-maestro-holder-evidence.sh
```

The runner creates an iPhone SE (3rd generation), builds or reuses and deploys the exact
clean-head receipt, runs every safe inventory-owned iOS scenario with
`canonical-holder-evidence` last, and deletes the receipt-owned simulator. It retains only the
eight canonical holder screenshots, two public developer-banner screenshots, one bounded
200-line Maestro tail per executed scenario when available, a scenario-to-artifact manifest at
`scenarios/manifest.jsonl`, and a machine-readable receipt that references that manifest below ignored
`target/mobile-visual-accessibility/ios-run-<head>-<started-at>/`. Raw per-device
Maestro/XCTest logs and diagnostics are deleted after collection, including on
failure. The receipt records duration, retry-independent outcome, public bytes,
screenshot count, exact head/device, and cleanup status.

After iOS, an operator may run one final semantic-only Android compatibility sweep on an
explicitly disposable emulator. The sweep records per-scenario outcomes and measured cleanup,
retains no Android screenshots or raw debug artifacts, and rejects a physical-device serial or
an empty scenario inventory:

```sh
OXID_ANDROID_DEVICE=emulator-<port> OXID_ANDROID_DISPOSABLE=1 \
  ./bootstrap.sh -- ./scripts/test-android-maestro-semantic-evidence.sh
```

## Flow and evidence contract

`tests/maestro/flows/` contains independently runnable, scenario-sized journeys. They reuse the clean launch, deterministic demo-profile bootstrap, and global-menu subflows in `tests/maestro/subflows/`. The compile-time demo bootstrap creates only a profile and process-local protection; it never derives an account, loads funding, enters a recovery ceremony, or starts an external service.

The inventory classifies each surface explicitly:

- `maestro`: safe reachability or public simulated state that a wrapper may execute.
- `lower-authoritative-layer`: protected, protocol, lifecycle, or finality behavior owned by Rust, headless, CDP, or XCTest coverage.
- `manual-local`: useful local evidence that has no secret-free clean-state bootstrap yet; it is never presented as an automated pass.

This means the black-box layer can reach Home, Receive, Send, Wallet status, Documents, DID inventory, Activity, Passport Vault, Settings, the safe onboarding boundary, credential-offer review, and credential-presentation review. The credential-offer flow proves that issuance consent remains explicit and refusal discards ephemeral protocol state. The presentation flow creates only the deterministic demo credential needed to reach the existing review, then refuses before proof generation; it never asserts private claim values or verifier finality. Development diagnostics and benchmark entry stay `manual-local` until a privacy-safe development-profile bootstrap exists. Maestro can produce automatic failure screenshots, so no flow enters recovery phrases, dynamic DID/credential values, protocol request URIs, recipients, amounts, or other private selectors. Only `canonical-holder-evidence` may request public screenshots; raw failure output remains private and is deleted by the evidence runner.

The older platform monoliths remain temporarily as compatibility fixtures for the final #798 platform audit; new execution uses only the inventory-owned modular flows. Existing CDP/XCTest/Rust/headless suites remain authoritative.

Android sets `androidWebViewHierarchy: devtools`. If Maestro cannot inspect the WebView despite the existing debuggable WebView/CDP seam, record that as the pilot blocker; do not weaken CDP coverage or add CI.

## Pilot recommendation

Keep this local while measuring one clean iOS then emulator run. Capture Maestro's output directory as a short-lived local artifact, redact before sharing, and record runtime/flake rate after at least ten clean runs. A practical future CI decision needs bounded runtime, <5% repeat-run flakes, retained/redacted artifacts, and a proven WebView hierarchy; none is established by this pilot.
