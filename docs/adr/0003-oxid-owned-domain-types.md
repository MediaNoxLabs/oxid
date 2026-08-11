# ADR-0003: Oxid owns its public domain types

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3, 6, and 18
- Implementation state: Enforced for the M0 vertical slice

## Context

Chain, identity, credential, protocol, storage, and UI libraries evolve on
different schedules and expose models shaped for their own APIs. Allowing those
types into Oxid's core would couple use cases and downstream applications to a
particular adapter.

## Decision

Public domain and application APIs use types owned by Oxid. Adapters translate
external SDK and wire types at the boundary. Core crates do not re-export
third-party models, and a dependency upgrade must not implicitly redefine an
Oxid entity, command, view, or error.

M0 demonstrates this with `WalletProfile`, `WalletProfileId`, `ProfileName`,
`CreateWalletProfileCommand`, and `WalletProfileView`. Even the generated
UUID-shaped identifier is represented as an Oxid-owned opaque identifier.

## Consequences

- Core contracts remain stable when adapters are replaced or upgraded.
- Boundary adapters carry explicit mapping and validation code.
- Lossless round trips may require preserving original signed or wire bytes
  separately from normalized core metadata.
- New SDK types appearing in a core public API violate this decision.
