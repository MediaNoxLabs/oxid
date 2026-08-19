# Oxid mobile native plugin

This repository-owned Manganis plugin is the single native bridge shared by
Oxid's driven mobile adapters. It owns only OS integration: QR camera capture,
custom-scheme delivery, typed public receive-address copy/share actions, device
custody/document pickers, and a boolean screen-snapshot privacy operation.
Protocol classification, presentation masking, consent, and wallet behavior
remain in Rust ports and application services.

Keeping one plugin package is required by Dioxus 0.7.10, whose iOS bundler
compiles multiple Swift packages but embeds only its primary framework.

## QR lifecycle contract

Native code captures one QR value and returns only this closed JSON status
vocabulary to Rust: `scanning`, `succeeded`, `cancelled`, `denied` (iOS only),
`unavailable`, `timed_out`, `invalid`, or `failed`. Only `succeeded` carries a
non-empty UTF-8 payload, bounded to 32 KiB before it crosses the bridge. Error
objects, permission details, request values, and platform exception text never
cross it.

Rust owns the 60-second deadline and asks the native coordinator to close the
exact active generation before publishing `timed_out`. iOS stops and dismisses
its repository-owned scanner. Google Code Scanner has no programmatic dismiss
API, so Android invalidates the logical handoff and every eventual stale task
callback; the holder may still need to dismiss the system-owned scanner UI.
This limitation must remain visible in physical-device evidence.

| Outcome | iOS native source | Android native source | Local evidence |
| --- | --- | --- | --- |
| Success | bounded AVFoundation QR metadata | bounded Google Code Scanner QR result | Rust closed-status and bound tests; physical capture open |
| Cancel | repository Cancel control | Code Scanner cancelled task | Rust closed-status test; physical gesture open |
| Denial | AVFoundation permission denied/restricted | not an app-owned permission outcome | Rust distinct-denial test; physical iOS denial open |
| Timeout | Rust deadline acknowledged; scanner stopped/dismissed | Rust deadline acknowledged; late task invalidated | Rust closed-status test; focused virtual/physical lifecycle evidence open |
| Unavailable | simulator or missing capture device | failed Play Services preflight/module | existing iOS simulator and focused adapter evidence |

Focused virtual-device harnesses are:

```bash
nix develop --command ./scripts/test-ios-identity-ingress.sh
nix develop --command ./scripts/test-android-identity-ingress.sh
```

They reset only Oxid's test application data on the selected simulator/emulator.
The Android harness refuses physical devices. A virtual-device unavailable,
cancel, or timeout result is lifecycle/packaging evidence only, never proof of
physical camera success or denial behavior.

## Verified HTTPS-link prerequisites

The plugin deliberately packages only the reviewed custom schemes today.
Universal links and Android App Links require external trust configuration that
does not exist in the repository yet:

- an approved HTTPS domain and bounded identity-request path policy;
- a matching hosted Apple `apple-app-site-association` document and iOS
  associated-domain entitlement;
- a matching hosted Android `.well-known/assetlinks.json`, release signing
  certificate fingerprints, and an `android:autoVerify="true"` intent filter;
- an ADR-reviewed HTTPS-to-protocol mapping that still passes through the strict
  Rust request router; and
- signed physical-device cold/warm delivery evidence.

Do not add placeholder domains, broad `https` capture, debug signing
fingerprints, or a second Swift/Kotlin classifier. Platform requirements are
documented by
[Apple](https://developer.apple.com/documentation/xcode/supporting-associated-domains)
and
[Android](https://developer.android.com/training/app-links/add-applinks).

## Physical-device QR evidence

Use a non-production standalone-development installation with no real funds or
credentials. Encode only the public deterministic standalone offer/request
fixtures already used by repository tests.

1. On iOS, start from camera permission `notDetermined`. Scan one valid fixture
   and confirm Oxid opens only the matching review page. Dismiss it without
   consent. Repeat and tap the native Cancel control. Reset camera privacy,
   deny the next request, and confirm denial imports nothing. Leave a final scan
   open for more than 60 seconds and confirm the timeout result, scanner
   dismissal, and successful fresh retry.
2. On Android with current Google Play Services, scan one valid fixture and
   confirm review without consent. Repeat and dismiss Code Scanner. Leave a scan
   open beyond 60 seconds, return to Oxid if the system scanner remains visible,
   confirm timeout imported nothing, and retry successfully. Repeat on a device
   without the Code Scanner module to prove the unavailable path.
3. For every case, inspect application logs only for absence of the request
   value; never enable payload logging to obtain evidence. Record OS/device,
   app commit, outcome, and whether any request reached preview. Camera success,
   denial, thermal behavior, and resource use remain physical evidence and must
   not be inferred from simulator/emulator results.
