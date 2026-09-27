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

`tests/maestro/{ios,android}-lunar-aegis.yaml` use the simulated first-run profile when it is present, then assert Home, Receive/open-close, Send's blocked prerequisite, Documents, Activity, and Settings/Backup. Screenshots have stable `lunar-aegis-<platform>-NN-<state>` names and must depict only simulated or empty states. Recovery-phrase content is not asserted or captured.

Android sets `androidWebViewHierarchy: devtools`. If Maestro cannot inspect the WebView despite the existing debuggable WebView/CDP seam, record that as the pilot blocker; do not weaken CDP coverage or add CI.

## Pilot recommendation

Keep this local while measuring one clean iOS then emulator run. Capture Maestro's output directory as a short-lived local artifact, redact before sharing, and record runtime/flake rate after at least ten clean runs. A practical future CI decision needs bounded runtime, <5% repeat-run flakes, retained/redacted artifacts, and a proven WebView hierarchy; none is established by this pilot.
