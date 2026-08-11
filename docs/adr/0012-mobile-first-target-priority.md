# ADR-0012: Android and iOS are Tier 1

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 1, 4, 12, 13, and 16
- Implementation state: Feature boundaries compile; native hosts are deferred

## Context

Wallet custody, QR flows, deep links, biometrics, secure storage, and everyday
consent happen primarily on phones. Desktop and web remain useful development,
accessibility, and secondary delivery targets.

## Decision

Treat Android and iOS as Tier-1 product targets and desktop/web as Tier 2.
User-facing capabilities are not complete until they receive an appropriate
mobile smoke test. Platform APIs remain behind ports rather than leaking into
shared components.

M0 uses a desktop default for fast local execution while preserving explicit
Dioxus `mobile` and `web` features. Generated Android/iOS projects arrive with
the first capability that needs native hosting; empty hosts are not scaffolded.

## Consequences

- Dependency reviews must assess both Android and iOS support.
- Desktop success alone cannot declare a user-facing milestone complete.
- Native build, signing, and device automation become required in later slices.
- The current M0 proves compilation and shared composition, not mobile release
  readiness.
