# Mobile visual accessibility evidence (local only)

This matrix is the privacy-safe, exact-head evidence map for the simulated first-run journey and holder shell. It is additive to Android CDP and iOS XCTest coverage, never authorizes product changes, and never substitutes a Maestro result for a protected, protocol, or finality outcome. Generated screenshots and logs remain below ignored `target/mobile-visual-accessibility/<platform>/`; recovery phrases and other private values are never captured.

## Evidence matrix

| Scenario ID | Authority | Platform | Design reference/no-match | Artifact | Evidence layer | Known gap |
| --- | --- | --- | --- | --- | --- | --- |
| canonical-holder-evidence | maestro | iOS Simulator | Per-screen Lunar Aegis ID in the artifact manifest; `no-match` for fixture Activity | `lunar-aegis-ios-01` through `-08` public screenshots and receipt manifest metadata | iOS visual, 375 pt and 402 pt receipt-owned lanes | Screen-reader traversal remains a lane observation. |
| onboarding-safe-boundary | maestro | iOS Simulator; Android Emulator semantic | `XSwTg6CjwXruX8QP3tXy`, `FFMmLvVQlc5xIun63FYX` | receipt outcome only | Safe onboarding reachability; no recovery ceremony | Native focus-order detail remains harness-dependent. |
| profile-realm-switching | maestro | iOS Simulator; Android Emulator semantic | no-match | receipt outcome only | Simulated profile and realm traversal | Real-network reconciliation remains lower-layer evidence. |
| home-receive-send-blocked | maestro | iOS Simulator; Android Emulator semantic | `7u81lbjNIKcn8dS79axb` | canonical Home/Receive/Send screenshots on iOS; Android receipt | Public holder reachability and blocked send entry | No recipient, amount, or transfer action is entered. |
| wallet-sync-status | maestro | iOS Simulator; Android Emulator semantic | no-match | receipt outcome only | Simulated status reachability | Synchronization finality remains Rust/CDP/XCTest evidence. |
| documents-empty-state | maestro | iOS Simulator; Android Emulator semantic | no-match | canonical Documents screenshot on iOS; Android receipt | Empty-state reachability | Android protected screenshot is intentionally not retained. |
| did-inventory-empty-state | maestro | iOS Simulator; Android Emulator semantic | no-match | receipt outcome only | DID inventory reachability | DID creation and detail are lower-authoritative. |
| did-inventory-copy | maestro | iOS Simulator | no-match | receipt outcome only | Public DID creation, details, and copy feedback | Clipboard contents are asserted by focused Rust tests; no dynamic DID is retained in screenshots or logs. |
| credential-offer-refusal | maestro | iOS Simulator | no-match | receipt outcome only | Credential-offer consent and refusal boundary | Issuance, protocol finality, and private offer material remain lower-layer evidence. |
| credential-presentation-refusal | maestro | iOS Simulator | no-match | receipt outcome only | Presentation request review, explicit consent boundary, and refusal before proof generation | Protected proof generation, verifier finality, and private claim values remain lower-layer evidence. |
| activity-fixture-history | maestro | iOS Simulator; Android Emulator semantic | no-match | canonical Activity screenshot on iOS; Android receipt | Fixture-history traversal | Fixture data is not transaction-finality evidence. |
| passport-vault-entry | maestro | iOS Simulator; Android Emulator semantic | no-match | receipt outcome only | Vault entry reachability | Protected proof and terminal outcomes are lower-authoritative. |
| settings-security-backup-entry | maestro | iOS Simulator; Android Emulator semantic | no-match | canonical Settings screenshot on iOS; Android receipt | Settings traversal | Backup stays native XCTest/CDP evidence. |
| credential-detail | lower-authoritative-layer | none | no-match | `tests/mobile/android-wallet-flow.mjs`; `tests/mobile/ios/OxidUITests/ProfileFlowTests.swift`; `crates/ui-dioxus/src/lib.rs` | Credential detail | Maestro must not select holder data. |
| did-create-resolve-detail | lower-authoritative-layer | none | no-match | `tests/mobile/android-wallet-flow.mjs`; `tests/mobile/ios/OxidUITests/ProfileFlowTests.swift`; `crates/ui-dioxus/src/dids.rs` | DID create, resolve, and detail | Protected state is not a Maestro claim. |
| presentation-proof-siopv2 | lower-authoritative-layer | none | no-match | `tests/mobile/android-identity-ingress.mjs`; `tests/mobile/ios/OxidUITests/IdentityIngressTests.swift`; `tests/mobile/ios/OxidUITests/ProfileFlowTests.swift`; `crates/ui-dioxus/src/lib.rs` | Presentation proof/finality and SIOPv2 protocol outcomes | Protected proofs, private claims, request URIs, and dynamic selectors are excluded. |
| passport-vault-terminal-outcomes | lower-authoritative-layer | none | no-match | `tests/mobile/android-wallet-flow.mjs`; `tests/mobile/ios/OxidUITests/ProfileFlowTests.swift`; `crates/ui-dioxus/src/passport_vault.rs` | Vault terminal outcomes | Proof evidence remains protected. |
| dev-diagnostics-benchmark | manual-local | iOS Simulator | no-match | owner-local notes | Diagnostics, event log, and benchmark entry | No secret-free development bootstrap exists. |
| developer-profile-banner | maestro | iOS Simulator; Android Emulator semantic | developer banner, safe-area verified at 375 pt | `developer-profile-banner-open` and `-closed` screenshots on iOS; Android receipt | Developer-profile banner visibility and dismissal | Android remains semantic-only after the final sweep. |

## State boundaries

The matrix retains these observed boundaries: Welcome and create-vs-restore fork; Mandatory device-protection explanation; Recovery boundary and Ready/Home (`xYA9BiozNUetlxPJYHPT`); Receive and Send entry; Empty Documents and fixture Activity; and Settings and native-custody Backup boundary. Where a public visual artifact is permitted, inspect 375 pt/dp and larger width, safe-area/navigation non-overlap, 44 px touch targets, large-text truncation, non-color status meaning, deterministic Back, modal focus return, reduced motion, and screen-reader labels/order.

## Receipt-owned lanes

Run all safe `authority=maestro` iOS scenarios serially through the receipt-owned lane; it records the exact head, simulator device/runtime, duration, public screenshot count/bytes, cleanup, and a per-scenario outcome. The canonical public-evidence scenario runs last.

```sh
./bootstrap.sh -- ./scripts/test-ios-maestro-holder-evidence.sh
```

Run the larger iOS width as a separate receipt-owned simulator pass. Its receipt pins the same exact head, `holder-public` capture policy, iPhone 17 Pro device type, iOS 26.4 runtime, 402-point class viewport, and each artifact's route, state, actual `demo` or `dev` UI profile, and Lunar Aegis design reference or `no-match`; it never reuses an ambient simulator.

```sh
./bootstrap.sh -- ./scripts/test-ios-maestro-holder-evidence-large.sh
```

After both iOS lanes, run one final sweep only on a disposable `emulator-*` Android Emulator. It retains semantic outcomes only and deletes all Android screenshots and debug output, including failures.

```sh
OXID_ANDROID_DEVICE=emulator-<port> OXID_ANDROID_DISPOSABLE=1 \
  ./bootstrap.sh -- ./scripts/test-android-maestro-semantic-evidence.sh
```

The desktop compatibility check is limited to the existing developer pager at supported mobile-like widths; do not create another desktop harness.

```sh
just developer-pager-desktop-e2e
```

## Operating rules

- Maestro flows are local-only and additive; existing CDP and XCTest coverage remains authoritative.
- Maestro never enters the recovery ceremony: never capture a recovery phrase. The demo adapter omits native-custody Backup, and protected/protocol flows stay in their exact Rust/CDP/XCTest layer.
- Android applies `FLAG_SECURE` after demo protection initializes. It is semantic-only in the final sweep; protected screenshots and failure artifacts are deleted.
- A global Maestro state directory such as `~/.maestro/tests/` is never an accepted evidence path. Only receipt-owned, repository-scoped artifacts may be inspected or retained.
- Android must be re-masked before any unrelated operator use after a semantic sweep. CDP/XCTest/Rust evidence remains authoritative for protected and protocol behavior.
- Capture the canonical iOS public lane at 375 pt/dp-class width and a separate receipt-owned larger 402 pt width. A platform-harness limitation is evidence, not permission to weaken semantic assertions or capture sensitive data.
- Each retained iOS receipt records the exact head, `holder-public` capture policy, OS runtime, device type/UDID, viewport, and every public artifact's UI profile, route, state, and Lunar Aegis design ID or `no-match`.
- A reproduced navigation, overlap, privacy, focus, or misleading-state defect blocks this evidence slice. Cosmetic deltas require a linked follow-up rather than product-code changes here.
