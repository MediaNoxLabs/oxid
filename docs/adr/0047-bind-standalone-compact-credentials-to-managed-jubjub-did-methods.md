# ADR-0047: Bind standalone Compact credentials to managed Jubjub DID methods

- Status: Accepted
- Date: 2026-08-13
- Blueprint: §§3, 5–7, 9–13, 16–18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport holder and DID paths
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Related: ADR-0037, ADR-0039, ADR-0045, ADR-0046, issues #27–29
- Implementation state: standalone DID creation, OpenID4VCI consent, and exact Compact re-issuance bind a protected Jubjub assertion method to the credential holder reference; presentation-time re-authorization, native custody, issuer anchoring, and proof execution remain fail-closed

## Context

The exact Compact fixture migrated by ADR-0045 contains a holder DID contract
address and method identifier, but until now standalone issuance returned those
fixed prototype participants regardless of the DID selected for OpenID4VCI.
ADR-0046 added correct Jubjub key generation and signing behind opaque custody,
but an unassociated key is not holder authorization. Returning the fixed body
after authenticating with an unrelated Ed25519 method would preserve valid
bytes while issuing the credential to the wrong holder.

OpenID4VCI proof-of-possession and Compact credential holder binding also use
different methods. The former is a nonce/audience-bound JWT signed by an
authentication method; the latter must name a Jubjub assertion method whose
protected key can later construct the credential-family presentation proof.

## Decision

Standalone DID creation now provisions three protected methods through
`WalletKeyOperationPort`: Ed25519 authentication, P-256 assertion, and Jubjub
holder presentation. The Jubjub method is public as an EC/Jubjub JWK with the
official little-endian coordinates, belongs to `assertionMethod`, and is marked
managed only while its profile-scoped opaque custody association exists.

Credential acceptance carries two explicit method identifiers:

1. `methodId` authenticates the OpenID4VCI request with the existing JWT proof;
2. `holderBindingMethodId` selects the credential holder's Jubjub assertion
   method.

The OpenID4VCI adapter independently reloads the selected profile's DID record
and rejects a deactivated, foreign-controller, unmanaged, non-assertion, or
non-Jubjub holder method. It passes only the normalized DID/method reference and
public JWK coordinates through the credential-application-owned
`BoundCredentialIssuerPort`.
No key reference or private scalar crosses that port.

The Midnight VC adapter then:

- requires the undeployed DID identifier to be the canonical lowercase
  32-byte contract address and the method to be a bounded canonical fragment;
- decodes the JWK coordinates into a non-identity Jubjub point;
- replaces the holder reference in the exact 18-chunk credential value;
- canonically re-encodes `MCV1` and rebuilds the detached nine-chunk issuer
  proof over the changed credential root using the existing public,
  deterministic standalone issuer fixture; and
- retains the separately bounded private opening envelope unchanged.

The credential family's `ExplicitHolderBinding` signs the DID contract address
and method identifier, not immutable public-key bytes. The JWK coordinates are
therefore issuance-request authorization evidence: they must resolve to a valid
managed Jubjub method, but they are not added to the 18-chunk credential. DID
method rotation semantics and current-key authorization remain presentation
policy, not a field silently invented by Oxid.

The standalone issuer scalar and nonce are public conformance inputs inherited
from the reference fixture. They are not production issuer custody or trust.
Normal composition has no bound-credential issuer and remains unavailable.

## Presentation boundary

Issuance binding does not authorize a future presentation forever. Before real
proof construction, the presentation adapter must reload the current selected
profile DID, require that the exact holder method encoded in the credential is
still active, assertion-authorized, managed, Jubjub, and backed by the currently
authorized protected public key, and sign the credential-family proof challenge
inside custody. A restored public DID record is not proof that its private key
is still owned. Whether a credential survives an intentional rotation of the
same method identifier must be decided explicitly before production proving.

The existing preflight continues to verify the dynamically bound credential,
detached issuer proof, openings, and public statement, then returns
`proof_unavailable`. It generates no presentation or `vp_token`.

## Consequences

- Standalone credentials are issued to the selected profile-managed holder
  instead of the checked-in prototype holder.
- Authentication-key substitution, assertion-relationship substitution,
  wrong-curve selection, non-canonical DID/method encoding, malformed Jubjub
  points, and profile mismatch fail before credential persistence.
- DID creation and headless/mobile surfaces expose one additional public method
  but no additional secret-bearing input or output.
- Exact static upstream body/proof vectors remain checked and round-trip through
  the native codec; dynamic vectors add a holder-specific body and valid
  detached proof without rewriting the fixture files.
- Native key wrapping, user presence, issuer DID anchoring, presentation-time
  re-authorization, credential-family challenge signing, proof execution,
  independent verification, and `vp_token` remain required production gates.
