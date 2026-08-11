# ADR-0017: Platform-backed secret storage

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 7, 12, and 17
- Implementation state: Must be resolved before production custody in M1

## Context

Android and iOS provide hardware- or OS-backed facilities with different
availability, authentication, migration, and backup behavior. Portable
encrypted databases can complement those facilities but cannot automatically
replace protected key operations.

## Proposed decision

Use platform-backed protection for production mobile keys and recovery
material where practical. Evaluate a portable encrypted store such as Askar
only for the data and KMS responsibilities its threat model actually covers.
Expose storage and cryptographic operations through separate focused ports and
opaque references.

Document device-lock, biometric, export, backup, restore, deletion, and
platform-unavailable behavior before choosing adapters.

## Consequences if accepted

- Android and iOS may need different adapter implementations.
- A development store cannot be presented as secure production storage.
- Backup and recovery require explicit security/product decisions.
- The current in-memory repository stores public profile metadata only and is
  not evidence for this proposal.
