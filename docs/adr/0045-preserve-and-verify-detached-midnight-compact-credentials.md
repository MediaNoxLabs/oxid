# ADR-0045: Preserve and verify detached Midnight Compact credentials

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #29](https://github.com/MediaNoxLabs/oxid/issues/29)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/vc_store/` and `wallet-core/src/oid4vci_client/credential/digital_passport.rs`
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Amends: ADR-0003, ADR-0007, ADR-0009, ADR-0011, ADR-0013, ADR-0015, ADR-0017, ADR-0020, ADR-0021, ADR-0023, ADR-0038, ADR-0039, ADR-0041, ADR-0042, ADR-0043, and ADR-0044
- Implementation state: exact Compact credential body, detached issuance proof, and private opening material are independently bounded, retained, encrypted, restored, and verified in standalone flows; ADR-0046 supplies exact development Jubjub signing through opaque references, while issuer-method anchoring, selected-DID holder binding, native custody, presentation proving, and production transport remain fail-closed

## Context

The prototype's Digital Passport is not the proof-bearing phase-1 CBOR fixture
Oxid first used to establish the credential hexagon. It stores three distinct
objects:

1. an MCV1-encoded Compact credential body;
2. an MCV1-encoded detached issuer proof; and
3. private claim values and commitment openings needed by the holder.

Collapsing these into one opaque blob would erase their different lifecycle and
verification rules. Treating the detached issuance signature as an OpenID4VP
presentation proof would be incorrect. Continuing to issue only the synthetic
phase-1 CBOR fixture would also leave the standalone application unable to test
the same stored representation as the mobile prototype.

The prototype derives a demonstration holder scalar from the public claim root
when constructing a presentation. That makes the example deterministic but is
not an acceptable production custody model.

## Decision

Credential core adds the explicit `midnight_compact_vc` format and an optional
`CredentialDetachedProof`. The proof is bounded to 1 MiB, debug-redacted, and
owned atomically by `CredentialRecord` beside—but never concatenated with—the
original body and optional 256 KiB format-private material. Application and
protocol ports transport these three values only through verified internal
commands. Ordinary views, headless responses, Dioxus state, capability output,
and errors omit all three byte payloads.

`adapters/vc-midnight` routes only an exact `MCV1` body to the Compact verifier.
It rejects detached proof data for phase-1 CBOR, so supplying a proof cannot
change the interpretation of another format. The Compact verifier:

- requires canonical, exact-end 18-chunk body and 9-chunk proof containers;
- reconstructs the exact Digital Passport package/schema and claim root;
- reconstructs the upstream credential-body and issuance-payload roots;
- parses canonical Jubjub points/scalars, rejects identity proof points, and
  verifies the Schnorr equation; and
- pins a cross-language fixture body root and positive/tamper vectors against
  the immutable reference implementation.

Successful verification marks structural, proof, and schema stages passed.
Issuer DID method anchoring, current-time expiration policy, status, and trust
remain explicitly `not_checked`; the embedded proof public key is not presented
as trusted merely because its signature is internally valid. Adding Jubjub to
the owned DID/JWK model and resolving the issuer method is tracked by issue #29.

`storage-credential-json` writes schema version 3 and stores the optional
detached proof inside the same authenticated XChaCha20-Poly1305 document as the
credential body and private material. It reads version 1 as body-only and
version 2 as body plus optional private material. Any base64, bound, domain, or
authentication failure remains an integrity error.

The deterministic standalone OpenID4VCI issuer now returns the exact public
Compact body/proof fixture and matching private opening envelope. Headless
issuance, claim-free disclosure planning, process restart, re-verification, and
deletion exercise that representation. The standalone inbox deliberately keeps
the older CBOR fixture as a second-format conformance path.

Protected holder Jubjub custody must be a separate key-operation adapter. Oxid
must not reproduce public claim-root-derived holder secrets in normal or mobile
composition. A deterministic public key may be used only by a clearly marked
standalone conformance prover. This ADR does not open the ADR-0043/0044
presentation gate: detached issuance proof verification is not presentation
proof generation or independent `vp_token` verification.

## Consequences

- Oxid preserves the prototype's exact credential representation without
  leaking schema or codec types into domain/application APIs.
- Existing encrypted development wallets migrate forward without rewriting or
  fabricating absent proof material.
- Issued standalone Digital Passports can be re-verified after restart, while
  response DTOs continue to expose metadata and stage codes only.
- Cryptographic consistency is separated from issuer authorization and trust;
  unsupported stages remain visible instead of being reported as passed.
- Production issuance and presentation remain unavailable until native
  wrapping, issuer-method anchoring, selected-DID/public-key holder binding,
  runtime proving, and independent verification are delivered. ADR-0046
  supplies only the exact process-local development Jubjub primitive.

## Validation

- Rust reproduces upstream body root
  `b42f1115042cefecbd5380a0a630c0ef5f18bb13e7615cb1de9d36256f100432`
  and accepts the exact detached issuance proof.
- Body, proof, missing-proof, trailing-data, cross-format, and identity-point
  tamper cases fail closed.
- Storage tests cover schema-1/schema-2 migration, schema-3 round trips,
  authenticated corruption, and ciphertext non-disclosure.
- Headless tests cover consented exact-Compact issuance, profile isolation,
  claim-free disclosure, encrypted process restart, detached-proof
  re-verification, and deletion without exposing protected bytes.
