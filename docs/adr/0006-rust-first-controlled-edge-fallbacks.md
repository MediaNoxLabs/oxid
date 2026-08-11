# ADR-0006: Prefer Rust and isolate non-Rust fallbacks

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3 and 4
- Implementation state: Enforced as policy; M0 has no foreign runtime fallback

## Context

Rust provides a shared, auditable implementation across Oxid's target
platforms. Some interoperability capabilities may nevertheless exist only as
mature JavaScript, WASM, or native platform libraries.

## Decision

Prefer maintained Rust implementations. Use JavaScript, externally supplied
WASM, or platform-native code only behind a focused adapter after documenting
why an adequate Rust implementation is unavailable. Such code must not define
core types or bypass application use cases.

Dioxus web output is a Rust target and does not itself justify importing
third-party JavaScript wallet logic. The prototype's vendored JS and WebView
bridges are deliberately excluded from M0.

## Consequences

- Most business and security logic remains shared and type checked.
- Necessary interoperability fallbacks have explicit replacement boundaries.
- Foreign-runtime bridges require dependency review, input validation, and
  platform-specific integration tests.
- Convenience alone is not sufficient justification for a fallback.
