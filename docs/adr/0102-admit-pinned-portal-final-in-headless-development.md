# ADR-0102: Admit pinned Portal Final issuance in headless development

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124) and merged Portal integration
- Portal integration source: commit `22ae5369b6f939e6b20648f4b85dd993527748ef`, tree `74d8d1a5b87c160ea554006e47d5f3edc3cd3e10`
- Final-profile provenance SHA-256: `cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87`
- Amends: ADR-0039 and ADR-0101
- Implementation state: strict native desktop/headless Portal HTTP issuance, exact private-material conversion, encrypted import, and new-process restore/reverification are implemented; one compile-gated iOS Simulator/Android QEMU development profile uses the same client, production composition remains unavailable, and ADR-0103 separately admits one physical-Android conformance profile

## Context

ADR-0101 preserved the incompatible Portal `804de0a9` wire shapes as negative
regression evidence. Portal integration subsequently implemented the reviewed
OpenID4VCI 1.0 Final profile and landed on merged Portal
`integration@22ae5369b6f939e6b20648f4b85dd993527748ef`. The schema-v3 source
lock pins that integration commit and tree plus the existing Final-profile
source commit and provenance digest. Its commit directory contains the lock and
the one authenticated provenance document; the historical protocol fixtures
remain in their original source directory instead of being duplicated.

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
manifest must bind Portal integration commit/tree, provenance digest,
issuer/resolver origins, issuer DID/full assertion
method, and canonical Jubjub public JWK digest. Partial, malformed, mismatched,
or alternate-resolver/live-Midnight combinations fail startup without fallback.
Normal `compose()` remains unavailable. WebAssembly cannot compile or name the
Portal client/configuration variant. Mobile can name it only through the
`standalone-portal` development feature on iOS Simulator or Android QEMU, or
through ADR-0103's separate physical-Android feature.

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

The `standalone-portal` virtual-mobile profile reuses the exact Final client,
deployment manifest, consent, managed-method, verification, encrypted storage,
and embedded holder-DID resolver boundaries. A repository launcher emits a
canonical target/profile declaration only after selecting an installed iOS
Simulator or live Android QEMU target; its caller-supplied digest detects drift
but is not a source attestation. The app receives a fixed non-secret trigger and
fetches one capability-authenticated offer from the repository-owned loopback
18091 harness. The harness unlinks its owner-private inputs before listening,
rejects replay, and retains no protocol material. The profile is compile-time
standalone development only, uses loopback transport without ambient proxying,
and has no production discovery, runtime feature selection, live DID write, or
release promotion path.

## Consequences

- Portal `integration` is now a positive, immutable local interoperability
  input without weakening ADR-0101's historical negative regression gate.
- Production discovery/trust, runtime production-route selection, ordinary
  mobile Portal transport, real KYC, live holder DID deployment,
  physical-device camera/tailnet evidence, and promotion beyond integration
  remain separate.
- A new Portal head, integration tree, profile source, or provenance digest is
  rejected until this source lock and decision are deliberately reviewed.
- Operator-selected local source/manifest authentication must not be described
  as signed release provenance or production deployment attestation.
