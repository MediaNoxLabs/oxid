# ADR-0011: Secure key operations stay behind ports

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 3, 7, 12, and 13
- Implementation state: Policy enforced; protected development derivation and
- Amended by: ADR-0037, ADR-0038, ADR-0039, ADR-0040, ADR-0041, ADR-0042, ADR-0045, ADR-0046
  signing implemented by #5/#8, native custody still required

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

The application may pass a bounded, validated HD path to a derivation port, but
the root and child private bytes stay inside custody. Chain adapters receive
only public-key metadata and an opaque child-key reference.

## Consequences

- Platform custody can change without redefining wallet use cases.
- Tests may use explicit development adapters and public conformance inputs,
  but cannot expose their root or child private bytes through incoming APIs.
- Recovery/export flows need separate, strongly reviewed capabilities.
- M1 cannot be production-capable until native adapters satisfy the accepted
  platform-custody policy in ADR-0017.
