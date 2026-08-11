# ADR-0019: Add application events only for concrete needs

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 3 and 13
- Implementation state: No event model in M0

## Context

Wallet capabilities may eventually need UI notifications, background sync,
audit history, or cross-module reactions. Introducing event sourcing or a
distributed event architecture before those flows exist would add speculative
contracts and persistence complexity.

## Proposed decision

Introduce explicit application events only when a concrete use case has more
than one justified consumer or needs durable state-transition evidence. Keep
events Oxid-owned and distinct from adapter wire messages. Do not adopt event
sourcing, a message broker, or distributed architecture without a separate
decision.

The synchronous M0 Create Wallet Profile flow returns a view directly and does
not publish an event.

## Consequences if accepted

- Early use cases remain simple and deterministic.
- Later background work can add narrowly scoped event ports.
- Event schemas require the same privacy and compatibility review as other
  public application types.
- This proposal does not authorize event infrastructure today.
