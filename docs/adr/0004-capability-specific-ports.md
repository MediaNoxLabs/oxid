# ADR-0004: Ports are capability-specific

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3 and 7
- Implementation state: Enforced for M0; future port names remain illustrative
- Amended by: ADR-0038, ADR-0039, ADR-0040, ADR-0094

## Context

A single wallet, chain, identity, or SSI service interface would force adapters
to claim unsupported behavior and would make small capability changes ripple
through unrelated integrations. DID methods and credential formats in
particular have different capability sets.

## Decision

Define small incoming use-case traits and outgoing capability ports at the
application boundary that owns the need. Do not create a universal plugin or a
god service. Capability availability must become explicit when an integration
cannot support every operation.

M0 contains `CreateWalletProfileUseCase`, `WalletProfileRepository`,
`ClockPort`, and `RandomPort`. Names listed in Blueprint Section 7 are a design
vocabulary, not authorization to scaffold unused traits.

## Consequences

- Adapters implement only capabilities they actually provide.
- Tests can substitute narrow fakes and exercise failure boundaries directly.
- Composition has more individual dependencies to wire.
- Capability discovery and negotiation will need explicit application models
  before heterogeneous chain, DID, or credential adapters are introduced.
