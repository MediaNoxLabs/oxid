# ADR-0068: Persist the standalone Passport Vault ledger

- Status: Accepted
- Date: 2026-08-15
- Blueprint source: Sections 3–7, 12–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/vault.rs` and headless vault verbs
- Tracking: issues #2 and #31
- Supersedes: ADR-0051's process-local-only standalone repository choice
- Implementation state: bounded owner-private JSON persistence, environment-aware composition, headless three-process conformance, truthful mobile copy, and iOS/Android restart assertions are implemented

## Context

ADR-0051 deliberately made the first standalone Passport Vault repository
process-local. That was sufficient to prove multi-lock accounting, exact
consent, Digital Passport policy checks, and per-lock credential-root replay
protection without pretending that local state was a deployed contract.

The mobile wallet now restores public profiles, encrypted credentials, public
DID documents, and public submission outcomes. Clearing the standalone vault
on every process restart is no longer useful parity: it loses the local lock
inventory and, more importantly, loses the consumed-credential set that makes
the conformance flow reject a repeated claim.

This state is not canonical Midnight state. Persisting it must not make it an
authority for native contract preparation, contract discovery, finality, or
settlement. It also contains correlating one-way credential fingerprints and
public policy material, so it does not belong in the public profile document.

## Decision

Add a separate `JsonPassportVaultRepository` outgoing adapter for the
standalone product ledger. The application continues to depend only on
`PassportVaultRepository`; persistence details remain in
`adapters/passport-vault` and composition.

The domain exposes an adapter-neutral complete snapshot and a single
`PassportVaultState::restore` gate. Restore revalidates:

- contiguous lock identifiers and the next identifier;
- at most 4,096 locks and 16,384 consumed claims;
- valid actors and policies;
- per-lock released amounts not exceeding deposits;
- checked aggregate deposited/released totals;
- unique consumed `(lock, credential fingerprint)` pairs that reference an
  existing lock; and
- exact agreement between the consumed set and claim count.

The version-1 file uses decimal strings for every `u64`/`u128` and lowercase
fixed-width hex for 32-byte policy/fingerprint fields. The complete document is
limited to 8 MiB and rejects unknown fields, malformed values, duplicate or
inconsistent records, non-files, and unsupported versions.

The adapter accepts only a normalized absolute file path, rejects direct file
or parent-directory symlinks, and requires owner-private permissions. On the
supported Unix targets it creates the private directory as mode `0700`, writes
a new mode-`0600` file, fsyncs it, atomically replaces the target, and fsyncs
the parent directory. Like the existing profile repositories, one process owns
the file; cross-process concurrent writers are unsupported.

Native standalone/headless composition uses
`OXID_PASSPORT_VAULT_STORE_PATH` when explicitly supplied. Otherwise it places
`private/passport-vault.json` beside the resolved public profile store. An
invalid explicit path fails startup without echoing the path. Native production
composition remains unavailable, and WASM/in-memory test composition remains
process-local.

Capability discovery reports `owner_private_atomic_file` separately from
Passport Vault contract-call mode. Dioxus states that the conformance ledger
survives restart while also stating that no on-chain transaction was submitted.
Headless and both Tier-1 mobile harnesses verify restart restoration.

## Security and truth boundaries

- The file may contain creator profile identifiers, public lock policy,
  accounting, verifier challenge, and one-way credential fingerprints. It must
  never contain a credential identifier, signed credential, detached proof,
  private opening, holder DID/key reference, scalar, nonce, witness, proof,
  signature, or serialized transaction.
- The repository is local conformance state only. It cannot implement
  `PassportVaultContractStateSourcePort`, satisfy
  `canonical_finalized_replay`, authorize a native call, or populate live
  submission history.
- Its source remains `standalone`; its persistence label is orthogonal to
  `deterministic_simulation` and `native_settlement`.
- Persisted consumed fingerprints preserve local replay rejection, but they are
  not evidence that a credential root is unspent on Midnight.
- Owner-private filesystem permissions are not platform-backed encryption or
  production credential custody.

## Consequences

- Standalone create/deposit/claim/withdraw accounting and claim replay survive
  an app or headless process restart.
- Corrupted or permissively stored data fails closed instead of silently
  resetting the ledger and reopening a consumed claim.
- Mobile restart coverage now includes the product ledger, not only public
  profile/submission metadata and encrypted credentials.
- Real-node fixtures, authenticated replay caching, device resource baselines,
  and native platform custody remain issue #31 follow-ups.

## Rejected alternatives

- Persisting the application view would omit the consumed fingerprint set and
  make replay protection non-durable.
- Adding vault fields to the public profile JSON would couple product state to
  wallet lifecycle and expose correlating metadata under the wrong boundary.
- Encrypting this file with the development credential key would conflate two
  stores and still would not create platform-backed custody.
- Reusing the local file as a native contract-state cache would turn
  unauthenticated standalone data into chain authority.
- Silently falling back to an empty in-memory repository after corruption
  would erase replay evidence and fail open.
