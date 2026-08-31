---
name: oxid-portal-e2e
description: "Load for Lace ID Portal E2E local, simulator, and physical Android Tailnet work."
---

# Lace ID Portal E2E

Read the authoritative [macOS laptop](../../../docs/factory/portal-macos-laptop.md),
[mobile simulator](../../../docs/factory/portal-mobile-simulators.md), and
[physical Android Tailnet](../../../docs/factory/portal-android-tailnet-physical.md)
runbooks before mutating host, Docker, device, or Tailscale state.

Missing standalone listeners `6300`, `8088`, or `9944` are normally actionable,
not a blocker: inspect ownership, start Docker Desktop only when authorized,
then run `just standalone-up`. Preserve a healthy pre-existing stack unless its
owner authorizes recreation. Tear down only a stack owned by this session or
expressly authorized for removal.

Use the local ladder first:

```bash
just portal-macos-laptop-e2e
OXID_XCODE_DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
OXID_IOS_RUNTIME_ID='<explicit-reviewed-runtime-id>' \
OXID_IOS_DEVICE_TYPE_ID='<explicit-reviewed-iphone-device-type-id>' \
OXID_ANDROID_AVD='<explicit-reviewed-avd>' \
just portal-mobile-simulators-e2e
```

These are explicit, operator-private virtual selectors; never infer a target.
Local QEMU requires an empty ADB inventory, so disconnect physical phones first.
A physical phone never substitutes for simulator evidence.

The separate physical lane is `just android-portal-tailnet-physical-smoke`.
Require Tailscale online on the Mac and phone, exactly one approved non-QEMU ADB
device, and validated HTTPS Serve configuration without logging identities. It
never accepts a simulator substitute.

For every retry create a completely fresh offer, capability, app state, and
runtime; never reuse a consumed offer. Preserve explicit consent, zero secret
calls before consent, encrypted persistence, a true process restart, listing,
and fresh reverification. Publish only redacted mode-`0600` evidence that names
the exact `HEAD` and tree, after receipt- and process-owner-safe cleanup.

Local evidence is loopback/pinned-development evidence. Tailnet evidence is
Tailscale HTTPS/tailnet-development evidence. Neither is production trust or a
live-KYC claim.

```json oxid-portal-e2e-contract-v1
{
  "schema": "oxid-portal-e2e-skill-contract-v1",
  "commands": {
    "localLadder": ["just portal-macos-laptop-e2e", "just portal-mobile-simulators-e2e"],
    "physicalTailnet": "just android-portal-tailnet-physical-smoke"
  },
  "local": {
    "virtualSelectors": {
      "privacy": "operator-private-explicit",
      "required": ["OXID_XCODE_DEVELOPER_DIR", "OXID_IOS_RUNTIME_ID", "OXID_IOS_DEVICE_TYPE_ID", "OXID_ANDROID_AVD"]
    },
    "adb": {
      "inventory": "empty-before-qemu",
      "physicalPhones": "disconnected",
      "physicalPhoneEvidence": "never-simulator-substitute"
    }
  },
  "physicalTailnet": {
    "tailscale": ["mac-online", "phone-online", "serve-validated-without-identities"],
    "adb": { "count": 1, "device": "approved-non-qemu", "simulatorSubstitution": false }
  },
  "standalone": {
    "ports": [6300, 8088, 9944],
    "missingListeners": "actionable",
    "inspectOwnership": true,
    "dockerDesktop": "start-if-authorized",
    "start": "just standalone-up",
    "preservePreexistingHealthy": true,
    "teardown": "session-owned-or-explicit-owner-authorization"
  },
  "safety": {
    "retries": "fresh-offer-capability-app-runtime",
    "neverReuseConsumedOffers": true,
    "acceptance": ["explicit-consent", "pre-consent-zero-secret-calls", "encrypted-persistence", "true-restart", "listing", "fresh-reverification"],
    "evidence": { "mode": "0600", "redacted": true, "exactHeadAndTree": true },
    "cleanup": "receipt-and-process-owner-safe"
  },
  "boundaries": {
    "local": { "network": "loopback", "trust": "pinned-development" },
    "tailnet": { "network": "tailscale-https", "trust": "tailnet-development" },
    "prohibitedClaims": ["production-trust", "live-kyc"]
  }
}
```
