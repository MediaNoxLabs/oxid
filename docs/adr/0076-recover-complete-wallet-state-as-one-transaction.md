# ADR-0076: Recover complete wallet state as one transaction

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/store/backup.rs`, `mobile-bench/wallet-core/src/service/backup_service.rs`, and `mobile-bench/dioxus-wallet/src/session_persist.rs`
- Tracking: issues #2 and #33
- Implementation state: public Midnight account associations, strict profile/DID/credential snapshot codecs, exact restored-DID key rebinding, the authenticated complete-wallet archive, the journaled all-store coordinator, fresh-install Dioxus recovery, complete Settings export, and an all-store standalone composition round trip are implemented; ADR-0078 writes the stronger version-3 envelope while retaining version-2 reads; complete iOS/Android document-round-trip and physical-device resource evidence remain issue #33 work

## Context

ADRs 0074 and 0075 deliberately recover only protected custody into an empty,
already-known profile. That proves the cryptographic package and native document
authority, but it does not reconstruct a wallet. A useful recovery must also
restore the selected public profile, managed DID documents, credential records
including their original signed bytes and format-private holder material, and
the public coordinates that reconnect deterministic Midnight accounts.

The prototype copies live encrypted rows and controller-secret envelopes, then
overwrites conflicts. It does not define a crash boundary, fresh-install profile
selection, strict cross-store validation, or rollback. Oxid cannot import that
storage topology because repositories are separate hexagonal adapters and
native custody initialization is a one-shot, user-authorized operation.

Two process-local associations also made a custody-only restart appear healthier
than it was. The Midnight adapter forgot selected networks and account/address
indices. The standalone DID lifecycle forgot which opaque custody reference
controlled each public verification method. Restoring the root and keys without
repairing those associations could show public records that no longer sign or
derive the same account.

## Decision

Oxid treats a complete wallet backup as one profile-scoped authenticated archive,
not as copies of repository files. The application continues to expose only the
opaque bounded encrypted `PortableWalletBackup`; recovery secrets and decrypted
sections remain below incoming adapters. `oxid.headless.v1` continues to expose
no backup or recovery method.

The archive contains four strictly versioned sections:

1. public profile metadata plus selected network and per-network account/address
   derivation indices;
2. validated public DID records for that profile;
3. complete credential domain records, including original signed bytes, detached
   proof, and format-private holder material;
4. the portable custody root and exact generated/derived key inventory.

Addresses, custody references, endpoints, balances, transaction history,
submission journals, sync checkpoints, proof artifacts, and native sealed-vault
ciphertext are not archive sections. Every section is decoded with the same
bounds and domain validation as its owning repository. Profile identifiers must
agree across all sections. Duplicate identifiers, unsupported versions, trailing
bytes, inconsistent DID methods, malformed credential material, existing target
records, and initialized custody fail before mutation.

The complete-wallet `OXIDBAK1` plaintext framing carries the three repository
sections beside the custody record under one Argon2id/XChaCha20-Poly1305
operation and one authenticated header. ADR-0078 writes version 3 with the
stronger complete-wallet KDF policy while retaining exact version-2 reads.
Version 1 remains readable as custody-only input but is not relabelled a
complete backup. The application and native document limits are raised only to
the reviewed complete-store bound; each inner section retains its smaller
independent limit. Decrypted aggregate and credential buffers are zeroizing.

Recovery uses a prepared transaction owned by the outgoing backup adapter:

1. authenticate and completely validate the archive;
2. prove that public repositories and native custody have no target conflict;
3. write an owner-private, symlink-resistant recovery journal describing only
   identifiers, prior active-profile selection, counts, and phase;
4. stage profile, association, DID, and credential records;
5. initialize native custody last through its fresh user-authorization boundary;
6. verify the committed public/custody shape, select the recovered profile, and
   remove the journal.

Any failure before custody initialization removes records inserted by this
transaction and restores the prior active profile. A later retry with the same
authenticated archive reconciles the journal: an uninitialized vault rolls
staged records back; an initialized vault must match every journaled identifier
and completes the public commit; ambiguous or inconsistent state fails closed
for explicit repair. Recovery never merges, overwrites, or silently chooses
among profiles.

Fresh-install recovery therefore accepts no caller-selected destination profile.
The authenticated profile identifier becomes the destination only after archive
validation. Recovery launched from an existing profile may additionally require
that exact identifier, preventing accidental cross-profile import. Both paths
retain the exact confirmation and fresh native authorization requirements from
ADRs 0074 and 0075.

Public capability continuity is reconstructed rather than backed up as opaque
runtime state. The profile repository retains only selected network and bounded
account/address indices. The Midnight adapter deterministically derives and
rebinds the exact account after restart. The DID lifecycle scans current custody
descriptors and accepts a method only when its algorithm and public JWK match
exactly and uniquely; restored opaque key references are never written into the
public DID store or archive association section.

## Security and privacy consequences

- A public DID record alone is never evidence of control. Control exists only
  after exact unique public-key matching against authorized custody.
- Public account persistence cannot reveal an address, key handle, balance, or
  history; the coordinates are useful only with the protected profile root.
- One authenticated envelope prevents independently substituting a profile,
  credential, DID, association, or custody sub-backup.
- Credential records make the complete package materially larger and more
  sensitive even while encrypted. Native bridges must bound base64 expansion and
  peak memory, and physical-device measurements remain a release gate.
- Custody-last commit avoids publishing restored identity/credential state when
  native initialization is denied. The recovery journal handles process death at
  the remaining cross-store boundary without containing protected material.
- Existing version-1 custody files remain importable only through the explicit,
  truthfully labelled legacy custody-only Settings path.

## Rejected alternatives

- Copying repository files was rejected because it imports encryption keys,
  paths, schema accidents, and partial-write behavior rather than domain state.
- Backing up opaque DID key references was rejected because references are
  adapter-local and public persistence would expose unnecessary capability
  metadata.
- Re-deriving every account at index zero was rejected because it loses valid
  selected-network and non-zero account/address choices.
- Committing custody first was rejected because cancellation or public-store
  failure would leave an initialized but undiscoverable wallet.
- Best-effort merge/overwrite was rejected because duplicate identities and
  credentials require an explicit product conflict policy that issue #33 does
  not authorize.
- Multiple independently encrypted files were rejected because they permit
  mix-and-match restore, multiply KDF cost, and weaken the single user-selected
  document model.
