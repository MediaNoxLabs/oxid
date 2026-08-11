# ADR-0021: Migrate the prototype in vertical slices

- Status: Accepted
- Date: 2026-08-11
- Source: Blueprint Sections 14, 17, 18, and 19 plus prototype review
- Implementation state: M0 profile slice implemented

## Context

The latest wallet prototype is embedded under `mobile-bench/` in the
`midnight-ledger` workspace. It combines valuable wallet, Midnight, SSI,
proving, storage, platform, and UI behavior with ledger-relative dependencies,
demo material, generated artifacts, and environment-specific hosts.

Copying the directory intact would preserve those couplings and contradict the
hexagonal boundaries. Rewriting every capability before validating the target
architecture would create a large, unverifiable migration.

## Decision

Treat `midnight-ledger` branch `feat/mobile-prototype` at immutable commit
`074b1a4bccbfee1740ee188374b606a022ecef42` as the reviewed migration baseline.
Move behavior in bounded vertical slices, reimplementing it with Oxid-owned
types and focused ports. Preserve provenance and useful behavioral lessons, but
do not copy ledger-relative paths, secrets, pre-production keys, generated
proofs, vendored JS, captured diagnostics, or platform signing state.

M0 migrates exactly Create Wallet Profile through foundation, domain,
application, platform ports, in-memory/system adapters, Dioxus, and composition.
The remaining source inventory and milestone destinations are maintained in
`docs/migration/midnight-ledger-prototype.md`.

## Consequences

- Each migrated capability is independently reviewable and testable.
- Prototype feature parity arrives incrementally rather than through a bulk
  directory move.
- Upstream behavior may need reimplementation instead of history-preserving
  file copies.
- Every later source refresh must record a new immutable baseline and repeat
  the exclusion/security review.
