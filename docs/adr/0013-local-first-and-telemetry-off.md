# ADR-0013: Local-first operation with telemetry off

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 4, 5, 12, and 13
- Implementation state: Enforced for M0
- Amended by: ADR-0036, ADR-0037, ADR-0038, ADR-0039, ADR-0040, ADR-0041, ADR-0042, ADR-0043, ADR-0044, ADR-0045, ADR-0080

## Context

Wallet state includes identifiers, credentials, relationships, transaction
intent, and consent history. Mandatory hosted services or default telemetry
would create privacy, availability, and white-label constraints.

## Decision

Prefer local storage and local execution where practical. Oxid requires no
mandatory Oxid-hosted backend. Telemetry is disabled by default; adding any
telemetry requires explicit user opt-in, data minimization, a threat/privacy
review, and a new ADR.

Never log secrets, claims, private identifiers, signing payloads, or raw
external error bodies that may contain sensitive data. M0 has no telemetry and
stores only public profile metadata in process memory.

## Consequences

- Offline and degraded-network behavior must be designed explicitly.
- Sync and backup are opt-in capabilities with clear trust boundaries.
- Diagnostics must be safe and useful without collecting sensitive payloads.
- The in-memory M0 adapter demonstrates isolation, not durable local storage.
