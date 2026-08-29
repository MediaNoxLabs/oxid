# ADR-0102: Admit pinned Portal Final issuance in headless development

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124) and Portal PR #17
- Portal integration source: squash commit `925ec8d04882eabd4ac7b784c70fc2f0c152faae`, tree `58b4597524f88a0ae2253439a44dab0dc60cbb6f`
- Historical Portal PR head: `9c82db23eabe8b6d758b2731f2225910ea627c14` (the same tree as the landed squash commit)
- Profile source: `76e8edf394a4cb37ca822037272d543c68f25f71`; exact provenance SHA-256 `cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87`
- Amends: ADR-0039 and ADR-0101
- Implementation state: strict native desktop/headless Portal HTTP issuance, authenticated development composition, exact private-material conversion, encrypted import, and new-process restore/reverification are implemented; production and mobile Portal HTTP composition remain unavailable

## Context

ADR-0101 preserved the incompatible Portal `804de0a9` wire shapes as negative
regression evidence. Portal PR #17 subsequently implemented the reviewed
OpenID4VCI 1.0 Final profile and landed on Portal `integration` as squash commit
`925ec8d`. The landed tree is byte-identical to historical PR head `9c82db2`.
The profile provenance was authored at `76e8edf`; squash history does not make
that commit an ancestor of the landed integration commit, so the source lock
records all three identities rather than conflating them.

The integration remains a controlled development capability. A local source
lock plus an operator-supplied exact deployment-manifest digest authenticates
reviewed bytes and public deployment facts for this harness; it is not a
production service attestation.

## Decision

Add one native desktop/headless-only Portal composition path selected only when
both of these variables are present:

- `OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH`;
- `OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256`.

The path must be absolute, regular, and non-symlinked. The exact canonical
manifest must bind Portal integration commit/tree, historical PR head, profile
source, provenance digest, issuer/resolver origins, issuer DID/full assertion
method, and canonical Jubjub public JWK digest. Partial, malformed, mismatched,
or alternate-resolver/live-Midnight combinations fail startup without fallback.
Normal `compose()` remains unavailable, and iOS, Android, and WebAssembly cannot
compile or name the Portal client/configuration variant.

The Portal client accepts only the pinned Final profile: by-value offer,
separate issuer and authorization-server metadata, form token request, empty
POST to the Nonce Endpoint, one `proofs.jwt` proof, and exactly one
`credentials` item with the narrow Midnight extension. It disables ambient
proxies, redirects, retries, cookies, automatic replay, and unbounded bodies.
Non-loopback endpoints require HTTPS; plaintext remains limited to syntactic
loopback/`localhost` development. No legacy Portal decoder or normalization
fallback is introduced.

Composition bridges `PortalCredentialMaterialDecoder` only to
`convert_portal_private_parts`. The existing application consent state machine,
managed authentication proof, separately selected managed Jubjub assertion
method, `MidnightCredentialVerifier`, valid-only sink, and encrypted credential
repository remain the owners. The live issuer's DID is resolved through the
manifest-selected resolver and checked against the exact trust anchor. DID
resolver output may use the exact legacy JWK-2020 context and null optional
collections emitted by the pinned resolver; the adapter maps that one context
to Oxid's canonical JWK context and never relaxes the issuer/method/key checks.

## Evidence

`just portal-headless-e2e` fails closed unless it can authenticate a clean
Portal checkout, fetch the exact landed integration commit and historical PR
head, prove their tree identity, verify the profile provenance, and start the
landed repository composition. The harness then:

1. creates and observes an approved mock-KYC session through the real Portal service;
2. routes the real by-value offer through `oxid-headless`;
3. uses an Oxid-managed authentication method and a distinct managed Jubjub assertion method;
4. imports the real credential body, detached proof, and converted private material only after all strict verification stages pass;
5. proves ciphertext-at-rest and starts a second Oxid process to list and reverify the record.

Only a closed, deterministic boolean/source-pin JSON record is retained under
`target/portal-headless-e2e/evidence.json`. Raw offer codes, tokens, nonces,
proof JWTs, credentials, private parts, claims, DIDs, routes, logs, PIDs, and
timestamps are excluded. Scripted HTTP/component tests remain separate from
this live evidence.

The iOS simulator and Android emulator continue to exercise the same incoming
router, explicit consent, managed-method, verification, encrypted-persistence,
and restart flows through their compile-time standalone test framework. They do
not compile or claim the native-headless Portal HTTP route.

## Consequences

- Portal `integration` is now a positive, immutable local interoperability
  input without weakening ADR-0101's historical negative regression gate.
- Production discovery/trust, runtime production-route selection, native mobile
  Portal transport, real KYC, live holder DID deployment, physical-device
  camera/tailnet evidence, and promotion beyond integration remain separate.
- A new Portal head, integration tree, profile source, or provenance digest is
  rejected until this source lock and decision are deliberately reviewed.
- Operator-selected local source/manifest authentication must not be described
  as signed release provenance or production deployment attestation.

## Amendment — 2026-08-26

Portal subsequently landed the reviewed Final implementation at
`22ae5369b6f939e6b20648f4b85dd993527748ef`, tree
`74d8d1a5b87c160ea554006e47d5f3edc3cd3e10`. That later integration identity
supersedes the runtime pin recorded above; this amendment preserves the
original accepted record rather than rewriting its 2026-08-21 history.

The source lock moved from `oxid-portal-source-lock-v2` to
`oxid-portal-source-lock-v3`. The v3 source lock still binds
`profileSourceCommit=76e8edf394a4cb37ca822037272d543c68f25f71` and the exact provenance
digest, but no longer treats the historical pull-request head as runtime
authority. The runtime deployment manifest moved to
`oxid-portal-deployment-v3`: it authenticates the landed integration
commit/tree, provenance digest, issuer/resolver origins, issuer DID and method,
and canonical Jubjub JWK digest. It does **not** attest `portalPrHead` or
`profileSourceCommit`; the latter remains a compiled-in source-lock check, not
a deployment-manifest claim.

The strict native client is additionally admitted through one authority-gated
iOS Simulator/Android QEMU development profile. Its loopback bearer capability
authenticates the app to the listener, not the plaintext listener to the app;
a competing local listener could consume the capability and choose a candidate
offer. This one-directional trust is limited to the compile-gated development
profile and is bounded by the strict offer router, explicit holder consent, and
full issuer DID/method/JWK trust, credential-proof, and holder-binding
verification before encrypted storage. ADR-0103 separately governs the
HTTPS-authenticated physical Android conformance profile.

## Corrective amendment — 2026-08-28

The Phase 1 live target must exercise the production-ready Rust implementation
from Lace `origin/integration`; an Oxid-owned issuer mock is not acceptable live
evidence. The TypeScript prototypes in `midnight-identity-solution-examples`
remain behavioral/protocol references and are neither copied nor run by
`just portal-headless-e2e`.

The target fetches and authenticates Portal integration commit
`22ae5369b6f939e6b20648f4b85dd993527748ef`, tree
`74d8d1a5b87c160ea554006e47d5f3edc3cd3e10`, and the retained Final-profile
provenance. It builds the Lace resolver, did-manager, and default Rust issuer
images. The Oxid-owned consumer compose preserves Lace's production Rust
composition and supported local mock seam while omitting only the duplicate
Midnight services: the Rust issuer's `DiditHttpAdapter` targets the in-stack
Smocker, and the exact Lace `mock/didit.yml` is loaded through Smocker's admin
API. No live DIDIT endpoint or external KYC provider is called. Oxid's Rust
holder-resolver helper is limited to resolving the process-local holder DID for
the client test; it does not replace the issuer service.

Lace's did-manager and resolver use the existing healthy `oxid-standalone`
node, indexer, and proof-server routes. The Lace bootstrap job creates the
issuer DID with a Jubjub assertion key and hands its method to the Rust issuer;
the issuer signs through Lace's did-manager custody service. Resolving that DID
through the Lace resolver proves that the bootstrap reached indexed local
state. This does not claim direct observation of node or prover interaction by
Oxid.

One native `oxid-headless` process is admitted to combine the authenticated
Portal profile with only the exact local standalone bundle: network
`undeployed`, canonical `127.0.0.1` indexer v4 WebSocket/HTTP, node and
proof-server routes, and the public standalone placeholder address. The
ordinary environment constructor remains fail-closed for this combination, so
desktop and production composition are unchanged. Partial, alternate,
read-only, remote, resolver-overridden, checkpointed, locally proving,
submission-journal, Passport Vault, and presentation-artifact combinations are
rejected.

The running Lace KYC flow returns the same exact `credentialOfferUri` that its
completion UI stores for QR and copy-link representation. The journey routes
that URL to Oxid, prepares issuance, rejects acceptance once with consent false,
and observes zero token, nonce, and credential calls before explicitly
accepting. While issuance remains pending, that same process derives an account
and synchronizes through local indexer v4; its websocket replay reports equal numeric current and target cursors. The accepted
Digital Passport must pass every required verification stage, encrypted
persistence, listing, process restart, restoration, and fresh reverification.

Cleanup is receipt-scoped to the `oxid-portal-consumer` Compose project and
never globally prunes or removes `oxid-standalone`. Exact-head secret-free
evidence records `portalServiceExercised:true`, the Lace integration
commit/tree/provenance, and `diditProviderMode:"lace-smocker"`. It reports only
`oxid-headless-indexer-sync` as proven Midnight interaction; node and
proof-server interactions remain explicitly false. This is not live DIDIT,
real-person KYC, production discovery/trust, release evidence, Oxid proving or
submission evidence, desktop/mobile/tailnet evidence, issue #162, or a Lace
source change.

## Test-only ARM64-Darwin amendment — 2026-08-29

One owner-invoked `desktop-portal-test` profile may reuse the exact native
Portal plus local-standalone policy in the real Dioxus `oxid-app` on ARM64
macOS. This is a simple test target for faster desktop/simulator feedback, not
a primary desktop product target, hosted graphical lane, or release profile.
It is absent from normal desktop/mobile/web composition and from the public
`HostedTarget` matrix.

The profile adds one compile-gated `QrScannerPort` adapter over the existing
owner-private port-18091 handoff. A single admission is burned when the visible
rendered Scan action calls `scan()`; only then may the adapter unlink the fixed
app-private capability and fetch the exact bounded offer. The offer and
capability cannot enter argv, environment, logs, retained JSON, or screenshots.
Malformed, unauthenticated, replayed, concurrent, second, and late requests
fail closed, and UI admission cannot replace a pending identity request. The
result remains the unchanged `ScannedQrPayload`; the normal strict router,
offer preview, explicit consent, verifier, and encrypted store retain authority.

macOS Accessibility is tried first. If WKWebView traversal is unavailable or
TCC-blocked, the compile-gated in-process Dioxus document driver may call
`.click()` only on rendered controls. It has no scanner, router, or use-case
surface and is excluded from normal release binaries. Live evidence must use
the Lace Rust issuer with supported Smocker, launch a clean second app process,
show fresh Reverify, retain only closed exact-head JSON and protocol-redacted
native window crops, and describe only app-observed indexer sync. Node and
proof-server interaction remain unproven unless directly observed.
