# ADR-0002: Dioxus is an incoming adapter

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3 and 6
- Implementation state: Implemented for the M0 UI and composition root

## Context

Oxid needs a shared Rust UI across mobile, desktop, and web without allowing a
UI framework or WebView runtime to become the application architecture.

## Decision

Dioxus renders application state and invokes incoming use-case traits. It does
not call storage, chain, identity, credential, protocol, or platform adapters.
The composition root provides use-case implementations through Dioxus context.

The M0 shell uses Dioxus 0.7 and a desktop default for local validation while
retaining explicit mobile and web feature boundaries. Platform host projects
are introduced only when the first mobile capability needs them.

## Consequences

- UI code can be replaced or supplemented by CLI, QR, deep-link, and test
  adapters without moving business rules.
- Dioxus upgrades cannot change core public types.
- Platform APIs and long-running work must enter through ports and must not run
  directly in component event handlers.
