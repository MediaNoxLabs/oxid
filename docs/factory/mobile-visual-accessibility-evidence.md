# Mobile visual accessibility evidence (local only)

This maintained matrix defines the bounded, privacy-safe evidence tranche for the simulated first-run journey and holder shell. It is additive to Android CDP and iOS XCTest coverage; it does not replace either suite or authorize product changes. Run the iOS Simulator and Android Emulator serially with receipt-owned runtimes. Generated screenshots and logs belong only under ignored `target/mobile-visual-accessibility/<platform>/`; never capture a recovery phrase or another private value.

## Capture matrix

| State | Lunar Aegis screen ID | Platforms | Artifact | Accessibility checks | Known gaps |
| --- | --- | --- | --- | --- | --- |
| Welcome and create-vs-restore fork | `XSwTg6CjwXruX8QP3tXy` | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-01-first-run` | 375 pt/dp and one larger width; safe-area/navigation non-overlap; 44 px touch targets; large-text truncation; non-color status meaning; screen-reader labels/order | Native focus-order detail remains platform-harness dependent. |
| Mandatory device-protection explanation | `FFMmLvVQlc5xIun63FYX` | iOS Simulator, Android Emulator | privacy-safe pre-ceremony capture only | deterministic Back; reduced motion; screen-reader labels/order | Do not capture the phrase ceremony or its private values. |
| Recovery review and Ready/Home | `xYA9BiozNUetlxPJYHPT`, `7u81lbjNIKcn8dS79axb` | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-02-home-empty` | modal focus return; safe-area/navigation non-overlap; large-text truncation | The recovery review itself is asserted without a screenshot. |
| Receive and blocked Send | holder shell | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-03-receive`, `lunar-aegis-<platform>-04-send-prerequisite` | 44 px touch targets; deterministic Back; non-color status meaning | Assistive technology traversal requires a supported platform harness. |
| Empty Documents and Activity | holder shell | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-05-documents-empty`, `lunar-aegis-<platform>-06-activity-empty` | safe-area/navigation non-overlap; screen-reader labels/order | Empty-state wording is product-owned. |
| Settings/Backup | holder shell | iOS Simulator, Android Emulator | `lunar-aegis-<platform>-07-settings-backup` | modal focus return; reduced motion; large-text truncation | Backup export and recovery values are out of scope. |

## Operating rules

- Maestro flows stay local-only and additive; preserve existing CDP and XCTest coverage.
- Capture at 375 pt/dp-class width and one larger width for every matrix state where the platform harness exposes the size control.
- If a visual or accessibility defect is observed, file a linked owning-screen follow-up instead of changing product code in this evidence slice.
- A WebView hierarchy or platform-harness limitation is evidence, not permission to weaken semantic assertions or capture sensitive data.
