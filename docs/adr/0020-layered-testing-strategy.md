# ADR-0020: Testing is layered by boundary and risk

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3, 13, and 18
- Implementation state: M0 baseline implemented; security suites expand later
- Amended by: ADR-0038, ADR-0039, ADR-0040, ADR-0041, ADR-0042, ADR-0043, ADR-0044, ADR-0045, ADR-0046

## Context

Unit tests alone cannot validate adapters or interoperability, while full UI or
network tests are too slow and fragile to carry every core invariant. Wallet
parsers and authorization boundaries need risk-specific verification.

## Decision

Use unit and property tests for domain/application logic, reusable contract
suites for ports, integration/interoperability tests for adapters, focused UI
flows, and fuzzing for security-critical parsers where practical. Keep core
tests independent of UI, network, and OS services.

M0 provides unit tests, a composition smoke test, an executable architecture
dependency check, desktop runtime smoke, mobile/web compile checks, and an 80%
provider-independent core line-coverage threshold. Coverage does not replace
security, dependency, architecture, or platform gates.

## Consequences

- Each capability carries tests at the boundary where failures occur.
- Later chain/SSI adapters need contract fixtures and live or controlled
  interoperability environments.
- Mobile user-facing work requires device-level smoke evidence.
- Coverage exclusions and thresholds must remain explicit and reviewable.
