# ADR-0094: Protect mobile snapshots through a boolean platform port

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 12–13, 16–18, and 21
- Design source: `docs/design/ui-profiles.md` P6, rollout Phase 4a
- Tracking: issues #2, #32, #65, and #85
- Amends: ADR-0004, ADR-0006, ADR-0029, and ADR-0070
- Implementation state: the existing repository-owned mobile plugin applies Android `FLAG_SECURE` or an iOS scene-background privacy overlay from one payload-free boolean port; physical Samsung/API 36 lifecycle inspection proves Android re-arm using the numeric platform flag mask; desktop/web remain unavailable

## Context

Render-only masking protects values inside the WebView but does not define how
the host operating system captures the application window. Android can prevent
screenshots, recording, and recent-task previews with `FLAG_SECURE`. iOS does
not expose an equivalent screenshot-blocking API; it can only cover the window
when the scene enters the background so the app switcher receives an opaque
preview.

The native operation needs no wallet value and must not become another generic
JavaScript/native command channel.

## Decision

Add `ScreenPrivacyPort::set_protected(bool)` to the platform boundary. The
request carries exactly one policy bit and returns only `Unavailable` or
`Failed`. It cannot receive balances, addresses, DIDs, credentials, claims,
protocol requests, routes, or arbitrary native command names.

`oxid-adapter-platform-system` maps that port to the existing single Manganis
mobile plugin. On Android the plugin performs the change on the UI thread and
sets or clears `WindowManager.LayoutParams.FLAG_SECURE`. On iOS it registers
scene background/foreground notifications. When enabled, backgrounding adds an
opaque black overlay to every application window; foregrounding removes it.
Disabling the policy also removes any retained overlay. User-facing copy and
documentation state that iOS does not block screenshots.

The composition root provides the native adapter only on iOS and Android and a
fail-closed unavailable adapter elsewhere. Dioxus requests protection whenever
secret mode is masked. It also forces protection on Settings and credential
routes so backup-secret entry and locally revealed credential claims remain
protected even during an explicit global reveal. Fresh onboarding recovery is
already in the masked resting state.

Native failures do not unmask the Dioxus presentation and are not logged with
payloads. They leave the OS capability unavailable while the render-only
protection remains active. This is privacy hardening, not wallet authority, and
does not participate in any transaction or consent decision.

## Consequences

- Android screenshots and recording are intentionally unavailable while the
  policy is active; users must explicitly reveal globally before the flag is
  cleared on ordinary routes.
- iOS app-switcher previews are opaque while protected, but foreground
  screenshots remain possible and must never be described as blocked.
- Backup and credential-reveal routes prefer protection even when ordinary
  balances have been explicitly revealed.
- The plugin stays capability-specific and receives no arbitrary text or
  secret-bearing payload.
- Physical Samsung SM-S928B / Android 16 (API 36) host inspection proves the
  secure-window bit is cleared only by explicit reveal and restored after
  background/resume. Screenshot/recording behavior on additional vendors and
  physical iOS multi-scene evidence remain part of issue #32.

## Validation

- Platform-port and system-adapter tests prove unsupported targets fail with
  closed payload-free categories.
- Rust cross-target builds compile the typed JNI and Swift bridges; the Android
  JNI exception-recovery smoke continues to exercise the bridge afterwards.
- Android and iOS standalone smokes verify the presentation toggle and
  lifecycle re-arm. Android host inspection reads
  `WindowManager.LayoutParams.FLAG_SECURE` as bit `0x2000`; this remains valid
  on hosts whose `dumpsys window` output omits the symbolic `SECURE` label. iOS
  UI coverage verifies foreground recovery after the privacy-overlay lifecycle.
- Strict architecture gates continue to forbid direct native dependencies in
  domain/application crates.

## Rejected alternatives

- A stringly `nativeCommand(name, payload)` bridge would create a general
  secret-export and authority surface.
- Treating iOS as screenshot-blocking would make a false platform claim.
- Applying native privacy only on app background would leave Android screen
  recording and foreground screenshots exposed.
- Making native success a prerequisite for wallet use would conflate a
  privacy hardening capability with custody and transaction correctness.
