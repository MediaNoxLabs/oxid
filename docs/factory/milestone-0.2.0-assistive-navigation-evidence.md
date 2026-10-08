# Milestone 0.2.0 assistive-navigation evidence

This note records a bounded, privacy-safe accessibility checkpoint for Oxid commit
`a000ec06aeffd5ea2a86fe3ad9180b07dc4e97bf`. It is diagnostic evidence for the
milestone candidate, not a claim that a human completed a VoiceOver or TalkBack
traversal.

## Exact-head result

| Lane | Result | Measured evidence |
| --- | --- | --- |
| iOS holder inventory | Passed | iOS Simulator 17.5, iPhone SE (3rd generation), 375-point class; 15/15 Maestro scenarios; 608 seconds; 10 bounded public screenshots; 5,711,481 public bytes |
| iOS native custody | Passed | `NativeCustodyTests/testNativeCompositionUsesDeviceCustodyOrFailsClosed`; 1/1 test; 8.608 seconds |
| Android semantic inventory | Deferred | The semantic runner requires an already operator-owned disposable `emulator-*`. A receipt-owned launcher that proves emulator identity, runs the sweep, and terminates only that emulator is the prerequisite for unattended evidence. No physical or ambient Android target was used. |

The iOS holder lane created its own simulator and reported
`receiptOwnedSimulator=true`, `privateDiagnosticsRemoved=true`, and
`rawArtifactsRemoved=true`. Its public receipt is retained below ignored
`target/mobile-visual-accessibility/`; no recovery phrase, holder identifier,
credential claim, address, personal device name, or protected Android image is
part of this note.

## What the checkpoint establishes

- The stable public names and ordering in the primary holder journeys remained
  reachable across onboarding, profile/realm selection, Home/Receive/Send,
  wallet status, Documents, DID inventory/copy, refusal boundaries, Activity,
  Settings/Security/Backup, and the development banner.
- Native XCTest observed the create/restore boundary, required device
  authorization, and the absence of a recovery phrase before authorization.
- Repository tests freeze privacy-safe DID accessible names, focus-visible
  header controls, 44-point header touch targets, deterministic navigation
  contracts, non-color status copy, modal semantics, and reduced-motion CSS.
- Maestro remains a semantic and visual reachability layer. Rust, XCTest, and
  CDP tests remain authoritative for protected state, keyboard/native custody,
  protocol outcomes, and finality.

## Remaining acceptance gaps

Issue #1049 remains open. Before accessibility acceptance, a human must still
record a real VoiceOver traversal on iOS and TalkBack traversal on Android,
including rotor/order, focus restoration after menus and sheets, and spoken
status meaning. The candidate also still needs an observed 200% text run and a
receipt-owned larger-width run. Android automation additionally needs the
receipt-owned disposable-emulator wrapper described above. These gaps must not
be inferred from the passing Maestro result.

## Reproduction

```sh
./bootstrap.sh -- ./scripts/test-ios-maestro-holder-evidence.sh
./bootstrap.sh -- just ios-native-custody-smoke
```

The first command owns and cleans its simulator. The second command must run
inside an explicitly receipt-owned simulator boundary; do not target an ambient
simulator or physical device. Android remains deferred until its semantic runner
owns the complete emulator lifecycle.
