# ADR-0071: Wrap mobile custody with device user presence

- Status: Accepted
- Date: 2026-08-17
- Blueprint source: Sections 3, 7, 12–13, and 16–18
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, especially `wallet-core/secret_storage` and `unlock`
- Tracking: issues #2, #29, and #30
- Implementation state: the Rust sealed-vault adapter, iOS Keychain and Android Keystore backends, normal mobile fail-closed composition, opt-in standalone native-custody composition, adapter tests, iOS capability/fail-closed smoke, and Android explicit-authorization/distinct-process/stable-root smoke are implemented; physical-device recovery, lifecycle/resource evidence, mobile Compact proving, and production release review remain open
- Amended by: ADR-0081

## Context

The standalone development adapter can exercise all migrated wallet and SSI
flows, but its root and authorization session are process-local. It cannot be
selected by a production-facing mobile composition. The prototype encrypts
software keys in files and redb, but moving that storage verbatim would not
meet the blueprint's requirement for platform-backed protection and explicit
user presence.

Wallet, Midnight, DID, credential, and presentation application code already
depend on capability-specific opaque-key ports. The missing piece is therefore
an outgoing mobile adapter, not a new custody API in the UI or domain. It must
retain the existing multi-curve and HD behavior, persist across application
restart, fail closed when the device lacks a secure lock, and describe the
effective protection class truthfully.

Dioxus/Manganis 0.7.10 cannot reliably generate the desired Swift bridge for
multiple string arguments. The existing single repository-owned native plugin
also remains the only iOS framework that the selected bundler embeds reliably.

## Decision

Add two driven-adapter crates:

- `custody-software` owns only adapter-private Ed25519, P-256, secp256k1,
  Jubjub, and BIP32 operations extracted from the development custody adapter.
  It has no persistence, session, or application-facing custody policy.
- `storage-mobile` implements the existing wallet protection, opaque key,
  derived-secret, HD derivation, and Jubjub signing ports over one bounded,
  versioned sealed vault per profile. It validates profile binding, duplicate
  references, key count, algorithms, and serialized size before using a vault.

The Rust adapter serializes at most 512 KiB and zeroizes plaintext buffers. A
single bounded JSON request crosses the native bridge because of the selected
Manganis limitation. Native responses expose only stable status, effective
protection class, and—in an already authorized protected operation—the sealed
vault plaintext required by Rust. Application and UI layers still receive only
opaque key references, public keys, signatures, and safe status.

On iOS, store the vault as a device-only generic-password Keychain item using
`kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly` and a `userPresence` access
control. `LAContext` authorizes access for a bounded 30-second in-process
session and is invalidated on explicit lock or expiry. Report the effective
class as `operating_system`; this design does not claim a Secure Enclave key
for arbitrary wallet algorithms.

On Android, generate an AES-GCM key in Android Keystore with user
authentication required for a 30-second window. Request StrongBox where
available, fall back to the platform Keystore, and inspect `KeyInfo` before
reporting `hardware_backed`. Store only the version, authenticated protection
label, IV, and ciphertext in an atomic digest-named file below
`noBackupFilesDir`; bind ciphertext to the profile, protection label, and
custody-domain string as AEAD additional data. Initialization and unlock
require the system device-credential confirmation surface.

User presence is entered only from an explicit initialize, unlock, or protected
operation intent. Initial account rendering reads protection status first and,
when protection is uninitialized or locked, renders a public unavailable
placeholder without asking the account adapter to re-derive protected state.
Returning from a native authorization surface makes Settings re-read the
authoritative adapter status; page-local task completion is not the source of
truth across a mobile pause/resume transition.

Normal iOS and Android composition selects this adapter. Missing native
packaging, missing secure device lock, inconsistent key/ciphertext state,
denied authorization, malformed vault data, and expired sessions all fail
closed. Desktop and web production composition remain unavailable.

Add `standalone-native-custody` as a mutually exclusive mobile feature. It
combines production native custody with the same deterministic standalone
wallet/SSI adapters used for parity testing; it does not relabel simulated
settlement as live. Keep `standalone-development` as the default simulator and
emulator experience so the complete application remains testable on devices
whose simulated security capability is absent.

## Security and truth boundaries

- Device wrapping protects data at rest and gates access; the contained
  multi-curve wallet secrets are still software key material. Never describe
  the whole vault as non-exportable hardware custody.
- Plaintext necessarily crosses the selected native FFI boundary during an
  authorized operation. Keep that bridge private, bounded, payload-redacted,
  and zeroizing where the language permits; do not log requests or responses.
- A 30-second session is a bounded usability policy, not proof of continuous
  biometric presence. Every operation after expiry re-enters native
  authorization before loading protected bytes.
- Background rendering and public account-status reads must never cause a
  credential prompt. Only an explicit user action may enter native custody.
- iOS Simulator may report the passcode-bound Keychain policy unavailable.
  That is a valid fail-closed result, not permission to substitute development
  custody inside normal composition.
- Android automation may set a temporary PIN only on an explicitly selected
  disposable `emulator-*` with no existing credential. It must clear app data
  and the temporary PIN on every exit.
- Native-custody standalone settlement and identity endpoints remain simulated
  unless the corresponding live adapters are explicitly configured and
  truthfully reported.

## Consequences

- The same persisted protected root can drive Midnight accounts, DIDs,
  credentials, and presentation authorization after an application restart.
- Native security policy remains an outgoing adapter concern; application and
  domain crates do not import Keychain, Keystore, Swift, Kotlin, or storage
  formats.
- Android emulator evidence covers a real system credential prompt, opaque
  no-backup ciphertext, a proven distinct process restart, explicit
  reauthorization, and stable protected account derivation from an unchanged
  sealed record. iOS simulator evidence covers native capability
  detection and fail-closed behavior; physical-device evidence remains a
  release gate.
- This removes native wrapping as the blocker for mobile Compact proving, but
  does not solve the separately measured prover memory/packaging constraint in
  issue #30.

## Rejected alternatives

- Migrating the prototype file/redb vault unchanged would omit device-bound
  wrapping and user-presence policy.
- Storing one native item per curve key would duplicate HD/root lifecycle
  rules across platforms and does not support every required wallet algorithm.
- Reporting every Android Keystore key as hardware-backed would overstate
  emulator and software-backed implementations.
- Silently falling back to development custody would make normal composition
  unsafe and its status misleading.
- Putting native calls in application use cases or Dioxus components would
  invert the hexagonal dependency direction.
