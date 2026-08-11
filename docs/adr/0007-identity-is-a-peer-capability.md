# ADR-0007: Identity is a peer wallet capability

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 1, 4, 5, and 6
- Implementation state: Architectural commitment; identity modules begin at M3

## Context

Many wallets treat identity as a chain-specific screen or an optional feature
inside an asset model. Oxid is intended to serve identity issuers, verifiers,
white-label products, and users managing both value and credentials.

## Decision

DID lifecycle, credentials, presentations, authentication, and consent are
first-class bounded capabilities alongside wallet and chain concerns. Identity
is not owned by a Cardano or Midnight adapter, and chain concepts do not become
the universal identity model.

The M0 wallet-profile slice intentionally establishes only shared foundation;
it does not introduce placeholder identity crates before concrete M3 use cases.

## Consequences

- Identity use cases may compose with chain capabilities without depending on
  their SDK types.
- Product navigation may present assets and identity as peers.
- Shared wallet/profile concepts must stay neutral enough for both domains.
- Identity delivery remains deferred, not silently satisfied by the profile
  entity implemented in M0.
