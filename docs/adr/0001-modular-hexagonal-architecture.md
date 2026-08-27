# ADR-0001: Modular hexagonal architecture

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3 and 6
- Implementation state: Enforced for the M0 dependency graph
- Amended by: ADR-0104

## Context

The prototype combines wallet, Midnight ledger, proving, SSI, persistence,
network, and UI concerns in a ledger workspace. Oxid must remain reusable across
chains, identity methods, credential formats, storage engines, and platforms.

## Decision

Oxid uses bounded Rust modules with hexagonal dependency direction. Domain code
owns entities and invariants. Application code owns use cases and its outgoing
ports. External SDKs, persistence, networking, and operating systems are
outgoing adapters. Composition selects concrete adapters.

The initial dependency graph is executable policy in
`scripts/check-architecture.sh`. Core crates may depend only on explicitly
allowed inward workspace crates.

Boundary type ownership and port granularity are specified independently by
[ADR-0003](0003-oxid-owned-domain-types.md) and
[ADR-0004](0004-capability-specific-ports.md). This record governs the overall
module and dependency direction.

## Consequences

- Core use cases can be tested without Dioxus, a network, or OS services.
- Early vertical slices have more small crates and explicit mapping code.
- Prototype code must be decomposed during migration instead of copied as one
  `wallet-core` crate.
- New cross-boundary dependencies require an ADR and architecture-check update.
