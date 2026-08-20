# ADR-0018: Structured error taxonomy

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 10 and 13
- Implementation state: Partially exercised in M0; cross-domain policy pending
- Amended by: ADR-0038, ADR-0039, ADR-0040, ADR-0080

## Context

External adapters expose unstable, verbose errors that may contain wire data,
identifiers, or secrets. A single opaque failure prevents useful recovery,
while exposing raw errors leaks implementation and sensitive details.

## Proposed decision

Define stable, bounded error categories at each core boundary. Adapters map
external failures to those categories and may retain sanitized diagnostic
causes outside ordinary UI DTOs. Verification uses structured stage outcomes
instead of reducing all evidence to a boolean.

M0 already separates name validation, platform, persistence, conflict, and
generated-identifier failures. Further slices must determine whether common
cross-domain categories add value before stabilizing a shared taxonomy.

## Consequences if accepted

- UI and protocol callers can make safe recovery decisions.
- Adapter-specific diagnostics require an explicitly protected channel.
- Exhaustive enums introduce intentional API evolution work.
- M0's local enums are evidence for the proposal, not a final universal model.
