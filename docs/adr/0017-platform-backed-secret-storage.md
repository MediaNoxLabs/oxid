# ADR-0017: Separate platform custody, secret blobs, and user authorization

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 7, 12, 15, and 17
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Focused ports plus process-local generated roots,
  protected BIP32/BIP340 derivation, and headless signing are implemented for
  development; native mobile adapters remain required

## Context

The reviewed prototype combines passphrase verification, an encrypted key
database, key generation/import/derivation, public-key lookup, signing,
verification, deletion, and backup concerns in broad storage interfaces. Its
useful behaviors include a boot-locked session, opaque key handles, operation
lockout, multi-curve metadata, and keeping decrypted key bytes out of signing
results. Its prototype shortcuts cannot become Oxid defaults: the UI pre-fills
the passphrase `midnight`, ordinary inputs carry raw private bytes and a seed
hex string, wallet information exposes `seed_hex`, and the application process
owns both the passphrase and decrypted secrets.

The mobile platforms do not offer one interchangeable facility for every
secret or algorithm:

- [Android Keystore](https://developer.android.com/privacy-and-security/keystore)
  can keep generated keys non-exportable, constrain their purposes, require
  user authentication, and report their security level. StrongBox is optional
  and supports a narrower algorithm set, so it cannot be assumed.
- [Apple Secure Enclave](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave)
  provides non-exportable operations only for generated NIST P-256 keys. It
  cannot import an existing recovery key or directly hold Ed25519, Jubjub, or
  arbitrary seed material.
- [Apple Keychain access control](https://developer.apple.com/documentation/security/restricting-keychain-item-accessibility)
  can require a device passcode and user presence and can make an item
  device-only. Removing the passcode can make the most restrictive items
  unavailable.
- Android app-private files are normally included in automatic backup. Secret
  ciphertext that is bound to a device key belongs in
  [`getNoBackupFilesDir()`](https://developer.android.com/reference/android/content/Context#getNoBackupFilesDir())
  or an explicit backup exclusion.

Askar is a maintained encrypted record store and software KMS, but a portable
database cannot turn unsupported keys into platform-non-exportable keys or
enforce native user presence. It may be evaluated later as an encrypted-record
adapter, not as a substitute for platform custody.

## Decision

Treat these as separate capabilities with focused ports:

1. **Wallet protection/session** reports whether a profile is uninitialized,
   locked, unlocked, or unavailable and reports the effective protection class.
2. **Key operations** generate, list, inspect, sign with, and delete keys by an
   Oxid-owned opaque reference. Normal ports never return raw private keys,
   seeds, recovery material, wrapping keys, or passphrases.
3. **Secret blob storage** protects recovery material and algorithms that
   cannot live natively in secure hardware. Ciphertext storage and key
   operations remain distinct even when one adapter implements both.
4. **User authorization** is requested at the last responsible moment through
   platform UI. Sensitive signing, deletion, export, backup, and recovery
   operations require an explicit human-readable intent and confirmation;
   export and backup additionally require re-authentication.

Production Android adapters use Android Keystore keys with immutable purpose
authorizations and per-operation or bounded-duration user authentication.
Hardware/StrongBox protection is requested where supported and reported from
the actual generated key; unsupported StrongBox generation falls back only to
an explicitly allowed lower protection class. Device-bound ciphertext is kept
out of automatic backup.

Production Apple adapters use Keychain device-only access classes and
`SecAccessControl` user-presence policy. Secure Enclave is used only for
supported, newly generated P-256 signing or agreement keys. Ed25519, Jubjub,
recovery, and seed material use Keychain-protected wrapping/blob designs whose
effective protection is reported honestly.

If the required device lock, user authentication, key algorithm, or protection
class is unavailable, setup and sensitive operations fail closed with a safe
capability error. Application-level attempt throttling may supplement native
controls but is not the primary authorization boundary. Production locking
clears process-held authorization and any reloadable decrypted material. The
ephemeral development adapter gates access but retains its process-local keys
so unlock sequencing can be tested; that limitation is part of its explicit
non-production status.

Backup and restore are explicit, portable, user-authorized encrypted packages;
they are never an accidental OS backup of device-bound ciphertext. Recovery
import uses a dedicated one-shot protected input boundary and is not exposed by
the ordinary headless protocol. Deletion removes protected records and key
handles; hardware deletion guarantees are documented per adapter.

A process-local development adapter may exercise lifecycle and cryptographic
flows in tests and the standalone headless wallet. It must identify itself as
`development_only`, keep no durable recovery material, zeroize supported
software key types on drop, and never be selected by production mobile
composition. Production composition reports custody as unavailable until a
native adapter is present.

For development account conformance, `initialize` generates a random
process-local root inside the adapter. A focused derivation port accepts only a
validated HD path, public label, algorithm, and purpose. It retains the child
signing key and returns public metadata plus an opaque reference; no ordinary
DTO accepts or returns the root, child scalar, mnemonic, or recovery phrase.
Repeated derivation of the same path and metadata is idempotent, while path or
label conflicts fail closed. Locking blocks derivation and signing. The root,
raw scalar buffers, and supported signing-key types use zeroization where the
selected libraries expose it, but the adapter remains process-local
development infrastructure rather than a durable software vault.

## Consequences

- Android and Apple have different adapter implementations and capability
  matrices behind the same Oxid-owned ports.
- A wallet can use hardware P-256 keys alongside wrapped software keys without
  misrepresenting both as equally protected.
- Device migration requires an explicit recovery or backup flow; device-only
  keychain/keystore records are not silently portable.
- Headless flows can test application sequencing, confirmation, opaque
  references, canonical Midnight HD derivation, and signatures without
  creating a production-security claim.
- The prototype's hard-coded passphrase, raw key/seed DTOs, broad store trait,
  and implicit file backup are permanent migration exclusions.
- Adding Askar or another encrypted store requires a dependency review and an
  adapter-specific threat model; it does not supersede this decision.
