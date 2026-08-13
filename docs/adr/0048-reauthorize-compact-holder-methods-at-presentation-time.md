# ADR-0048: Reauthorize Compact holder methods at presentation time

- Status: Accepted
- Date: 2026-08-13
- Blueprint: §§3, 5–7, 9–13, 16–18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport presentation and DID custody paths
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Related: ADR-0037, ADR-0043 through ADR-0047, issues #27–29
- Implementation state: standalone presentation preflight now requires current protected control of the exact credential-bound Jubjub assertion method; Compact proof execution, independent proof verification, native custody, and `vp_token` remain fail-closed

## Context

ADR-0047 binds a standalone Compact credential to a DID and method identifier.
The reference family's `ExplicitHolderBinding` does not commit immutable public
key bytes into the credential. A valid stored credential therefore proves
neither that the wallet still controls the method nor that a restored public
DID record has restored custody.

The Digital Passport presentation `Proof` is also not the generic
`midnight-did:jubjub-schnorr:v1` signature exposed by the development DID
adapter. The reference proof signs the credential-family presentation body and
context, signer reference, time, verifier challenge, public key, and nonce
point. Substituting a generic DID signature for that proof would create a token
with the wrong cryptographic meaning.

Oxid needs a current-control gate before it can connect the generated Compact
runtime, and it needs an explicit answer for credentials whose bound method is
rotated without changing its DID URL.

## Decision

`presentation/application` owns a separate
`PresentationHolderAuthorizationPort`. It accepts only profile scope, the exact
holder DID/method extracted from the verified credential, verifier identity,
and the exact 32-byte `MPS1` presentation statement. It returns authorization
or a typed failure; it returns no signature or proof artifact.

After the existing credential, detached issuance proof, private openings,
public-input codec, and independent reconstruction checks succeed, the
standalone Midnight VC adapter must:

1. reload the holder DID from the selected profile;
2. require the exact credential-bound DID and method, an active document, exact
   controller, current-process managed status, `assertionMethod` membership,
   and a canonical non-identity EC/Jubjub public JWK;
3. domain-separate and hash the DID, method, verifier, and exact presentation
   statement;
4. sign that authorization payload through the existing protected DID custody
   use case after the outer presentation consent; and
5. independently verify the returned generic DID signature against the public
   JWK reloaded before signing, then discard the signature.

This signature is a custody attestation and proof-runtime precondition only.
It must never be encoded as the credential-family `Proof`, returned through an
incoming adapter, logged, persisted, or placed in a `vp_token`. Until the real
Compact runtime constructs a proper proof and an independent verifier accepts
it, presentation still terminates with `proof_unavailable` and emits no token.

Locked or unavailable custody maps to `holder_authorization_unavailable`.
Missing management, removed relationships or methods, deactivation, invalid
binding, and signature rejection map to `holder_not_authorized`. Both outcomes
are terminal for that single-use presentation session.

## Rotation policy

The credential binds the DID URL of the holder method, not its issuance-time
JWK bytes. Rotating protected key material while preserving that exact method
identifier and its `assertionMethod` relationship transfers presentation
authority to the new current managed key. The old key no longer authorizes the
credential.

Changing or removing the method identifier, removing `assertionMethod`,
deactivating the DID, losing the profile-scoped custody association, or merely
restoring the public DID document does not preserve presentation authority.
Those states fail closed. A future credential format that commits key bytes or
a key epoch requires a new reviewed policy rather than inheriting this one.

## Consequences

- A presentation consent cannot reach Compact proving unless the wallet
  demonstrates current protected control of the exact holder reference.
- Same-method rotation has deterministic semantics and is covered by a
  headless flow; relationship removal, locked custody, and restart without
  custody have distinct fail-closed results.
- Presentation authorization stays profile-scoped and no private scalar,
  custody reference, signing payload, signature, opening, or claim value
  crosses an incoming adapter.
- The current development confirmation is derived internally because the user
  already approved the exact outer presentation. Native custody must preserve
  user-presence policy without adding secret-bearing UI fields.
- This decision does not implement Compact proof generation, proof encoding,
  independent proof verification, verifier response transport, issuer trust,
  status/revocation, or native platform custody.

## Validation

- The full standalone issuance/presentation test rotates the holder key under
  the same method identifier and reaches the intentional `proof_unavailable`
  gate only after current-key authorization.
- The same flow removes `assertionMethod` and receives
  `holder_not_authorized`, and locks custody and receives
  `holder_authorization_unavailable`; neither response contains `vp_token`.
- The executable restart flow restores the encrypted credential and public DID
  document without restoring process-local custody, then receives
  `holder_not_authorized` and emits no token.
- The authorization request's debug representation omits the exact statement,
  and the unavailable default adapter fails closed.
