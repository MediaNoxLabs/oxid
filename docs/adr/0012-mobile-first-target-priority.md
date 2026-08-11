# ADR-0012: Android and iOS are Tier 1

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 1, 4, 12, 13, and 16
- Implementation state: Shared shell smoke-tested on iOS; Android host deferred

## Context

Wallet custody, QR flows, deep links, biometrics, secure storage, and everyday
consent happen primarily on phones. Desktop and web remain useful development,
accessibility, and secondary delivery targets.

## Decision

Treat Android and iOS as Tier-1 product targets and desktop/web as Tier 2.
User-facing capabilities are not complete until they receive an appropriate
mobile smoke test. Platform APIs remain behind ports rather than leaking into
shared components.

Oxid uses a desktop default for fast local execution while preserving explicit
Dioxus `mobile` and `web` features. The shared shell is built and launched with
Dioxus's generated iOS simulator bundle; generated output remains uncommitted.
Custom Android/iOS hosts arrive only with capabilities that need native bridges.

## Consequences

- Dependency reviews must assess both Android and iOS support.
- Desktop success alone cannot declare a user-facing milestone complete.
- Native build, signing, and device automation become required in later slices.
- The iOS shell smoke proves rendering and shared composition, not custody,
  native-bridge parity, signing, distribution, or mobile release readiness.
