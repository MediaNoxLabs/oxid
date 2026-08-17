# ADR-0073: Anchor standalone Compact credential policy

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport issuer and verification paths
- Tracking: issues #2, #29, and #34
- Implementation state: standalone composition resolves and authorizes the exact Compact issuer method, binds its Jubjub key to the detached proof, enforces issuance/proof/expiry time policy, and requires the pinned standalone trust anchor; credential status remains explicitly not checked and normal production composition remains unavailable

## Context

ADR-0045 preserved the prototype's exact Compact credential body and detached
issuer proof, then independently verified their roots and Jubjub Schnorr
equation. That deliberately proved cryptographic self-consistency without
claiming that the proof key belonged to an authorized issuer, that the
credential was currently valid, or that the issuer was trusted. The seven-stage
verification report therefore left issuer, temporal, status, and trust as
`not_checked`.

That proof-only verifier remains valuable for immutable historical conformance
vectors, including credentials whose fixed timestamps are no longer current.
It is not sufficient for accepting a newly issued standalone credential into
the active wallet. The standalone issuer already uses one stable public
reference key, but that key was not represented through the same DID resolution
and relationship boundary used by other credentials.

## Decision

Keep `MidnightCompactCredentialVerifier::default()` proof-only for exact
historical conformance. Standalone wallet and headless composition must instead
construct it with an explicit resolver, clock, and Digital Passport issuer trust
anchor.

The standalone DID resolver publishes the exact reference issuer as:

- DID
  `did:midnight:undeployed:a4c9483a0c7cdd808056a93334ab97207b38b4363d1da5cbfb78ad256cd689f0`;
- verification method `#issuer-key-1`;
- controller equal to that DID;
- `assertionMethod` authorization; and
- an EC/Jubjub public JWK whose canonical 32-byte x and y coordinates equal the
  public point carried by the detached Compact proof.

Policy-enabled inspection performs the existing exact structural, schema, and
proof checks first. It then fails closed unless:

1. the credential's exact issuer DID resolves successfully;
2. the referenced method exists, is controlled by that DID, and is assertion
   authorized;
3. its canonical EC/Jubjub key equals the detached proof key;
4. issuance is not in the future;
5. proof creation is not before issuance or in the future;
6. proof creation precedes credential expiration and the credential is not
   currently expired; and
7. the exact DID, method, and proof key match the explicitly supplied trust
   anchor.

Failures use stable redacted reason codes and never expose the credential body,
proof, key coordinates, or protected claims. Resolver unavailability is an
error outcome rather than an invalid-credential claim. A successful standalone
report marks structural, issuer, proof, temporal, schema, and trust passed.
Status remains `not_checked`: the reviewed Compact credential has no status
reference and this decision introduces no revocation registry or resolver.

Normal production composition remains unavailable. It must not inherit the
standalone DID document or trust anchor. A later production adapter must select
its own issuer resolution, trust, status, and freshness policy explicitly.

## Security and privacy consequences

- A self-contained valid signature can no longer be accepted by standalone
  composition as issuer authority by itself.
- Key substitution, wrong relationship, wrong controller, future issuance,
  invalid proof chronology, expiry, and untrusted issuers fail closed.
- Trust is a composition input rather than an ambient global or a wallet-core
  dependency.
- The mobile and headless surfaces expose only stage outcomes and stable reason
  codes. They do not expose DID documents, JWK coordinates, proof material, or
  claim values.
- Revocation is not inferred from absence of a status reference. The UI must
  continue to say that status was not checked.

## Consequences

- Newly issued standalone credentials are useful after a process restart only
  while their issuer, chronology, and trust anchor still validate.
- Exact historical Compact fixtures can continue to test byte and proof
  conformance through the proof-only constructor without weakening active
  standalone wallet policy.
- The standalone DID resolver gains one immutable public issuer document; it
  does not gain mutation authority or private issuer material.
- No new dependency or network lookup is introduced. The standalone policy is
  deterministic and available in simulator, emulator, and headless flows.

## Rejected alternatives

- Treating the embedded proof key as trusted would collapse cryptographic
  validity into issuer authority.
- Enabling wall-clock checks in the proof-only default would invalidate the
  immutable historical vectors and hide the distinction between conformance
  and active-wallet policy.
- Marking status passed because the credential has no status reference would
  fabricate revocation evidence.
- Reusing the standalone trust anchor in normal production composition would
  turn a deterministic test policy into a production trust decision.
