# ADR-0070: Constrain mobile links and public text export

- Status: Accepted
- Date: 2026-08-17
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, whose mobile clipboard adapter is a no-op and whose identity ingress is QR-only
- Tracking: issues #2 and #32
- Implementation state: iOS/Android custom-scheme delivery, strict shared routing, explicit dismissal, typed public-address copy/share, consolidated native packaging, headless routing conformance, and simulator/emulator lifecycle evidence are implemented; physical Android warm/foreground and cold custom-scheme delivery are proven on Samsung SM-S928B / Android 16 (API 36); universal HTTPS links, production endpoint discovery, physical iOS evidence, and device resource baselines remain #32
- Amended by: ADR-0081, ADR-0094

## Context

ADR-0069 made camera capture an opaque incoming capability and required every
identity request to pass through the strict Rust router before reaching an
existing preview and consent surface. Mobile operating systems can also launch
or resume the wallet with `openid-credential-offer` and `openid4vp` links. A
second parser in Swift, Kotlin, or Dioxus would weaken that boundary and could
lose a cold-start request before the component tree exists.

The receive page also needs normal mobile copy and share behavior. A generic
clipboard or share port would allow a future caller to export a seed, private
key, credential, proof, token, or secret-bearing protocol URI through a public
API. The prototype does not supply a safe implementation: its mobile clipboard
method explicitly does nothing.

Dioxus 0.7.10 discovers multiple Manganis Swift packages and compiles each one,
but its iOS bundler embeds only the primary framework. Oxid therefore cannot
rely on one native package per Rust adapter at this version.

## Decision

Add `IdentityLinkIngressPort` to `platform-ports`. It admits one opaque,
32-KiB-bounded, control-free link at a time and exposes payload-free errors.
The single pending slot prevents a new OS event from replacing a request the
holder is already reviewing.

Register only the two protocol schemes in `apps/oxid/Dioxus.toml`:

- iOS captures Tao's `Event::Opened` in the application launch configuration,
  before the Dioxus component tree is constructed, so terminated-state links
  cannot race component registration.
- Android uses the repository-owned `MainActivity` with `singleTop`; `onCreate`
  captures cold links and `onNewIntent` captures warm links. Only `ACTION_VIEW`
  intents with an exact registered scheme enter the bounded native queue. Wry
  does not translate a foreground Android `onNewIntent` into Tao `Opened`, so
  the rendered Android component polls only that one-item native handoff every
  250 ms. The hook is paused when the component is not rendered; it never logs
  or carries the link itself.

Native capture never classifies or executes a request. Dioxus drains a pending
link only after a profile is active and sends it through the same
`IdentityRequestRouterPort` used by QR and headless
`identity.request.route`. A successful classification selects the existing
issuance, authentication, or presentation page and announces that review is
required. The request remains pending until explicit dismissal; preview and
consent remain separate actions.

Add `PublicReceiveAddress` and `PublicTextExportPort` to `platform-ports`. The
type is bounded to 4 KiB, rejects padding and control characters, and redacts
its `Debug` representation. The port has exactly two methods:
`copy_receive_address` and `share_receive_address`. It has no arbitrary-string,
protocol-request, credential, or secret export method. Dioxus constructs the
type only from an already rendered public receive address.

Implement native copy and share with `UIPasteboard`/
`UIActivityViewController` on iOS and `ClipboardManager`/`ACTION_SEND` on
Android. Desktop, web, and production compositions without the adapter fail
closed as unavailable.

Package QR, Android link queueing, clipboard, and share code in the single
`oxid-adapter-mobile-native` driven-adapter crate. It owns one Swift package and
one Kotlin/Gradle module so Dioxus embeds a single native plugin. Rust consumers
see payload-free bridge failures and capability-specific ports. On Android,
Rust invokes public methods on the repository-owned activity instance; this
uses the application class loader and avoids `FindClass` failure on a Rust
worker thread.

## Security and truth boundaries

- Custom-scheme registration proves OS delivery, not requester authenticity.
  Every value remains hostile until strict Rust routing, protocol validation,
  preview, and explicit consent succeed.
- Unknown `openid4vp` endpoint pairs remain ambiguous and fail closed.
- Raw links, nested request endpoints, clipboard contents, addresses, native
  exceptions, and activity results must not be logged or returned by headless
  methods.
- Only typed public receive addresses may cross the export port. Protocol URIs
  and credential material are deliberately outside its API.
- One pending request cannot replace another. Queue saturation is a stable,
  payload-free failure, never permission to discard the active review.
- Simulator/emulator tests prove packaging and lifecycle behavior. The
  repository physical harness additionally proves one-item warm and cold
  Android custom-scheme delivery into review, with explicit dismissal and no
  native classification, persistence, execution, or request logging. Neither
  kind of evidence proves application-link/domain association or production
  requester discovery.

## Consequences

- The standalone mobile app can be opened from supported identity links in
  both warm and terminated states without bypassing consent.
- Receive addresses use native clipboard and share surfaces while the focused
  Rust port removes any generic-string export method. Call sites must still
  construct the capability-specific type only from reviewed public address
  views.
- Native mobile packaging has one explicit repository-owned bridge crate. A
  future Dioxus upgrade may remove the single-package constraint, but splitting
  the bridge still requires a full native lifecycle and packaging review.
- Universal HTTPS links and arbitrary production OpenID4VP discovery remain
  unavailable until their trust and association policies are accepted. No
  reviewed public domain or valid `assetlinks.json` currently exists, so the
  Android App Link requirement is an external blocker rather than a simulated
  pass.

## Rejected alternatives

- Parsing or routing links in Swift/Kotlin duplicates a security boundary.
- Registering the event handler only inside a Dioxus component loses cold iOS
  events emitted before that component mounts.
- Polling the pasteboard or application intent globally obscures lifecycle and
  expands the secret-bearing surface.
- A generic `copy(String)` or `share(String)` port is too broad for a wallet.
- Keeping separate Swift packages is not buildable with the selected Dioxus
  bundler because only its primary compiled framework is embedded.
