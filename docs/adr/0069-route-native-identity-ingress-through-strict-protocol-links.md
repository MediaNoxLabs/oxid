# ADR-0069: Route native identity ingress through strict protocol links

- Status: Accepted
- Date: 2026-08-17
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/qr_scanner.rs`, `mobile-bench/dioxus-wallet/src/identity_centre.rs`, and the Android/iOS QR bridges
- Tracking: issues #2 and #32
- Implementation state: strict standalone request routing, native iOS/Android QR adapters, bounded timeout closure, explicit iOS camera-denial status, Dioxus handoff, headless conformance, native packaging, and simulator/emulator fail-closed evidence are implemented; physical Android success, cancellation, timeout, post-return liveness, and consent isolation are proven on Samsung SM-S928B / Android 16 (API 36); ADR-0070 adds custom-scheme OS link delivery through the same router, while physical iOS and verified universal/app-link evidence remain #32

## Context

The prototype makes camera scanning a primary entry point for credential
issuance and identity interactions. Oxid already implements consented
standalone OpenID4VCI issuance, SIOPv2 authentication, and OpenID4VP credential
presentation, but those flows previously started only from deterministic text
fixtures in the Dioxus pages.

A camera is an operating-system capability, while deciding what a scanned link
means is protocol-edge behavior. Putting both in Dioxus would couple the
incoming adapter to AVFoundation, Google Play services, and URI parsing. It
would also make it easy to route an untrusted QR payload by string prefix or to
leak an offer, nonce, state, or request endpoint through debug output.

SIOPv2 and the current presentation fixture both use the `openid4vp` scheme.
The scheme alone therefore cannot distinguish authentication from credential
presentation. Production endpoint discovery is not yet implemented.

## Decision

Add a capability-specific `QrScannerPort` to `platform-ports`. A successful
scan returns an opaque, 32 KiB-bounded value whose `Debug` output reports only
its length. Stable failures distinguish cancellation, camera denial,
unavailability, timeout, invalid payload, and generic failure without carrying
native error bodies. Denial is an iOS camera-permission outcome; Android's
Google Code Scanner owns camera access outside the app and reports its
permission/module failures as unavailable.

Implement the port in `adapters/identity-ingress`:

- iOS uses a statically packaged Manganis Swift bridge and AVFoundation. It
  requests camera permission only when scanning starts, accepts QR metadata
  only, and reports unavailable in the Apple simulator.
- Android uses a statically packaged Manganis Kotlin bridge and Google Code
  Scanner 16.1.0 in QR-only mode. The scanner is Play-services-backed, requires
  no app camera permission, and may be unavailable where its module cannot run.
- Desktop, web, and uncomposed targets use a fail-closed unavailable adapter.

The Rust adapter owns a 60-second scan budget. When it expires, the native
coordinator must acknowledge `timed_out` and invalidate the exact active scan
generation before Rust publishes the terminal result. iOS also stops and
dismisses its repository-owned scanner. Google Code Scanner does not expose a
programmatic dismissal operation: Android closes Oxid's logical handoff and
discards its eventual generation-stale callback, while the holder may still
need to dismiss the system-owned scanner UI. A late native callback can never
complete a subsequent scan.

Google Code Scanner 16.1.0 on the reviewed Samsung/API 36 host returns
`MlKitException.INTERNAL` when Back closes an already-presented scanner rather
than the documented scanner-cancelled code. Oxid normalizes that exact result
to cancellation only when its owning activity observed a foreground loss while
the same generation was scanning. An internal failure before presentation, a
stale callback, or an inactive generation remains fail-closed. The exception
message and QR value are never logged or returned.

Keep request classification in a separate `IdentityRequestRouterPort` owned by
`protocol/application`. `StrictIdentityRequestRouter` parses the complete URI,
rejects user-info, fragments, ports, unknown or duplicate query fields, empty
values, control characters, whitespace padding, and payloads over 32 KiB. It
accepts exactly one `credential_offer` or `credential_offer_uri` for
`openid-credential-offer`.

For `openid4vp`, standalone composition registers the exact expected
`client_id` and `request_uri` pairs for its SIOPv2 and presentation fixtures.
An exact match selects the existing flow. Unknown pairs are `ambiguous` and
fail closed; the wallet must not guess from scheme, port, host prefix, or page
state. Production composition remains credential-offer-only until reviewed
endpoint discovery supplies an authenticated registry.

Android serializes an empty-authority credential-offer URI with a `/` path
when it delivers the intent. The shared router accepts only the equivalent
empty and `/` path forms; non-root paths, hosts, and every existing query-field
restriction still fail closed. Native code forwards the bounded OS value
unchanged and does not classify or normalize it.

Dioxus exposes one Scan QR action and passes only the classified request to the
existing issuance, authentication, or presentation page. Scanning does not
preview, consent to, or execute a protocol operation. The existing page-level
preview and explicit-consent gates remain mandatory. The temporary raw request
stays in UI state only and is not logged.

Expose the same classifier as headless method `identity.request.route`. Its
response contains only the route kind and UI destination, never the raw or
nested URI. This supplies deterministic coverage for all three routes and the
ambiguous failure without requiring camera hardware.

## Security and truth boundaries

- Treat every QR/deep-link string as hostile protocol input. Native code only
  captures bytes; shared Rust validates and classifies them.
- Native exception text, payloads, offer codes, nonces, state, request objects,
  and endpoints must not enter logs, debug output, or headless responses.
- A simulator result proves packaging and the unavailable path, not camera
  success. Repository-owned physical evidence proves Android success,
  cancellation, timeout, stale-result isolation, post-return controls, and
  consent isolation on Samsung SM-S928B / Android 16 (API 36). Android denial
  is not an app-owned state because Google Code Scanner is permissionless;
  module/vendor unavailability remains a fail-closed fixture rather than a
  device setting manufactured for a test. Physical iOS permission and camera
  evidence remains issue #32.
- Google Code Scanner is a replaceable Android edge dependency, not a wallet or
  identity core dependency. Manual fixtures and headless routing remain
  available where Play services are absent.
- ADR-0070 routes OS deep/app links through this same bounded router; they may
  not create a second, looser classification path.
- Verified HTTPS delivery cannot be declared from application code alone.
  Universal links require an approved domain and path, an iOS associated-domain
  entitlement plus a matching hosted AASA document, and signed-device evidence.
  Android App Links require that same approved URL policy, an `autoVerify`
  manifest filter, and a hosted `assetlinks.json` containing the release signing
  identity. Until those external inputs and the HTTPS-to-protocol mapping are
  reviewed, Oxid must not invent a domain, accept arbitrary `https`, or broaden
  the strict router. Custom schemes remain the only implemented OS-link route.

## Consequences

- The mobile shell now has scan-first access to all implemented standalone
  identity flows while retaining their existing preview and consent controls.
- Native platform code stays statically packaged at the edge and cannot import
  application services, custody, credential storage, or protocol adapters.
- Headless tests can prove routing and redaction independently of camera
  hardware.
- Arbitrary external OpenID4VP links remain unavailable until production
  discovery can classify the shared scheme without ambiguity.

## Rejected alternatives

- Prefix matching is vulnerable to malformed or smuggled query parameters.
- Routing every `openid4vp` link to authentication or presentation would make
  the other protocol unreachable and could show the wrong consent surface.
- A WebView JavaScript camera bridge would expand the secret-bearing browser
  boundary retained by the prototype.
- Owning the Android camera directly would add permission, lifecycle, and image
  processing surface without improving the current QR-only use case.
- Fetching the nested request URI during classification would combine ingress,
  transport trust, and protocol execution before the user sees a preview.
