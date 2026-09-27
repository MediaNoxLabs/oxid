# Mobile visual accessibility evidence (local only)

This maintained matrix defines the bounded, privacy-safe evidence tranche for the simulated first-run journey and holder shell. It is additive to Android CDP and iOS XCTest coverage; it does not replace either suite or authorize product changes. Run the iOS Simulator and Android Emulator serially with receipt-owned runtimes. Generated screenshots and logs belong only under ignored `target/mobile-visual-accessibility/<platform>/`; never capture a recovery phrase or another private value.

## Capture matrix

| State | Lunar Aegis screen ID | Platforms | Artifact | Accessibility checks | Known gaps |
| --- | --- | --- | --- | --- | --- |
| Welcome and create-vs-restore fork | `XSwTg6CjwXruX8QP3tXy` | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-01-first-run` | 375 pt/dp and one larger width; safe-area/navigation non-overlap; 44 px touch targets; large-text truncation; non-color status meaning; screen-reader labels/order | Native focus-order detail remains platform-harness dependent. |
| Mandatory device-protection explanation | `FFMmLvVQlc5xIun63FYX` | iOS Simulator visual; Android Emulator semantic | iOS: `lunar-aegis-ios-02-device-protection` | reduced motion; screen-reader labels/order | Stop and relaunch cleanly before native authorization; do not capture the phrase ceremony or its private values. Android already protects this screen, so no Android screenshot is retained. |
| Recovery boundary and Ready/Home | `xYA9BiozNUetlxPJYHPT`, `7u81lbjNIKcn8dS79axb` | XCTest/CDP for recovery; iOS Simulator for Home; Android public simulated Home after explicit reveal | iOS: `lunar-aegis-ios-03-home-empty`; Android: `lunar-aegis-android-03-home-public-revealed` | modal focus return; safe-area/navigation non-overlap; large-text truncation | Maestro must not enter the recovery ceremony because its automatic failure artifacts could retain a phrase. Android captures Home only through the explicit Session privacy reveal and re-masks immediately afterward. |
| Receive and Send entry | holder shell | iOS Simulator visual; Android Emulator semantic | iOS: `lunar-aegis-ios-04-receive`, `lunar-aegis-ios-05-send-entry` | 44 px touch targets; deterministic Back via `Go back` to Home; non-color status meaning | The simulated demo profile is already a synchronized protected account, so the safe boundary is the `SEND NIGHT` entry form. The flow never enters a recipient, amount, or transfer action. Android transitions pass semantically, but protected screenshots are intentionally black. |
| Empty Documents and fixture Activity | holder shell | iOS Simulator visual; Android Emulator semantic | iOS: `lunar-aegis-ios-06-documents-empty`, `lunar-aegis-ios-07-activity-history` | safe-area/navigation non-overlap; screen-reader labels/order | The simulated profile intentionally exposes `Sent` and `Received` fixture history. Android Documents forces `FLAG_SECURE`; Maestro retains semantic assertions only. |
| Settings and native-custody Backup boundary | holder shell | iOS Simulator; Android Emulator; native XCTest/CDP for Backup | iOS: `lunar-aegis-ios-08-settings` | modal focus return; reduced motion; large-text truncation; Android semantic header and Settings traversal | Android reaches Settings semantically while `FLAG_SECURE` remains active. The simulated demo custody adapter intentionally has no Backup capability. |

## Operating rules

- Maestro flows stay local-only and additive; preserve existing CDP and XCTest coverage.
- Maestro must never enter the recovery ceremony. Native XCTest/CDP own that
  behavior without publishing visual artifacts; Maestro uses the compile-time
  simulated demo drawer to reach the holder shell.
- The demo adapter intentionally omits native-custody Backup. Maestro captures
  Settings and leaves Backup behavior to the existing native XCTest/CDP suites.
- Both Maestro output and debug logs stay below the ignored per-device artifact
  root; a global Maestro state directory is never an accepted evidence path.
- Android applies `FLAG_SECURE` after demo protection initializes. Its only
  post-protection capture is the public simulated Home route after the explicit
  Session privacy reveal; the flow re-masks immediately before entering Settings.
  Settings, Documents, credential review, backup/recovery, and other secret-bearing
  routes remain protected. CDP remains authoritative for the 30-second timeout and
  lifecycle re-arm behavior. iOS has no foreground screenshot-blocking API.
- Capture at 375 pt/dp-class width and one larger width for every matrix state where the platform harness exposes the size control.
- If a visual or accessibility defect is observed, file a linked owning-screen follow-up instead of changing product code in this evidence slice.
- A WebView hierarchy or platform-harness limitation is evidence, not permission to weaken semantic assertions or capture sensitive data.
