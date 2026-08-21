# ADR-0101: Gate LaceID Portal interoperability on Final OpenID4VCI

- Status: Accepted
- Date: 2026-08-21
- Source: [issue #124](https://github.com/MediaNoxLabs/oxid/issues/124)
- Portal source: `lace-id-portal` commit `804de0a9e58cf48ece3cc6c24b2245bb70bc80f1`
- Related decisions: ADR-0039 and ADR-0097
- Amended by: ADR-0102
- Implementation state at this decision: source-derived negative contract evidence for Portal `804de0a9` is implemented at the existing strict OpenID4VCI adapter boundaries; ADR-0102 separately admits the later pinned Final profile in native headless development without rewriting this historical evidence

## Context

Issue #124 seeks a real LaceID Portal issuance path through Oxid's existing
consent, verification, trust, and protected-storage boundaries. The pinned
Portal source does not yet implement the exact OpenID4VCI 1.0 Final subset
selected by ADR-0039. This mismatch is an upstream and integration gate, not a
reason to broaden Oxid's decoder.

The pinned source shows these incompatibilities:

- the by-value offer URI adds an `issuer_origin` query parameter outside
  `credential_offer`, while the embedded pre-authorized grant emits
  `"tx_code": null` rather than omitting Transaction Code;
- the well-known response combines issuer and authorization-server concerns by
  placing `token_endpoint` in credential-issuer metadata, omits Oxid's separate
  metadata and nonce model, and derives advertised issuer/endpoints from the
  incoming `Host` or always-trusted `X-Forwarded-*` origin;
- the Credential Request uses singular `proof`; the handler checks its
  `proof_type` but accepts the JWT without verifying its signature, `kid`, DID
  authentication relationship, nonce, audience, or `iat`;
- the pre-authorized code is a stateless seven-day session JWT, and the token
  exchange and credential retrieval do not consume the code, access token, or
  nonce. Those artifacts can therefore be replayed during their lifetimes;
- the Credential Response is a singular custom `credential` envelope with
  Compact proof, holder-binding, nonce, and private-parts fields instead of the
  Final `credentials` array consumed by Oxid.

## Decision

Oxid retains the exact controlled-edge OpenID4VCI 1.0 Final subset in ADR-0039:
a by-value offer, separate Credential Issuer and Authorization Server metadata,
the optional Nonce Endpoint model, a pre-authorized grant without `tx_code`,
`proofs: { "jwt": [jwt] }`, and `credentials: [{ "credential": ... }]`.
Portal-specific extra query parameters, null transaction-code declarations,
combined metadata, singular proof, and singular/custom response envelopes do
not create a compatibility mode. No permissive Portal decoder, normalization
layer, or legacy fallback is allowed.

Non-loopback endpoints remain HTTPS-only. Plain HTTP remains limited to
syntactic IPv4/IPv6 loopback or `localhost` in explicit standalone development.
ADR-0097's `standalone-local` and `standalone-tailnet` routes remain separate,
compile-time-only profiles; Portal integration must not introduce runtime
profile selection, plain-HTTP tailnet transport, personal endpoints, or a path
into normal production composition.

Positive HTTP transport, composition, and trust-manifest work is blocked until
linked upstream changes required by issue #124 provide the agreed Final wire
contract, independently verify holder proof, make authorization artifacts
short-lived and single-use, and bind the advertised origin to an explicit
standalone profile rather than reflected request headers. The upstream changes
must be linked and pinned before Oxid can admit positive cross-repository
contract or end-to-end evidence.

## Future trust and provenance manifest

A future positive integration must provide a secret-free, immutable manifest
with at least:

- manifest schema version and the exact compile-time profile identity
  (`standalone-local` or `standalone-tailnet`);
- the OpenID4VCI version and exact supported profile identifier;
- source repository URLs and full commits for LaceID Portal and its selected
  VC, DID, and Compact dependencies;
- immutable OCI image names and SHA-256 digests, plus corresponding SBOM
  digests;
- the validated Credential Issuer origin and holder-DID resolver origin;
- the Midnight network/chain identity and genesis digest;
- the issuer DID, full verification-method identifier, public-key type, and
  public-key digest.

The manifest must contain no seed, scalar, private key, token, nonce, offer
code, credential, private credential part, signing material, certificate
private material, or route credential/configuration. A future reviewed
implementation must authenticate the exact manifest bytes through the selected
compile-time standalone profile before consuming them; the manifest and its
consumer are **not implemented by this slice**.

## Evidence boundary

The checked-in Portal fixtures are derived from the pinned source files and
replace tokens, nonces, JWTs, credentials, private parts, and origins with
visibly synthetic values. Their provenance records full source paths and
source/fixture SHA-256 digests. They are not runtime HTTP captures and do not
claim byte-for-byte observation of a running Portal deployment.

The adapter-local tests prove only a negative contract gate: the source-derived
offer, metadata, request, and response are rejected at Oxid's existing strict
parser/validator boundaries, while known-valid Oxid controls remain accepted.
Source inspection supplies the separate origin-reflection, proof-verification,
and replay/lifetime findings; the fixtures do not simulate those runtime
behaviors.

This ADR contains no positive Portal acceptance or runtime evidence. ADR-0102
owns the later positive native-headless Portal integration; simulator/emulator
framework evidence and physical-device/tailnet evidence remain separately
labelled. Issue #124 remains open until its deferred phases are truthfully met.

## Consequences

- Oxid deliberately cannot interoperate positively with the incompatible
  `804de0a9` contract recorded here; ADR-0102's later profile must not be used to
  erase this regression evidence or claim completion of issue #124.
- Portal changes can be evaluated against one exact Final contract without
  weakening production parsing or transport policy.
- Future headless, simulator, device, and tailnet evidence must use real Portal
  HTTP output after the upstream, origin, replay, proof, profile, and trust
  gates are satisfied; source-derived fixtures cannot substitute for it.
- Removing the negative fixtures would remove regression evidence only; it
  would not roll back or alter runtime behavior because this slice adds no
  Portal transport, composition, trust consumer, or decoder.
