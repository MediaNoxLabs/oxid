# ADR-0037: Manage standalone Midnight DIDs through opaque protected custody

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3–7, 9, 12–13, 16–18 and [issue #22](https://github.com/MediaNoxLabs/oxid/issues/22)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/did/`, `contracts/midnight-did/did.compact`, Dioxus DID operation builder, and headless wallet
- Supersedes: ADR-0036 statements that DID create, update, and deactivate are queued
- Amends: ADR-0007, ADR-0008, ADR-0011, ADR-0013, ADR-0017, ADR-0021, ADR-0023, ADR-0024, and ADR-0029
- Implementation state: complete development-only standalone lifecycle, Ed25519/P-256/Jubjub signing, headless flow, and mobile operation builder implemented; Compact-backed live writes, durable native custody, and recovery remain queued
- Amended by: ADR-0038, ADR-0040, ADR-0046

## Context

The prototype presents the complete Midnight DID operation vocabulary, but its
mutable behavior is coupled to Dioxus, proving code, and a JavaScript bridge.
One headless result also exposes `controllerSkHex`, which violates Oxid's
protected-key boundary. Its `DidService` is only a constructor shell, so moving
the file would preserve neither the behavior nor the intended architecture.

Oxid needs a useful standalone identity wallet before live Compact proving is
ready. The implementation must exercise real cryptographic key generation and
signing rather than manufacture UI-only records, while never claiming that a
process-local state transition is a deployed ledger mutation.

## Decision

`identity/application` owns a capability-specific `DidLifecyclePort` and
create, update, deactivate, and DID signing use cases. The port accepts Oxid DID
domain objects and opaque intent data; it has no dependency on wallet-domain
types or a cryptography package. Every document mutation, signing operation,
and deactivation requires a bounded, human-readable confirmation supplied by
the incoming adapter.

`adapters/did-midnight::StandaloneDidLifecycle` is selected only by explicit
development compositions. It delegates key generation and signing to the
existing `WalletKeyOperationPort`, retaining only opaque key references in a
profile-and-DID-scoped process-local map. It creates an undeployed DID with an
Ed25519 authentication method, P-256 assertion method, and ADR-0047 Jubjub
holder-presentation assertion method, derives the 64-hex identifier from a
domain-separated SHA-256 digest of public inputs, and emits a fully validated
DID document. Private key bytes never cross the custody port.

The adapter implements the useful prototype state transitions:

- add and remove `alsoKnownAs` entries;
- add, rotate, and remove Ed25519, P-256, or Jubjub verification methods;
- add and remove authentication, assertion, capability-invocation, and
  capability-delegation relationships;
- add, update, and remove URI-backed services;
- sign a bounded payload using an owned document method;
- irreversibly mark the standalone record deactivated.

Every update reconstructs the complete dependency-free DID domain object, so
duplicates, dangling references, incompatible relationships, invalid
coordinates, and oversized input fail before public persistence. Removing or
rotating a method removes the DID-to-key association but deliberately does not
delete the protected key: if public persistence subsequently fails, Oxid must
prefer an unreachable retained key over a public document pointing at a key
that was destroyed. Explicit generic key deletion remains a separately
confirmed wallet operation.

The profile-scoped repository remains the source of truth for the public
document. It persists no custody reference. Development custody and the
DID-to-key association reset on process restart; restored public records remain
inspectable, resolvable from inventory, and forgettable, but mutation or
signing returns `NotManaged`. This is an intentional, visible standalone
constraint rather than recovery behavior.

Headless v1 exposes `did.create`, `did.update`, `did.sign`, and
`did.deactivate` alongside inventory and resolution. Dioxus exposes the same
use cases through a managed-DID operation builder. Both derive profile scope
from the selected wallet and return public documents/signatures only. Normal
production composition retains `UnavailableDidLifecycle`. Non-`undeployed`
creation and all live writes fail closed until a reviewed adapter proves,
submits, and reconciles the official Compact circuits.

## Consequences

- Standalone tests exercise actual protected Ed25519/P-256/Jubjub keys and signatures
  without exposing a seed, private scalar, or custody reference through DID
  APIs.
- The mobile and headless adapters share one lifecycle implementation and the
  complete prototype operation vocabulary instead of duplicating mutations in
  presentation code.
- Public DID inventory survives restart while honestly refusing managed
  operations after development custody resets.
- Jubjub assertion keys are implemented for standalone lifecycle and issuance;
  live `setSchnorrJubjubVerificationMethod` circuits remain a visible parity
  gap.
- Live `setAlsoKnownAs`, `setVerificationMethod`, relationship, service,
  deactivate, authorization, proving, submission, and finality handling remain
  a separate adapter slice governed by ADR-0015, ADR-0017, and ADR-0028.
