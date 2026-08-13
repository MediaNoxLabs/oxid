<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0049: Sign Compact holder challenges inside opaque custody

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3, 5–7, 9–13, 16–18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport presentation path
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Related: ADR-0011, ADR-0017, ADR-0037, ADR-0043 through ADR-0048, issues #27–29
- Implementation state: the standalone composition constructs and independently verifies the exact credential-family holder `Proof` before the Compact prover gate; native custody, ZK proof execution/verification, and `vp_token` remain fail-closed

## Context

ADR-0048 proves current control of the credential-bound Jubjub method with a
generic DID signature. That is a consent and custody precondition, not the
Digital Passport family's presentation `Proof`.

The family proof uses Schnorr over Jubjub, but it cannot use a conventional
one-shot `sign(payload)` API. Its challenge commits to the presentation body
root, presentation context, signer reference, creation time, verifier
challenge, public key, and the fresh nonce announcement. The caller therefore
needs the public announcement before it can derive the scalar challenge. The
private key and nonce must still remain inside custody.

Exporting a private scalar, exporting a nonce, teaching wallet core about a VC
schema, or substituting the generic DID signature would each violate an
existing architecture or cryptographic boundary.

## Decision

Oxid uses a synchronous, two-step, adapter-to-adapter challenge-signing
capability:

1. wallet custody resolves an opaque key reference, creates a fresh random
   Jubjub nonce, and passes only canonical compressed public-key and
   announcement bytes to a callback;
2. the Midnight VC adapter derives the exact reference-family challenge from
   its public protocol transcript and returns one canonical field;
3. custody computes the response and returns only the public key,
   announcement, and response; and
4. the VC adapter reconstructs the exact nine-chunk `Proof`, independently
   verifies the Schnorr equation and every signer/time/challenge binding, then
   passes that proof only to the future Compact proof-preimage builder.

`wallet/application` owns the algorithm-shaped protected operation, but no
credential or DID semantics. `identity/application` binds it to a currently
managed DID method. `did-midnight` resolves the opaque key reference.
`vc-midnight` alone owns the Digital Passport challenge and proof codecs.

The operation is deliberately synchronous and does not expose a reusable
session identifier. A nonce cannot survive the call, be completed twice, be
persisted, or cross an incoming adapter. The development implementation derives
the nonce from the protected key seed and fresh `RandomPort` entropy for every
attempt, then zeroizes the random nonce seed.
Production platform adapters may reject this capability until they can retain
the key and nonce under equivalent protection.

The generic ADR-0048 control attestation remains a separate precondition. It
is independently verified and discarded. It is never reused as the family
proof. After the exact family proof is constructed and checked, standalone
presentation still returns `proof_unavailable`: neither this proof nor an
`MPS1` value is a ZK proof or a `vp_token`.

## Rejected alternatives

- Exporting the protected Jubjub scalar or nonce to Rust protocol code would
  break opaque custody and make safe native adapters impossible.
- Passing a caller-chosen challenge into a generic public signing use case
  would expose a signing oracle and bypass the DID/consent binding.
- Keeping an opaque asynchronous nonce session would add replay, expiry,
  persistence, and crash-cleanup state without a current need.
- Reimplementing the credential-family challenge inside wallet custody would
  couple the wallet hexagon to one VC schema and make source updates harder to
  audit.
- Reusing the generic DID signature would produce a different cryptographic
  statement and cannot satisfy the reviewed Compact circuit.

## Consequences

- The exact credential-family holder proof now uses the currently managed key,
  including same-method rotation, without exporting private material.
- Locked custody, missing management, the wrong algorithm, malformed points,
  callback failures, and public-key mismatches fail closed as holder-custody or
  holder-authorization errors, never as the later ZK-prover-unavailable state.
- The challenge transcript remains reviewable in `vc-midnight`; custody stays
  reusable and protocol-neutral.
- A future native adapter must support fresh protected nonce generation and an
  atomic challenge completion or explicitly report the capability unavailable.
- This decision does not authorize ZK proving, artifact loading, independent ZK
  verification, OpenID4VP response transport, or token creation.

## Validation

- A custody/DID test proves the callback runs once, verifies the returned
  Schnorr equation, and observes a locked failure without exposing secret or
  nonce bytes.
- Native VC tests round-trip the exact proof codec and reject wrong body roots,
  verifier challenges, and signer references.
- The complete standalone headless issuance/presentation flow rotates the
  credential-bound key and reaches `proof_unavailable` only after both current
  authorization and exact holder-proof construction; locked, unlinked, and
  restart-without-custody flows emit no `vp_token`.
