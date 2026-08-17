# ADR-0074: Package portable custody for one-shot recovery

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3, 7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/store/backup.rs`, `session_persist.rs`, and the Dioxus `WalletBackupCard`
- Tracking: issues #2 and #33
- Implementation state: the application boundary, authenticated package codec, and development/mobile custody export and empty-profile recovery are implemented; user-selected native file transfer, Dioxus Settings UX, public profile/DID/credential associations, and Tier-1 export/restore evidence remain issue #33 work

## Context

The prototype has a useful export/import journey, but its implementation is not
a safe portable recovery boundary. It serializes wallet-seed and DID-controller
rows into a JSON wrapper while retaining their existing live-store ciphertext,
reuses the live unlock passphrase, accepts a user-entered path, can overwrite
existing records, and continues past some malformed or unknown-network rows.
Its iOS session paths also place wallet database and backup files under
`Documents`, where implicit OS backup is possible. Raw controller-secret reveal
and copy actions exist elsewhere in the same prototype.

ADR-0071 replaced that live secret store with a profile-bound, device-only
sealed vault. Copying that vault's native ciphertext cannot be portable because
its wrapping key is intentionally non-migratable. Conversely, exposing its
plaintext to application DTOs, Dioxus, headless JSON, arbitrary paths, logs, or
the clipboard would destroy the custody boundary.

This decision covers the first recovery slice: the root seed and each generated
protected key that cannot be re-derived from it, plus the public metadata and HD
paths needed to reconstruct exact opaque key references. Profile metadata, DID
records, credential records, and their public associations remain separate
stores and are not yet claimed as recovered.

## Decision

The wallet application owns four opaque boundary types: a validated recovery
secret, bounded encrypted package, safe recovery summary, and
`WalletPortableBackupPort`. Incoming use cases require exact operation titles
and summaries in addition to an explicit confirmation. The recovery secret has
12–128 characters, at most 256 UTF-8 bytes, no controls, and no surrounding
whitespace. Its `Debug` output is always redacted. The encrypted package is at
most 1 MiB and its `Debug` output reveals only length.

The shared outgoing adapter uses this exact binary envelope:

- eight-byte magic `OXIDBAK1` and format version 1;
- KDF identifier 1: Argon2id v1.3 with 19,456 KiB memory, two iterations, one
  lane, a random 16-byte salt, and a 32-byte output;
- AEAD identifier 1: XChaCha20-Poly1305 with a random 24-byte nonce;
- the exact KDF/AEAD identifiers, parameters, salt, nonce, and ciphertext length
  as authenticated associated data; and
- strict `deny_unknown_fields` JSON inside the ciphertext only.

All header fields and the exact package length are validated before invoking
Argon2id. Future versions, unknown algorithms, changed KDF parameters,
truncation, trailing data, malformed plaintext, more than 256 keys, duplicate
references/labels/paths, invalid key metadata, and invalid HD paths fail
closed. Wrong recovery secrets and ciphertext/tag tampering produce the same
authentication failure. The authenticated profile identifier must equal the
requested target profile.

The package contains exactly one custody root seed and one entry per retained
key. A generated key entry carries the protected 32-byte software secret; a
derived entry carries only its public BIP32 path. Each entry also authenticates
its opaque reference, label, algorithm, purpose, public key, and creation time.
Recovery reconstructs every public key from the recovered secret or root/path
and rejects any mismatch before changing destination state. Secret-bearing
adapter structures have redacted formatting and zeroize transient roots,
generated keys, plaintext, and derived KDF output on normal drop paths.

Development custody may export only while unlocked. It is a conformance
adapter and cannot claim platform user presence. Mobile export always calls the
native `unlock` operation with a dedicated backup-export reason, even when a
30-second session is already active. Mobile recovery first requires an
uninitialized destination, authenticates and validates the whole package, then
uses native vault initialization as the fresh platform-authorization and atomic
creation boundary. Development recovery similarly inserts only after full
validation and refuses an existing profile. Neither adapter performs a
destructive overwrite or partial import.

The application and custody ports do not choose or open filesystem paths. A
later native adapter must use the OS document exporter/picker, reject symlinks
and non-regular files, and create one explicitly user-selected file outside the
device-bound vault. Recovery is intentionally absent from `oxid.headless.v1`;
headless may expose capability metadata and adapter conformance tests only.

## Security and privacy consequences

- Native vault ciphertext remains device-bound and excluded from portable
  backup. The portable copy receives independent password-derived encryption.
- Package metadata needed to select a KDF is visible but authenticated. Profile,
  key, path, and public-key metadata remain encrypted.
- Authentication precedes all semantic use. Public-key reconstruction prevents
  a package from pairing attacker-chosen metadata with different secret bytes.
- Export and recovery require explicit intent; native export forces fresh user
  presence and native recovery uses fresh authorized initialization.
- No raw seed, key, recovery secret, or plaintext package enters ordinary UI,
  headless, clipboard, logging, or error DTOs.
- Fixed Argon2id parameters make resource use reviewable. Raising them requires
  a new version and migration decision, not accepting attacker-selected values.

## Consequences and remaining work

- `portable_backup_supported` now describes custody package support in the
  development and mobile adapters. It does not claim that a file-transfer UI is
  already available.
- The current recovery slice restores exact Midnight account roots, generated
  DID/credential-holder keys, derived paths, and opaque references. It does not
  yet restore the public profile record, DID documents, encrypted credentials,
  or association records required for full issue #33 acceptance.
- The next slice must stage all public/private store records, detect conflicts
  across every store, and commit or roll back the complete recovery as one
  operation. It must then add the native file picker/exporter and Dioxus
  Settings warning/confirmation flow.
- iOS and Android tests must still cover OS document cancellation, app restart,
  fresh installation recovery, no-backup placement of device ciphertext, and
  physical-device resource behavior. Simulator/host unit tests are not that
  evidence.

## Rejected alternatives

- Reusing live-store ciphertext and password was rejected because device-bound
  ciphertext is not portable and live unlock credentials are not recovery-key
  separation.
- Exporting raw seeds or controller secrets, even behind reveal/copy UX, was
  rejected because it widens secret exposure to UI and clipboard surfaces.
- Allowing arbitrary caller paths was rejected in favor of future native OS
  document APIs with explicit user selection.
- Importing into an initialized profile or merging record-by-record was rejected
  because conflict and partial-failure behavior can silently replace authority.
- Adding headless export/recovery methods was rejected because NDJSON is not an
  acceptable transport for recovery secrets or encrypted backup payloads.
