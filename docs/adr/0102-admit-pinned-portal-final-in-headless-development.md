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

The original Evidence section above is historical and is superseded for
`just portal-headless-e2e`. That target no longer checks out or starts the
Portal repository, a Portal service, a production issuer, Smocker, or DIDIT.
It now uses an Oxid-owned ephemeral HTTP mock implementing only the strict
issuer metadata, authorization metadata, token, nonce, credential, and issuer
resolution contract. The mock creates valid signed credential bytes and a
detached proof with Oxid's `StandaloneBoundCompactCredentialIssuer`; DIDIT is
not a live or transitive service dependency of the test profile.

One native `oxid-headless` process is admitted to combine the authenticated
Portal-shaped issuer profile with only the exact local standalone bundle:
network `undeployed`, canonical `127.0.0.1` indexer v4 WebSocket/HTTP, node and
proof-server routes, and the public standalone placeholder address. The
ordinary environment constructor remains fail-closed for this combination, so
desktop and production composition are unchanged. Partial, alternate,
read-only, remote, resolver-overridden, checkpointed, locally proving,
submission-journal, Passport Vault, and presentation-artifact combinations are
rejected.

The journey proves explicit consent and zero token/nonce/credential calls when
consent is false, then preserves the pending issuance while that same process
synchronizes through the local indexer. Its numeric live height is compared
with an independent indexer-v4 query within a bounded advancing-tip delta.
After consent it requires a managed authentication method, a separate managed
Jubjub binding, valid import, encrypted persistence, process restart, listing,
and fresh reverification. Docker container/service/image identity must be
unchanged before evidence is published.

The resulting evidence proves `indexer-sync` only. Node and proof-server
readiness checks are prerequisites, not observations of headless node or prover
use, and those interactions remain explicitly unproven. It is not evidence of
real Portal interoperability, DIDIT or KYC, production discovery/trust, an
on-chain issuer DID, chain writes, proving, submission, desktop behavior,
mobile/emulator/physical flows, tailnet behavior, issue #162, Lace changes, or
release readiness. Existing mobile Portal lifecycle and stack ownership remain
unchanged.
