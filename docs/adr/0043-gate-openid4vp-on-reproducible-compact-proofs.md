# ADR-0043: Gate OpenID4VP presentation on reproducible Compact proofs

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #27](https://github.com/MediaNoxLabs/oxid/issues/27)
- Normative source: [OpenID for Verifiable Presentations 1.0 Final](https://openid.net/specs/openid-4-verifiable-presentations-1_0.html)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/oid4vp_client/` and `mobile-bench/dioxus-wallet/`
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Amends: ADR-0007, ADR-0009, ADR-0010, ADR-0013, ADR-0015, ADR-0020, ADR-0021, ADR-0023, ADR-0029, ADR-0040, and ADR-0042
- Implementation state: strict standalone request-by-reference and DCQL parsing, profile-scoped candidate matching, exact consent, single-use session lifecycle, and headless/mobile preview are implemented; ADR-0082 requires visible exact credential selection when several candidates match; ADR-0050 satisfies the proof gate for explicit native headless proving and independent `vp_token` verification, while mobile proving and production transport remain fail-closed
- Amended by: ADR-0044, ADR-0045, ADR-0046, ADR-0082, ADR-0083

## Context

OpenID4VP 1.0 Final defines credential presentation with `vp_token` and DCQL.
The prototype does not implement that flow: its working path is SIOPv2
authentication and its presentation action is disabled. The reference Digital
Passport package contains Compact sources and pure validation tests, but the
pinned revision does not contain committed generated artifacts for that
contract. Its generic holder/context signature is not a selective-disclosure or
age-predicate proof.

Oxid can safely migrate request interpretation, credential matching, consent,
and replay protection now. Calling a locally recomputed predicate, a generic
signature, or a synthetic boolean a proof would create false interoperability
and a serious disclosure failure.

## Decision

Create a separate, dependency-free presentation domain and application
hexagon. It owns profile-scoped preview and lifecycle state, schema-neutral
requested-claim intent, candidate selection, exact consent, and redacted proof
ports. It does not own OpenID JSON, DCQL syntax, Digital Passport codecs, or
Compact artifacts.

The standalone OpenID4VP adapter accepts one deterministic, by-reference,
loopback profile. It strictly and boundedly validates:

1. `response_type=vp_token`, `response_mode=direct_post`, a `response_uri`, and
   the Final `redirect_uri:` client identifier prefix;
2. an exact DCQL query containing one Digital Passport credential query;
3. first-name and last-name reveal intents plus an age-over-18 date-of-birth
   predicate; and
4. duplicate, unknown, oversized, over-deep, expired, replayed, and unsupported
   requests as terminal failures.

`midnight_compact_vp` is an explicit incubating Oxid/Midnight format identifier,
not a registered interoperable OpenID4VP format. The deterministic adapter is a
standalone conformance harness, not a production verifier endpoint.

Preparation may return verifier, purpose, query metadata, requested-claim
labels/intents, and matching credential identifiers. It must never return
claim values, openings, challenges, request state, proof bytes, or response
tokens. Acceptance requires the exact `ACCEPT_CREDENTIAL_PRESENTATION` intent
and an explicit confirmation for one listed credential.

Proof creation and independent proof verification are distinct outgoing ports.
The adapter may construct and submit a `vp_token` only after both operations
succeed for the exact challenge, credential, verifier, and requested claims.
Any proof or verifier error consumes the one-time session, records a terminal
failure, and leaves `presentationGenerated` and `verifierValidated` false. The
current standalone composition deliberately selects unavailable proof and
verifier ports, so acceptance returns `proof_unavailable` and produces no
`vp_token`. Normal production composition keeps the whole protocol unavailable.

Sessions are process-local, profile-scoped, bounded, and single-use. Refusal
discards adapter state. Restart discards incomplete sessions; no request,
consent, proof, or response token is persisted.

## Consequences

- Headless and mobile clients can exercise genuine OpenID4VP request parsing,
  matching, consent, refusal, profile isolation, and replay behavior now.
- Capability discovery must label presentation acceptance as blocked and link
  issue #28; preview readiness must not imply presentation readiness.
- Adding a real prover cannot bypass the same independent verifier gate or
  widen the consented claim selection.
- Live request retrieval, signed request objects, native deep-link/QR ingress,
  response delivery, additional DCQL constructs, and other credential formats
  remain reviewed follow-ups.
- Issue #28 must pin the compiler/toolchain, generate artifacts reproducibly,
  bind proof inputs to the signed credential commitments and OpenID challenge,
  and add positive plus tamper vectors before `vp_token` is enabled.

## Validation

- Domain/application tests cover invariants, exact consent, profile isolation,
  and terminal proof failure.
- Adapter tests cover strict Final-shaped parsing, DCQL mapping, hidden values,
  duplicate/unknown/oversized input, expiry, single use, redacted artifacts,
  and the unavailable-proof boundary.
- Headless tests issue and store a commitment-bound Digital Passport, prepare a
  request, reject missing consent, fail on unavailable proof, and assert no
  private value or `vp_token` reaches protocol output.
- Mobile builds expose the same preview, exact consent, refusal, and explicit
  “no presentation generated” state without weakening production composition.
