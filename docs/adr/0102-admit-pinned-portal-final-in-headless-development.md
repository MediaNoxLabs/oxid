# ADR-0102: Admit pinned Portal Final issuance in headless development

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124) and Portal PR #17
- Portal integration source: squash commit `925ec8d04882eabd4ac7b784c70fc2f0c152faae`, tree `58b4597524f88a0ae2253439a44dab0dc60cbb6f`
- Portal lifecycle helper: signed commit `00d3d6c6b9ebe37e1a4bffc4dd7a3f27cf6e4b24`, tree `3cecc6e17d56b2c0d646150df3861005df831ed8`, reviewed in Portal PR #19 (draft dependency; Oxid does not merge it)
- Historical Portal PR head: `9c82db23eabe8b6d758b2731f2225910ea627c14` (the same tree as the landed squash commit)
- Profile source: `76e8edf394a4cb37ca822037272d543c68f25f71`; exact provenance SHA-256 `cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87`
- Amends: ADR-0039 and ADR-0101
- Implementation state: strict native desktop/headless Portal HTTP issuance, authenticated development composition, exact private-material conversion, encrypted import, and new-process restore/reverification are implemented; real Portal evidence now runs only through the fail-closed local same-head recipe, while hosted CI validates public/static contracts without a private repository credential or real-execution claim; ADR-0103 separately admits the same client to an explicit standalone-local iOS/Android test profile, while production/native-custody/tailnet Portal composition remains unavailable

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
Normal `compose()` remains unavailable. Under this decision iOS, Android, and
WebAssembly could not compile or name the Portal client/configuration variant;
ADR-0103 later adds only an explicit compile-time standalone-local iOS/Android
test profile and leaves WebAssembly, production, native custody, and tailnet
closed.

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

The shared headless environment uses one absolute canonical owner-`0600`
`STACK_ENV_FILE`, created only by Portal's generator. The signed lifecycle helper
at `00d3d6c...` and the immutable protocol source at `925ec8d...` are distinct
authorities in that profile. Oxid validates the closed v1 keys, exact roots,
commits, trees, projects, routes, mode, ownership and helper signature without
sourcing dotenv or assigning Portal secret values; it passes only the profile
path to the authenticated Portal helper.

`just local-headless-up <profile>` owns or attaches to the exact
`oxid-standalone` node/indexer/proof project, verifies node, indexer v3+v4 and
proof readiness, then delegates Portal-only startup. An owner receipt is written
outside Git under the profile's private state only when that call actually
starts Midnight. `local-headless-down` delegates Portal cleanup first and may
stop Midnight only when the same profile path, Compose digest and exact three
container IDs match. Attach shutdown never stops Midnight or resets Tailscale.

`just local-headless-test <profile>` requires both owner status documents to be
ready, then runs the strict live flow against the persistent detached
`925ec8d...` protocol worktree. It does not create another Portal checkout or
invoke Portal Compose directly. The harness then:

1. creates and observes an approved mock-KYC session through the real Portal service;
2. routes the real by-value offer through `oxid-headless`;
3. uses an Oxid-managed authentication method and a distinct managed Jubjub assertion method;
4. imports the real credential body, detached proof, and converted private material only after all strict verification stages pass;
5. rejects replay, proves ciphertext-at-rest, and starts a second Oxid process to list and reverify the record.

Only a closed, deterministic boolean/source-pin JSON record is retained under
`target/portal-headless-e2e/evidence.json`. Raw offer codes, tokens, nonces,
proof JWTs, credentials, private parts, claims, DIDs, routes, logs, PIDs, and
timestamps are excluded. Scripted HTTP/component tests remain separate from
this live evidence.

The original iOS simulator and Android emulator evidence exercised the same
incoming router, explicit consent, managed-method, verification,
encrypted-persistence, and restart flows through the embedded standalone
issuer. ADR-0103 adds separately labelled evidence using this exact Portal HTTP
client in the compile-time standalone-local mobile test profile; neither result
is physical-device or production evidence.

### Evidence placement

Real Portal execution is an operator-local evidence boundary. This reviewed
operator decision supersedes issue #124's older hosted-real-execution wording
for PR #137 without weakening the issue's protocol, security, or platform
acceptance requirements. The complete `just portal-local-conformance <profile>` recipe authenticates the signed helper and persistent detached protocol source from the owner-private profile,
runs headless first, then the ADR-0103 iOS/Android platform-plus-standard-smoke
pairs, and validates all retained documents against one immutable Oxid head.
Hosted Oxid CI keeps a required repository-only contract job for the immutable
source lock, orchestration order/cleanup, evidence schema/head/provenance and
acceptance checks, secret sentinel, and sanitized-only publication policy. It
receives no private cross-repository credential, does not fetch or execute
Portal, does not upload local evidence, and must not claim that real conformance
ran.

Portal's own CI remains the correct home for issuer-owned protocol and fixture
tests. It must not fetch and execute an unmerged public Oxid PR while private
Portal source or credentials are present: that would place untrusted proposed
code inside the private trust boundary and create an avoidable disclosure path.
It also cannot directly satisfy Oxid's iOS Simulator and Android QEMU wallet
journey evidence, so it is not a substitute for the Oxid-owned local recipe.

The three retained records are local, head-bound review inputs rather than
hosted artifacts. Reviewers must treat all earlier records as stale after any
Oxid head change and regenerate the complete set before accepting the evidence
gate. A signed/DCO Oxid commit and the exact source-lock provenance make that
review reproducible; they do not turn local development evidence into release
attestation.

## Consequences

- Portal `integration` is now a positive, immutable local interoperability
  input without weakening ADR-0101's historical negative regression gate.
- Production discovery/trust, runtime production-route selection, native-custody
  or tailnet Portal transport, real KYC, live holder DID deployment,
  physical-device camera/tailnet evidence, and promotion beyond integration
  remain separate. ADR-0103 admits only standalone-local virtual-device test
  transport.
- A new lifecycle helper commit/tree, Portal protocol commit/tree, profile source, or provenance digest is
  rejected until this source lock and decision are deliberately reviewed.
- Operator-selected local source/manifest authentication must not be described
  as signed release provenance or production deployment attestation.
