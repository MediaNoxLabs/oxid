# ADR-0025: Persist public wallet profile metadata separately from secrets

- Status: Accepted
- Date: 2026-08-11
- Source: Blueprint Sections 3, 7, 12, 13, and 17 plus [issue #1](https://github.com/MediaNoxLabs/oxid/issues/1)
- Implementation state: Version 1 JSON profile store, active selection, mobile onboarding, and headless profile lifecycle implemented

## Context

The migrated application must restore wallet profiles across launches and let a
user choose an active profile. The M0 in-memory repository cannot provide that
behavior. A profile currently contains only an Oxid identifier, a display label,
and a creation timestamp; the selected profile identifier is application state.
None of those fields grants custody or proves identity.

The reviewed prototype stores several kinds of wallet state together, including
material that is inappropriate for an ordinary file. Reusing that design would
blur the boundary established by ADR-0011 and prejudge the still-proposed
platform secret-storage decision in ADR-0017.

## Decision

Keep the `WalletProfileRepository` port in the wallet application boundary and
extend it with list, select, and active-profile operations. Implement a
replaceable JSON adapter for public profile metadata only. The document has an
explicit schema version and contains exactly profile identifiers, display names,
creation timestamps, and an optional active identifier.

On Apple and desktop hosts, the adapter uses the platform-conventional local
application data directory. On Android it asks the runtime application
`Context` for the durable internal files directory; it does not persist under
the evictable cache directory. If a durable default path cannot be resolved,
the repository fails closed. `OXID_PROFILE_STORE_PATH` provides an explicit path
for isolated development and headless test runs. Writes validate domain
invariants, reject unknown fields and schema versions, cap document size and
profile count, write through a temporary file, sync it, and replace the prior
document. Unix files are created with owner-only permissions.

This adapter must never contain seeds, raw private keys, recovery phrases,
credentials, credential claims, signing payloads, or authentication tokens.
Those values require capability-specific protected storage and key-operation
ports after ADR-0017 is resolved. The in-memory adapter remains available only
for deterministic tests and development composition.

The file adapter serializes access within one repository instance. The current
mobile application is a single process; concurrent multi-process writers are not
a supported deployment mode. Headless automation that may overlap another Oxid
process must use a distinct `OXID_PROFILE_STORE_PATH`.

## Consequences

- First launch, profile creation, selection, and restore can complete without a
  chain, DID, credential, or secret-storage adapter.
- UI and headless drivers share the same focused application use cases and do
  not own persistence.
- An integration test drives create/select/restore through two actual
  `oxid-headless` processes against one isolated store.
- Corrupt, oversized, or future-schema documents fail closed as storage
  unavailable rather than being partially accepted.
- A later database or platform-native metadata adapter can replace JSON without
  changing the domain, use cases, UI contract, or headless protocol.
- Profile labels remain local metadata, not a security boundary. Production
  custody still cannot begin until protected key operations are implemented.
