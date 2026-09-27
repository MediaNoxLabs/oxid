# Mobile visual accessibility evidence (local only)

This maintained matrix defines the bounded, privacy-safe evidence tranche for the simulated first-run journey and holder shell. It is additive to Android CDP and iOS XCTest coverage; it does not replace either suite or authorize product changes. Run the iOS Simulator and Android Emulator serially with receipt-owned runtimes. Generated screenshots and logs belong only under ignored `target/mobile-visual-accessibility/<platform>/`; never capture a recovery phrase or another private value.

## Capture matrix

| State | Lunar Aegis screen ID | Platforms | Artifact | Accessibility checks | Known gaps |
| --- | --- | --- | --- | --- | --- |
| Welcome and create-vs-restore fork | `XSwTg6CjwXruX8QP3tXy` | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-01-first-run` | 375 pt/dp and one larger width; safe-area/navigation non-overlap; 44 px touch targets; large-text truncation; non-color status meaning; screen-reader labels/order | Native focus-order detail remains platform-harness dependent. |
| Mandatory device-protection explanation | `FFMmLvVQlc5xIun63FYX` | iOS Simulator, Android Emulator | privacy-safe pre-ceremony capture only | deterministic Back; reduced motion; screen-reader labels/order | Do not capture the phrase ceremony or its private values. |
| Recovery boundary and Ready/Home | `xYA9BiozNUetlxPJYHPT`, `7u81lbjNIKcn8dS79axb` | XCTest/CDP for recovery; iOS Simulator for Home; Android semantic assertion | iOS: `lunar-aegis-ios-02-home-empty` | modal focus return; safe-area/navigation non-overlap; large-text truncation | Maestro must not enter the recovery ceremony because its automatic failure artifacts could retain a phrase. Android applies `FLAG_SECURE` after demo protection initializes. |
| Receive and blocked Send | holder shell | iOS Simulator visual; Android Emulator semantic | iOS: `lunar-aegis-ios-03-receive`, `lunar-aegis-ios-04-send-prerequisite` | 44 px touch targets; deterministic Back; non-color status meaning | Android transitions pass semantically, but protected screenshots are intentionally black. |
| Empty Documents and Activity | holder shell | iOS Simulator visual; Android Emulator semantic | iOS: `lunar-aegis-ios-05-documents-empty`, `lunar-aegis-ios-06-activity-empty` | safe-area/navigation non-overlap; screen-reader labels/order | Android Documents forces `FLAG_SECURE`; Maestro retains semantic assertions only. Empty-state wording is product-owned. |
| Settings and native-custody Backup boundary | holder shell | iOS Simulator; Android follow-up #813; native XCTest/CDP for Backup | iOS: `lunar-aegis-ios-07-settings` | modal focus return; reduced motion; large-text truncation | Android's DevTools hierarchy omits the header menu after bootstrap, so Settings is unreachable in this black-box flow. The simulated demo custody adapter intentionally has no Backup capability. |

## Operating rules

- Maestro flows stay local-only and additive; preserve existing CDP and XCTest coverage.
- Maestro must never enter the recovery ceremony. Native XCTest/CDP own that
  behavior without publishing visual artifacts; Maestro uses the compile-time
  simulated demo drawer to reach the holder shell.
- The demo adapter intentionally omits native-custody Backup. Maestro captures
  Settings and leaves Backup behavior to the existing native XCTest/CDP suites.
- Android applies `FLAG_SECURE` after demo protection initializes, so only the
  pre-protection first-run screenshot is retained. Its DevTools hierarchy still
  proves Home, Receive, blocked Send, Documents, and Activity transitions. The
  missing header-menu semantics and explicit-reveal route are tracked by #813;
  this slice does not weaken native privacy to manufacture screenshots. iOS has
  no foreground screenshot-blocking API.
- Capture at 375 pt/dp-class width and one larger width for every matrix state where the platform harness exposes the size control.
- If a visual or accessibility defect is observed, file a linked owning-screen follow-up instead of changing product code in this evidence slice.
- A WebView hierarchy or platform-harness limitation is evidence, not permission to weaken semantic assertions or capture sensitive data.
