# ADR-0075: Transfer wallet backups through native document pickers

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3, 7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Dioxus `WalletBackupCard`, and `mobile-bench/wallet-core/src/store/backup.rs`
- Tracking: issues #2 and #33
- Implementation state: iOS and Android document transport plus custody-only Dioxus Settings UX are implemented; all-store recovery, Android picker interaction evidence, and physical-device evidence remain issue #33 work

## Context

ADR-0074 created an authenticated portable custody package but deliberately did
not let application or UI code choose filesystem paths. The prototype accepts
caller-entered paths and writes through ordinary filesystem APIs. That is not a
safe mobile boundary: it can address unintended locations, bypass platform
document consent, create implicit app-container copies, and blur cancellation
with storage failure.

Oxid also cannot put encrypted backups into the public-text share port. Although
the package is encrypted, it remains privacy-sensitive wallet material and must
not reach the clipboard, logs, app links, WebView messages, or headless NDJSON.

## Decision

The wallet application owns `PortableWalletBackupDocumentPort`. It transports
only the bounded `PortableWalletBackup` type and exposes asynchronous export and
import operations with stable payload-free cancellation, unavailable, timeout,
invalid-document, and failure categories. The application supplies the single
fixed suggested filename `oxid-wallet-custody.oxidbak`; callers never supply a
path. Non-mobile composition fails closed.

`oxid-adapter-backup-document-mobile` encodes only encrypted package bytes for
the repository-owned native bridge, accepts at most a 1 MiB decoded package and
a correspondingly bounded native response, polls at 100 ms for at most five
minutes, treats cancellation separately from failure, and clears transient
encrypted package buffers on drop/completion. It neither parses nor decrypts
custody plaintext. Because Dioxus/Manganis 0.7.10 embeds only the
primary native framework, these methods remain in the already reviewed mobile
plugin rather than adding a second Swift/Kotlin package.

iOS exports by creating one unique, complete-file-protected temporary file,
marking it excluded from backup, and presenting
`UIDocumentPickerViewController(forExporting:asCopy:)`. Completion or
cancellation removes the temporary directory. Import uses the system document
picker as a copy, requires exactly one regular non-symlink document with a
known non-zero size no greater than 1 MiB, then reads and returns only its
encrypted bytes.

Android exports with `ACTION_CREATE_DOCUMENT`, `CATEGORY_OPENABLE`, the fixed
filename, and `application/octet-stream`; it writes only after the user returns
a content URI. Import uses `ACTION_OPEN_DOCUMENT` and `ContentResolver`, rejects
known empty or oversized documents before reading, and enforces the 1 MiB limit
again while streaming when a provider reports an unknown length. Retained
encrypted byte arrays are cleared after export/import completion. The
repository-owned `MainActivity` forwards only the two reserved result codes.

Dioxus Settings exposes the ADR-0074 use cases only when the composed custody
adapter reports portable-backup support. Password fields use zeroizing Rust
state, export requires two matching values, and both operations require their
exact explicit confirmation. The UI says prominently that this migration slice
restores custody only and excludes profile metadata, DID documents,
credentials, associations, and transaction history. Recovery is enabled only
for an uninitialized active profile and never offers overwrite or merge.

## Security and privacy consequences

- No Rust, Dioxus, or headless caller can name an export or import path.
- The native bridge carries encrypted package bytes only. Recovery secrets and
  decrypted custody never cross it.
- iOS rejects symlinks and non-regular files directly. Android consumes only a
  user-selected openable content URI and never resolves it to a caller path.
- Cancellation is a normal no-change outcome. A malformed, empty, oversized,
  wrong-profile, wrong-secret, or tampered document still fails closed at the
  appropriate document/package boundary.
- The temporary iOS export copy is protected, excluded from implicit backup,
  and removed after the picker completes. The explicit user-selected copy is
  the only durable portable artifact created by this flow.

## Evidence and remaining work

An iPhone 17 Pro simulator exercised Settings rendering, import cancellation,
export to the Files document picker, process restart with development custody
cleared, import of the 862-byte package, restoration of one protected key, and
reconnection to the deterministic 5 NIGHT standalone account. The Android
adapter assembles, installs, and launches on `emulator-5554`; interactive
Android document cancellation/export/import and physical-device coverage remain
release evidence rather than inferred parity.

This does not complete issue #33. A later decision must stage public profile,
DID, credential, and association records with the custody package and commit or
roll back the complete destination atomically. It must also define fresh-install
profile selection when no matching public profile exists.

## Rejected alternatives

- Caller-supplied paths and generic filesystem ports were rejected because they
  bypass platform consent and broaden the write/read authority.
- Clipboard, generic share sheets, app links, WebView messages, and headless
  JSON were rejected as wallet-backup transports.
- Persisting a second app-container copy was rejected because only the explicit
  user-selected portable document should survive.
- Enabling recovery into initialized custody or silently merging rows was
  rejected in favor of the existing empty-destination atomic boundary.
