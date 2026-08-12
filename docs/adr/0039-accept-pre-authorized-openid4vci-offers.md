# ADR-0039: Accept pre-authorized OpenID4VCI offers through a protocol hexagon

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #24](https://github.com/MediaNoxLabs/oxid/issues/24)
- Normative source: [OpenID for Verifiable Credential Issuance 1.0 Final](https://openid.net/specs/openid-4-verifiable-credential-issuance-1_0.html), published 2025-09-16
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/oid4vci_client/` and `mobile-bench/wallet-core/tests/oid4vci_issuance_e2e.rs`
- Supersedes: ADR-0038 statements that all protocol ingress is queued
- Amends: ADR-0004, ADR-0007, ADR-0010, ADR-0011, ADR-0013, ADR-0017, ADR-0018, ADR-0020, ADR-0021, ADR-0023, ADR-0024, ADR-0029, and ADR-0038
- Implementation state: final-shape embedded-offer, pre-authorized-code issuance is implemented with an in-process standalone issuer, explicit consent, DID-bound JWT proof, strict verification, protected persistence, headless flow, and Dioxus mobile flow; production HTTP, discovery, native custody, and other grant/transport variants remain unavailable

## Context

The prototype proves a useful end-to-end issuance journey, but its OID4VCI
client was built against a pre-final draft and combines protocol state,
transport, key material, persistence, and UI-facing progress in one wallet
module. Copying it would preserve obsolete request shapes and give incoming
adapters access to pre-authorized codes, access tokens, nonces, and signed
proofs.

OpenID4VCI 1.0 Final separates Credential Issuer metadata from OAuth
Authorization Server metadata, defines a distinct Nonce Endpoint, uses the
`proofs` object in Credential Requests, and returns a `credentials` array.
Credential offers are untrusted input and must be previewed before the wallet
acts. Oxid also needs a deterministic path that exercises the real custody and
credential-verification boundaries without depending on an external issuer.

## Decision

Add peer `protocol/domain` and `protocol/application` hexagons. Core owns only
bounded offer previews, profile-scoped issuance identifiers, lifecycle states,
commands, and capability-specific ports. It has no JSON, URL, HTTP, OAuth,
JOSE, Dioxus, DID-method, or credential-codec dependency. The application
retains metadata-only sessions in `awaiting_consent`, `issuing`, `succeeded`,
`refused`, or `failed` state. Acceptance requires both `confirmed = true` and
the exact `ACCEPT_CREDENTIAL_ISSUANCE` intent.

`adapters/openid4vci` implements one controlled-edge subset of OpenID4VCI 1.0
Final:

- `openid-credential-offer://` offers sent by value in `credential_offer`;
- one pre-authorized-code grant without `tx_code`;
- separate issuer and authorization-server metadata;
- the optional Nonce Endpoint;
- one credential configuration using the reviewed Midnight phase-1 CBOR
  format profile;
- a `proofs: { "jwt": [jwt] }` Credential Request;
- a `credentials: [{ "credential": ... }]` response.

The parser rejects duplicate JSON members, excessive size/depth, ambiguous or
by-reference offers, duplicate configurations, userinfo, fragments, remote
plaintext HTTP, malformed endpoints, unsupported proof algorithms, and
deferred responses. Unknown extension members inside a valid credential offer
are ignored as required by the specification. Production endpoint validation
is HTTPS-only. Plain HTTP is permitted solely for an explicit IPv4/IPv6
loopback or `localhost` endpoint in the deterministic standalone adapter.

The standalone adapter is in-process and does not bind a socket. It models the
offer, metadata, token, nonce, proof, credential request, and credential
response exchanges using bounded strict documents. Its pre-authorized code,
access token, and nonce are ephemeral, zeroized where retained, single-use,
and never returned through application or incoming-adapter views.

Holder proof generation bridges only to existing `GetDidRecordUseCase` and
`SignDidPayloadUseCase` capabilities. It requires a non-deactivated,
profile-scoped DID method controlled by the holder and authorized for
`authentication`. It emits an EdDSA or ES256 compact JWS with type
`openid4vci-proof+jwt`, DID URL `kid`, issuer audience, current `iat`, and fresh
`nonce`; opaque custody performs the signature. The standalone issuer resolves
the selected public DID method independently, verifies the Ed25519 or P-256
signature, enforces the anonymous-flow `iss` rule, and bounds `iat` freshness
before issuing. No key handle or signing input leaves that bridge.

DID inventory views distinguish methods managed by the current lifecycle
adapter from methods that are merely present in a resolved or restored public
document. The mobile acceptance flow considers only an active authentication
method in that managed set. This prevents a foreign resolved DID that sorts
ahead of the holder DID from being selected for proof generation. The proof
bridge rechecks control and authorization at the application boundary.

Protocol output reaches the protected credential repository only through a
new strict import use case. The existing verifier must produce `valid` before
the original bytes can be atomically persisted. The normal production
composition wires unavailable protocol and sink ports. Standalone headless and
mobile composition wire the in-process issuer, existing development custody,
strict Midnight verifier, and encrypted credential repository.

Headless v1 exposes `credential.issuance.prepare`, `accept`, `refuse`, `get`,
and `list`. Results contain only offer display metadata, lifecycle state, safe
failure code, and final credential identifier. Dioxus previews issuer and
credential display metadata, requires a checkbox confirmation, selects an
active managed authentication method, and refreshes the same protected
inventory after success.

## Alternatives considered

- Copy the prototype OID4VCI module: rejected because it implements draft
  shapes and crosses transport, storage, key, and UI boundaries.
- Add an OID4VCI SDK to core: rejected because protocol and serialization
  dependencies belong at the adapter edge and the final subset is small.
- Start with a live public issuer: rejected because endpoint trust, OAuth
  client policy, redirects, timeouts, and interoperability need a separate
  production adapter review.
- Let the UI submit tokens or nonces: rejected because protocol secrets must
  remain adapter-private.
- Store before verification: rejected because an issuance success must not
  bypass ADR-0038's strict verifier.

## Consequences

- Oxid now exercises the complete standalone issuance journey through the same
  headless and mobile use cases while retaining the direct public fixture inbox
  as a lower-level verifier/storage diagnostic.
- A future live HTTP adapter can replace the deterministic issuer without
  changing consent, DID proof, storage, or incoming adapter contracts.
- Issuance session metadata is process-local. Successfully issued credentials
  survive restart; incomplete offer/token sessions deliberately do not.
- Authorization Code, `credential_offer_uri`, Transaction Code, batch,
  deferred, notification, encrypted response, wallet attestation, and multiple
  proof flows remain unavailable.
- OID4VP/SIOP, selective disclosure, status/schema/trust policy, Compact
  passport proofs, camera/deep-link bridges, and production native custody
  remain separate reviewed slices.

## Validation

- protocol-domain state and bounded-preview tests;
- application consent, profile-scope, refusal, terminal-state, and verified
  sink tests;
- strict offer/metadata/request/response, duplicate-member, endpoint-policy,
  unsupported-variant, and DID proof tests;
- headless offer-to-store flow with a rejected unconfirmed attempt and no
  protocol-secret projection;
- real binary issuance followed by encrypted credential restoration and
  re-verification in a new process;
- Dioxus standalone offer preview/consent/issue inventory flow;
- iOS simulator and Android emulator issuance plus restart smoke flows;
- architecture, formatting, clippy, workspace, coverage, advisory, license,
  source, and rustdoc gates.
