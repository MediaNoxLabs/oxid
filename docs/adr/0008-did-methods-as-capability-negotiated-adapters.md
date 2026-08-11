# ADR-0008: DID methods are capability-negotiated adapters

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 7 and 9
- Implementation state: Planned for M3 and M5

## Context

DID methods differ in creation, resolution, update, deactivation, verification
methods, service endpoints, networks, and custody requirements. Treating every
method as a uniform CRUD provider would misrepresent real behavior.

## Decision

Implement each DID method as one or more adapters behind focused identity
capability ports. Expose supported operations through explicit capability
metadata or negotiation; never infer that resolution implies update or
deactivation support.

Prioritized methods are `did:key`, interoperable `did:peer` variants,
`did:web`, `did:webvh`, `did:prism`, and `did:midnight`. Their exact normative
versions and adapter dependencies must be recorded before implementation.

## Consequences

- Method-specific restrictions remain visible to use cases and consent UI.
- DID domain types stay method-neutral where the standards permit.
- Composition may combine multiple adapters to satisfy the full capability set.
- No DID method SDK is selected or included in the current M0 graph.
