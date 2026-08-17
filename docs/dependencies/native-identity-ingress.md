# Native mobile bridge dependency review

## Manganis 0.7.10

- Project: [Manganis](https://github.com/DioxusLabs/dioxus/tree/main/packages/manganis/manganis)
- Selected version: 0.7.10, matched to Dioxus 0.7.10 and pinned by the workspace
  plus `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: maintained in the active Dioxus monorepo
- Security/audit evidence: no independent Oxid audit; RustSec, source, license,
  locked-build, and native-package gates are mandatory
- Target support: the selected FFI macro packages static Swift and Kotlin
  source for iOS and Android; it is not used by core crates
- Cryptography: none
- API stability: pre-1.0 and tied to the Dioxus release family; upgrades require
  rebuilding both native targets and exercising the bridge contract
- Reason selected: it is the Dioxus-native static FFI packaging path and avoids
  a WebView JavaScript bridge or a manually maintained generated host project
- Alternatives considered: direct generated-project edits, a generic C ABI,
  WebView JavaScript, and separate Swift/Kotlin applications
- Adapter boundary: `crates/adapters/mobile-native-plugin` owns the single
  native package; `identity-ingress` and `platform-system` expose only focused
  Rust ports, payload-free failures, and successful bounded values
- Exit strategy: replace the adapter or bridge implementation while retaining
  `QrScannerPort`, `IdentityLinkIngressPort`, `PublicTextExportPort`, and the
  Rust protocol router

The selected Dioxus iOS bundler compiles multiple discovered Swift packages
but embeds only its primary framework. Oxid therefore keeps one reviewed mobile
plugin package for QR, Android link queueing, and public-address copy/share.
Splitting that package requires a Dioxus upgrade plus native packaging evidence.

## Google Code Scanner 16.1.0

- Project: [Google Code Scanner](https://developers.google.com/ml-kit/vision/barcode-scanning/code-scanner)
- Selected version: `com.google.android.gms:play-services-code-scanner:16.1.0`,
  pinned in the adapter Gradle build
- License: Google Play services SDK terms; review those terms before
  redistribution or a store release
- Maintenance/activity: maintained as an ML Kit/Google Play services Android
  API; version changes require explicit review rather than a dynamic range
- Security/audit evidence: no independent Oxid audit; Android packaging and
  device tests are required, and raw Google failure details are discarded.
  Google's integration guide states that image processing is on-device and
  scan results/images are not stored by Google
- Target support: Android API 23+, Google Play-services-backed devices; the app
  does not request Android camera permission for this scanner. The scanner is
  an unbundled module requested for install through the adapter manifest and
  can still require a first-use download
- Cryptography: none; QR decoding provides no authenticity or integrity
- API stability: vendor API and runtime module availability may change outside
  the app; unavailable and cancelled results remain normal typed outcomes
- Reason selected: the prototype used the same system-mediated scanner model;
  it provides QR-only capture with a small permission/lifecycle surface
- Alternatives considered: CameraX plus ML Kit barcode scanning, ZXing, and a
  hand-written camera host
- Adapter boundary: the Kotlin plugin receives an `Activity`, returns bounded
  status JSON, selects only `FORMAT_QR_CODE`, and never logs the payload
- Exit strategy: substitute another Android implementation of `QrScannerPort`;
  no Google type crosses into Rust application or domain crates

## Apple AVFoundation

- Project: Apple system framework, linked from the iOS static plugin
- Selected version: the iOS 15+ platform SDK supplied by Xcode
- License/maintenance: governed and maintained as part of Apple's platform SDK
- Security/audit evidence: no independent Oxid audit; permission-denial,
  cancellation, physical-camera, and native package tests are required
- Target support: iOS physical devices; the simulator truthfully reports
  unavailable
- Cryptography: none
- Adapter boundary and exit: confined to the Swift implementation of
  `QrScannerPort`; it can be replaced without changing protocol/application
  code

## Native public text export

- Project: Apple UIKit and Android platform APIs supplied by the selected SDKs
- Selected surface: `UIPasteboard`, `UIActivityViewController`,
  `ClipboardManager`, and `ACTION_SEND`
- License/maintenance: governed and maintained as part of the Apple and Android
  platform SDKs; no additional Cargo or mobile runtime dependency is added
- Security/audit evidence: simulator/emulator UI tests assert clipboard success
  and native activity/chooser presentation; no independent Oxid audit
- Target support: iOS 15+ and Android API 23+ through the existing mobile host
- Cryptography: none
- API stability: platform APIs, isolated behind a replaceable Rust port
- Reason selected: system surfaces provide expected mobile behavior without a
  WebView JavaScript bridge
- Alternatives considered: generic JavaScript clipboard/share APIs, a generic
  string-export port, or leaving the prototype's no-op clipboard behavior
- Adapter boundary: only `PublicReceiveAddress` may cross
  `PublicTextExportPort`; protocol URIs, credentials, proofs, and secrets have no
  export method
- Exit strategy: replace the native implementation without widening the port
