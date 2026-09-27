# Maestro mobile pilot (local only)

This is an opt-in black-box pilot for simulated debug builds. It complements and does not replace the existing Android CDP and iOS XCTest suites. It is not GitHub CI.

## Reproducible invocation

Maestro is supplied by the pinned `nixpkgs` input as `.#maestro` (currently 2.8.0); no global installation or download is used. Build/install the simulated app through the repository launcher, then run one platform at a time:

```sh
OXID_IOS_DEVICE=<receipt-owned-udid> ./scripts/run-maestro-ios.sh
OXID_ANDROID_DEVICE=emulator-<port> ./scripts/run-maestro-android.sh
```

The iOS route is the first runtime attempt. The Android wrapper refuses anything except an `emulator-*` serial and only deploys through the repository launcher. Never use a physical phone.

## Flow and evidence contract

`tests/maestro/{ios,android}-lunar-aegis.yaml` capture the simulated first-run fork and the safe device-protection explanation, then relaunch cleanly before native authorization. The compile-time demo drawer creates only a profile and process-local protection; it does not run the account-derivation or funding fixture actions. The simulated profile nevertheless exposes a synchronized protected account by design, so both flows assert Receive/open-close and stop on the `SEND NIGHT` entry form without entering a recipient, amount, or transfer action; they also cover Documents and Activity, while both platforms reach Settings. Android deliberately applies `FLAG_SECURE` after demo protection initializes. Its only post-protection screenshot is the public simulated Home route after the user-visible Session privacy action; the flow immediately re-masks before Settings, while CDP remains authoritative for the 30-second timeout and lifecycle re-arm. Settings, Documents, credential review, backup/recovery, and other secret-bearing routes stay protected. The simulated demo custody adapter intentionally has no Backup capability; existing native-custody XCTest/CDP suites own that screen. Maestro can produce automatic failure screenshots, so recovery-phrase content is neither entered nor asserted by this flow. Both normal and debug output are routed beneath the ignored per-device artifact root.

Android sets `androidWebViewHierarchy: devtools`. If Maestro cannot inspect the WebView despite the existing debuggable WebView/CDP seam, record that as the pilot blocker; do not weaken CDP coverage or add CI.

## Pilot recommendation

Keep this local while measuring one clean iOS then emulator run. Capture Maestro's output directory as a short-lived local artifact, redact before sharing, and record runtime/flake rate after at least ten clean runs. A practical future CI decision needs bounded runtime, <5% repeat-run flakes, retained/redacted artifacts, and a proven WebView hierarchy; none is established by this pilot.
