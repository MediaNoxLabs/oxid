# Milestone 0.2.0 iOS demo evidence

This note records the canonical local iOS evidence captured for issue #939. It
is bounded to public holder-shell behavior and the native custody authorization
boundary. It does not claim Android parity, live-network finality, credential
issuance, DID publication, or Passport Vault proof finality; those remain in
their authoritative test and demo lanes.

## Exact source and platform

- Milestone head: `7e0136ed66375256be31fb44374ff6c477761b55`
- Application artifact: receipt-verified iOS simulator bundle built from that
  head
- Device: disposable iPhone SE (3rd generation), 375-point class
- Runtime: iOS 17.5
- Ownership: one receipt-owned simulator per lane; both simulators were deleted
  after their lane completed

## Automated evidence

Run the public, privacy-safe holder inventory with:

```sh
./bootstrap.sh -- ./scripts/test-ios-maestro-holder-evidence.sh
```

The run passed all 11 inventory scenarios in 567 seconds:

- developer-profile banner open and dismiss
- Activity fixture history
- DID inventory empty state
- Documents empty state
- Home, Receive, and blocked Send entry
- safe onboarding boundary
- Passport Vault entry
- profile and realm chooser
- Settings, Security, and Backup entry
- wallet synchronization status
- canonical public holder evidence

The receipt reported ten public screenshots (5,355,968 bytes), removed private
diagnostics and raw Maestro artifacts, and deleted its simulator. Eight images
cover the canonical first-run, device-protection, Home, Receive, Send,
Documents, Activity, and Settings surfaces; two cover the developer notice open
and dismissed states. Generated evidence remains ignored under
`target/mobile-visual-accessibility/` and contains no recovery phrase or dynamic
holder value.

Run the independent native authorization boundary with:

```sh
OXID_IOS_DEVICE=<receipt-owned-booted-simulator> \
  ./bootstrap.sh -- just ios-native-custody-smoke
```

The focused XCTest passed 1/1 in 8.3 seconds. It proved that a clean install
offers both **Create private wallet** and **Restore from backup**, requires
**Generate recovery phrase**, exposes no legacy skip action, invokes the iOS
device authorization boundary, and does not expose a recovery phrase before
authorization succeeds.

## Manual demo checklist

Use the iOS simulator fast lane and a demo composition. Do not enter a real
recovery phrase or live protocol payload while capturing evidence.

1. Start from a clean app state and confirm **Create private wallet** and
   **Restore from backup** are both available.
2. Open wallet creation, continue to device protection, and stop before the
   protected recovery ceremony unless native authorization is the purpose of
   the demo.
3. Return to a clean demo state, open **Standalone demo setup**, select the
   public standalone profile, initialize process-local custody, and close the
   setup panel.
4. On Home, open and close **Receive**, then open **Send** and return without
   entering a recipient or amount.
5. Switch the active wallet profile and confirm the current realm remains
   explicit.
6. Open Wallet and inspect **Selected realm** and **Wallet status**.
7. Open Documents, verify the empty state, and open **Manage identities** to
   inspect the DID inventory empty state.
8. Open Activity and inspect the public sent/received fixture history.
9. Swipe the Home capability carousel and open **Passport Vault**; stop at its
   entry surface because protected proof outcomes are not claimed here.
10. Open Settings, then Security, return, and open Backup. Confirm the page
    describes one encrypted wallet document.
11. In a development composition, verify the developer notice can be dismissed
    for the current session.

## Deferred release evidence

- Android visual/semantic parity remains deferred to the final cross-platform
  pass under #798.
- Live issuer, verifier, Tailnet, faucet, DUST finality, DID publication, and
  Passport Vault proof scenarios require their dedicated environment-owned
  lanes.
- Legacy iOS fixtures still using the removed onboarding skip flow are tracked
  by #943 and do not weaken this focused native-custody result.
