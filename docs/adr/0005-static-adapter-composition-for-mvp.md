# ADR-0005: MVP adapters are statically composed

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 5 and 6
- Implementation state: Implemented for M0

## Context

Oxid must support replaceable integrations, but runtime-loaded native plugins
on Android and iOS add distribution, ABI, signing, and security complexity
before concrete product needs justify it.

## Decision

Select adapter crates through Cargo features and wire concrete implementations
at a composition root. Runtime native plugin loading is deferred beyond the
MVP. Replaceability means preserving ports and boundary mappings, not loading
untrusted code dynamically.

`crates/composition` currently constructs the in-memory repository, system
clock, OS randomness, and wallet-profile service. `apps/oxid` launches the UI
with that composed capability set.

## Consequences

- Builds have an explicit, auditable adapter set.
- Mobile packaging uses ordinary statically linked Rust artifacts.
- Changing adapters requires a rebuild.
- Any future dynamic extension mechanism requires a superseding ADR with a
  threat model, compatibility contract, and platform distribution design.
