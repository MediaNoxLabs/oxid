# ADR-0040: Add consented standalone SIOPv2 DID authentication

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #25](https://github.com/MediaNoxLabs/oxid/issues/25)
- Normative sources: [Self-Issued OpenID Provider v2 draft 13](https://openid.net/specs/openid-connect-self-issued-v2-1_0.html) and [OpenID for Verifiable Presentations 1.0 Final](https://openid.net/specs/openid-4-verifiable-presentations-1_0.html)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/oid4vp_client/`, and its mobile login flow
- Supersedes: ADR-0010 and ADR-0039 statements that all SIOP behavior is queued
- Amends: ADR-0004, ADR-0007, ADR-0010, ADR-0011, ADR-0013, ADR-0017, ADR-0018, ADR-0020, ADR-0021, ADR-0023, ADR-0024, ADR-0029, ADR-0037, and ADR-0039
- Implementation state: deterministic request-by-reference, consented self-issued DID authentication, signed ID Token response, independent standalone verifier, headless flow, and Dioxus mobile flow are implemented; OpenID4VP credential presentation, live transport, native ingress, and production custody remain unavailable

## Context

The prototype calls its client `oid4vp_client`, but its implemented mode does
not present a Verifiable Credential. It creates a self-issued `id_token` for
DID authentication and submits it with `direct_post`; presentation modes are
explicitly rejected. Treating that behavior as credential presentation would
hide an important privacy and consent distinction.

The standards have also diverged in maturity. OpenID4VP 1.0 is Final and
defines `vp_token` presentation using DCQL. SIOPv2 remains draft 13 and defines
a self-issued ID Token whose subject can be a DID. Oxid therefore pins the
draft behavior it implements, keeps it separate from Final OpenID4VP
presentation, and does not claim production interoperability from a
deterministic prototype-compatible harness.

## Decision

Extend the existing dependency-free protocol domain/application hexagons with
an independent self-issued-authentication aggregate. Core owns only a bounded
verifier/purpose preview, a profile-scoped opaque authentication identifier,
lifecycle state, commands, and capability-specific ports. It has no URL, JSON,
JOSE, transport, DID-method, or Dioxus dependency. Application sessions are
process-local and metadata-only. Acceptance requires `confirmed = true`, the
exact `ACCEPT_SELF_ISSUED_AUTHENTICATION` intent, and an active managed DID
authentication method from the selected profile.

`adapters/siopv2` implements a deliberately narrow SIOPv2 draft-13 standalone
profile:

- one prototype-compatible `openid4vp://authorize` invocation containing
  exactly `client_id` and `request_uri`;
- request by reference resolved only by the in-process deterministic verifier;
- `response_type=id_token`, `scope=openid`, and `response_mode=direct_post`;
- one exact loopback `client_id`, request URI, and response URI;
- bounded `nonce`, `state`, `iat`, `exp`, and human-readable purpose;
- EdDSA or ES256 self-issued ID Tokens containing `iss = sub = holder DID`,
  `aud`, `nonce`, `iat`, `exp`, and DID URL `kid`;
- a single-use response consumed and verified by the standalone verifier.

The parser rejects duplicate JSON members, excessive input/depth, unknown or
duplicate invocation parameters, userinfo, fragments, mismatched endpoints,
expired requests, `redirect_uri`, `vp_token`, presentation definitions, DCQL,
and every response mode/type outside this subset. Production endpoint
validation is HTTPS-only. Plain HTTP is allowed only for explicit loopback in
the standalone profile. The request object is deterministic and unsigned
because it never crosses a network boundary; production request-object trust
and transport require a separate adapter decision.

Proof creation bridges only to existing DID lookup and opaque signing use
cases. It rechecks that the DID is active, the selected method is controlled by
the DID, belongs to the current managed set, and is authorized for
`authentication`. No private key, key handle, nonce, state, signing input, or
signed token reaches an incoming-adapter view.

The standalone verifier consumes its session before verification so replay
cannot retry a proof. It independently resolves the public DID document,
checks the DID URL `kid`, relationship, controller, algorithm, signature,
audience, nonce, subject/issuer equality, and time bounds. A response becomes
`succeeded` only after that verification completes.

Headless v1 exposes `identity.authentication.prepare`, `accept`, `refuse`,
`get`, and `list`. Results contain only identifier, verifier, purpose,
lifecycle state, and safe failure code. Dioxus previews verifier and purpose,
states that no credential is disclosed, requires explicit checkbox consent,
and selects only an active managed authentication method.
The prototype-oriented `identity.login` method name aliases only `prepare`; it
cannot bypass preview or consent.

Normal production composition wires unavailable ports. Only explicit
standalone headless/mobile composition wires the deterministic verifier and
development custody.

## Alternatives considered

- Label the flow OpenID4VP: rejected because no `vp_token`, DCQL query, or
  credential presentation exists.
- Upgrade the flow to OpenID4VP Final in the same slice: rejected because
  presentation selection, disclosure policy, and credential-format behavior
  need their own domain and consent model.
- Copy the prototype module: rejected because it combines protocol state, key
  access, transport, and verification and does not independently verify the
  submitted token.
- Return the ID Token through the application/UI: rejected because it exposes
  a replayable protocol artifact and bypasses the verifier boundary.
- Enable arbitrary HTTP verifier endpoints: rejected because live transport,
  redirect policy, client identity, request-object trust, and native ingress
  have not been reviewed.

## Consequences

- Oxid migrates the prototype's useful login-with-DID behavior without
  misrepresenting it as credential presentation.
- Consent and public preview remain protocol-neutral, while a future SIOP or
  verifier transport adapter can replace the standalone edge.
- Sessions and protocol secrets deliberately disappear on restart; there is no
  authentication credential to persist.
- Final OpenID4VP `vp_token`/DCQL presentation, selective disclosure,
  Presentation Exchange compatibility, response encryption, signed request
  objects, deep links/universal links, QR/camera ingress, live verifier
  transport, and production native custody remain separate reviewed slices.

## Validation

- domain bounds and lifecycle tests;
- application profile-scope, exact-consent, refusal, failure, and terminal-state tests;
- strict invocation/request/response, duplicate-member, endpoint-policy,
  unsupported-presentation, expiry, Ed25519/P-256 signature, tamper, and replay tests;
- headless managed-DID login with an unconfirmed rejection and no token/nonce projection;
- Dioxus preview, consent, refusal, and success states;
- iOS simulator and Android emulator login smoke flows;
- architecture, formatting, clippy, workspace, coverage, advisory, license,
  source, and rustdoc gates.
