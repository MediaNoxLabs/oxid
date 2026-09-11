# Issue #291 local acceptance record

This tracked manifest is intentionally redacted. The receipt files named below are
local mode-`0600` evidence and bind their completed lanes to the exact Oxid
`HEAD` and tree; they contain no device, Tailnet, offer, credential, or custody
material.

| Lane | Receipt | Status |
| --- | --- | --- |
| Hermetic headless Portal | `target/portal-headless-e2e/evidence.json` | local receipt only |
| Disposable iOS Simulator Portal | `target/ios-portal-exact-sequence-simulator/evidence.json` | local receipt only |
| Physical Android Tailnet Portal | `target/android-portal-tailnet-physical/evidence.json` | unchecked: no approved physical Android device attached |

The pre-existing `oxid-standalone` Compose project is not owned by this record.
Only harness-created, receipt-matched Portal and Simulator resources may be
removed. Virtual evidence is diagnostic and loopback/pinned-development only;
it is not physical-device, Tailnet, production-trust, live-KYC, or
native-custody evidence.

Next physical-device checkpoint, with one approved non-QEMU Android device and
validated Tailscale Serve baseline: `just android-portal-tailnet-physical-smoke`.
