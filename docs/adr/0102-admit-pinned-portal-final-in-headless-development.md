# ADR-0102: Admit pinned Portal Final issuance in headless development

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124) and Portal [PR #17](https://github.com/input-output-hk/lace-id-portal/pull/17)
- Portal integration source: squash commit [`925ec8d04882eabd4ac7b784c70fc2f0c152faae`](https://github.com/input-output-hk/lace-id-portal/commit/925ec8d04882eabd4ac7b784c70fc2f0c152faae), tree `58b4597524f88a0ae2253439a44dab0dc60cbb6f`
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
