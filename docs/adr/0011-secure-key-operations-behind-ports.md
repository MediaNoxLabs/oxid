# ADR-0011: Secure key operations stay behind ports

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3, 7, 12, and 13
- Implementation state: Policy enforced; custody adapters begin in M1

## Context

Private keys, seeds, recovery material, and authorization operations are the
wallet's highest-value assets. Passing raw key bytes through UI or ordinary
application DTOs widens exposure and makes platform-backed protection
impossible to preserve.

## Decision

Application and UI layers use opaque key references and focused key-operation
ports. Generation, derivation, signing, agreement, import/export, biometric
authorization, and deletion occur in protected adapters. Ports do not return
raw private keys as their normal result.

Human-readable signing and disclosure confirmation must precede sensitive
operations. Export and backup require explicit authorization and re-authentication.

M0 creates only public profile metadata and deliberately contains no key or
seed type.

## Consequences

- Platform custody can change without redefining wallet use cases.
- Tests use fake references and operation outcomes rather than production key
  material.
- Recovery/export flows need separate, strongly reviewed capabilities.
- M1 cannot be production-capable until the proposed secret-storage decision
  in ADR-0017 is resolved.
