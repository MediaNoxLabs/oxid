# ADR-0001: Modular hexagonal architecture

- Status: Accepted
- Date: 2026-08-11

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

Oxid owns its public core types. Third-party types are converted at adapter
boundaries. Ports are capability-specific rather than aggregate wallet or SSI
interfaces.

## Consequences

- Core use cases can be tested without Dioxus, a network, or OS services.
- Early vertical slices have more small crates and explicit mapping code.
- Prototype code must be decomposed during migration instead of copied as one
  `wallet-core` crate.
- New cross-boundary dependencies require an ADR and architecture-check update.
