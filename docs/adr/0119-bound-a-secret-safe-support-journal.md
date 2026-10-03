# ADR-0119: Bound a secret-safe support journal

- Status: Accepted
- Date: 2026-10-03
- Issue: [#1026](https://github.com/MediaNoxLabs/oxid/issues/1026)
- Parent: [#119](https://github.com/MediaNoxLabs/oxid/issues/119)
- Amends: ADR-0013, ADR-0018, ADR-0021, ADR-0024, ADR-0080, ADR-0086,
  and ADR-0095
- Implementation state: architecture only; persistence, UI, headless operations,
  and export require issue-backed child slices

## Context

ADR-0080 deliberately admits only a bounded, process-local ring of payload-free
diagnostic codes. That surface is safe for current health, but it disappears on
restart and cannot explain a failure that crosses process or lifecycle
boundaries. The reviewed prototype has useful action correlation, pagination,
and separate live/archive controls, but its arbitrary messages, targets,
tracing fields, paths, and unbounded persistence are an unacceptable secondary
data channel for an identity wallet.

Oxid needs an optional operational support journal without turning diagnostics
into wallet authority, telemetry, or an audit ledger. This decision fixes the
privacy model and resource envelope before any durable adapter is implemented.

## Decision

### Two separate diagnostic products

The existing ADR-0080 live ring remains enabled, process-local, payload-free,
and capped at 256 entries by default (1,024 maximum). It does not acquire
timestamps, identifiers, persistence, or export.

The durable support journal is a separate, optional composition capability. It
is **off by default** in every build profile. A holder may enable one support
session for at most 24 hours; expiry stops new durable capture without deleting
or extending the existing archive. Enabling, extending, clearing, and exporting
are explicit user actions. No environment variable, remote request, deep link,
or support recipient can enable it.

The journal is best-effort operational evidence only. Its presence, absence,
order, or content cannot authorize or decide readiness, retry, consent,
custody, signing, proof, submission, transaction finality, credential validity,
recovery, or compliance.

### Closed record model

The application port accepts only a versioned `SupportJournalEvent` made from:

- a device-local monotonic `u64` sequence;
- closed subsystem, event-code, severity, lifecycle-stage, and outcome enums;
- a random 128-bit session epoch generated when capture is enabled;
- an optional random 128-bit action token generated for one local action;
- an optional wall-clock time rounded down to the minute; and
- the compile-time schema version.

Session epochs and action tokens are random correlation handles. They are not
derived from, and may never equal, a profile ID, account fingerprint, address,
DID, credential, transaction, proof, native handoff, or protocol request ID.
They are destroyed on archive clear and are never stable across journal epochs.

There is no string, byte buffer, map, arbitrary target, source location,
request identifier, endpoint, path, error, stack trace, or extension field.
User-facing labels are derived locally from the closed enums and are never the
persisted source of truth. Adding a code or field requires schema and ADR
review; there is no sanitised-string escape hatch.

### Fixed resource envelope

The durable adapter must enforce the following limits before allocation,
encoding, decompression, or write:

| Resource | Hard limit |
| --- | --- |
| Capture window | 24 hours from explicit enablement |
| Archive retention age | 7 days from the last recorded event |
| Pending channel | 128 records; overflow increments one closed drop counter |
| Durable records | 4,096 records |
| Durable encoded bytes | 2 MiB including framing and indexes |
| Capture rate | 20 records/second and 200 records/minute per process |
| Flush batch | 32 records or 2 seconds, whichever occurs first |
| Export bundle | 1 MiB and 4,096 records after framing |
| Rendered page | 100 records; newest first |

The first reached durable limit triggers deterministic oldest-segment eviction
and a closed gap marker. Rate or channel pressure drops records rather than
blocking a wallet operation. Journal lock poisoning, disk full, key
unavailability, encoding failure, corruption, and writer death degrade or stop
capture but never alter the initiating wallet result.

On the first application activation after the seven-day deadline, the adapter
destroys the expired epoch key and archive before serving a page or export.
The OS is not expected to wake a suspended application just to enforce expiry.

Minute-rounded wall time is display and filtering help only. Monotonic sequence
and explicit restart/session markers own local ordering. Clock rollback emits a
closed clock-discontinuity code and cannot reorder records or extend retention.

### Storage, encryption, and integrity

The archive is physically and logically separate from wallet, profile,
credential, consent, transaction, protocol, and backup stores. It is excluded
from every backup, recovery, profile transfer, crash report, and telemetry path.

Each journal epoch uses a fresh random data-encryption key, protected by the
platform's device-local secret store. Versioned records use reviewed
authenticated encryption with unique nonces. Writes use length-delimited
frames, an authenticated segment header, write-to-new-plus-atomic-replace, and
bounded recovery that quarantines the smallest corrupt segment. A keyed chain
over frame sequence makes modification, reordering, duplication, and internal
gaps detectable while the protected epoch key and latest sealed head remain
available.

This is not non-repudiation. An attacker able to delete both archive and sealed
head, compromise the device secret store, or roll back the complete protected
application container may erase or roll back evidence. Export review states
that limitation. The wallet never infers safety from a chain-valid archive.

Confirmed clear destroys the epoch key where the platform permits, removes the
archive through crash-safe replacement, clears the live view separately only
when requested, and starts no new capture session. Interrupted clear resolves
to either the prior readable epoch or an empty archive; it never mutates wallet
state.

### Review-first support export

Export is a pull/user-share flow. Before encryption, the review surface shows
the rounded time range, schema/build provenance, record counts by closed
subsystem/severity/outcome, gap/eviction/drop totals, and every metadata field
that will leave the device. It shows no raw wallet or protocol state because
none can enter the record model.

The holder explicitly selects one of two reviewed encryption modes:

1. a support-recipient public key shipped in a signed build manifest with key
   ID, validity interval, and revocation status; or
2. a fresh high-entropy one-time secret displayed once for a user-controlled
   out-of-band exchange.

An absent, expired, revoked, ambiguous, or untrusted recipient key disables
that mode; there is no fallback to plaintext. The versioned bundle is bounded
before construction, authenticated, encrypted, and created in owner-only
temporary storage. Cancellation or share failure removes the temporary bundle
and never leaves plaintext. Delivery uses the OS share/export surface only.
There is no upload URL, analytics SDK, background sync, live stream, remote
shell, or automatic retry.

### Ports and later delivery slices

The application layer will own the closed event types and use cases for
enable/disable, append, status, page, clear plan/confirm, and export plan. It
will not depend on tracing, Dioxus, a database, OS APIs, or a transport.

A separate encrypted adapter will own bounded persistence, key access,
framing, corruption isolation, and bundle encryption. Composition may omit the
adapter entirely. The domain and wallet-authority crates remain unaware of the
journal.

Dioxus and the versioned headless protocol may later expose equivalent typed
status, filtering, paging, clearing, and export planning. Search operates only
over locally rendered closed labels. Headless stdout remains protocol-only;
invalid input is never echoed. Dioxus must represent disabled, locked,
degraded, corrupt, evicted, export-ready, export-failed, and cleared states and
retain accessible focus, announcements, dynamic text, and safe-area behavior.

Implementation is split into bounded children of #119:

1. typed application port and negative compile/runtime tests;
2. encrypted bounded adapter with crash/corruption tests;
3. Dioxus and headless read/clear surfaces; and
4. review-first encrypted OS-share export.

Each slice must inject sentinel seeds, addresses, DIDs, claims, endpoints,
paths, native errors, and proof/transaction bytes and prove they cannot enter
records, UI DTOs, headless output, bundles, logs, or persisted bytes.

## Consequences

- Restart-spanning troubleshooting becomes possible only after explicit,
  time-bounded holder enablement.
- Compile-time closed types and fixed limits keep persistence reviewable and
  resource-safe.
- Journal failure is deliberately invisible to wallet outcomes; support may
  see gaps and degraded capture.
- Correlation helps reconstruct an operational timeline without becoming a
  consent receipt, transaction journal, or compliance record.
- Remote support remains an explicit encrypted file exchange, not telemetry.
- #119 remains open until the separately reviewed application, adapter,
  presentation, export, and adversarial-test slices are complete.

## Alternatives rejected

- Copying the prototype tracing/Redb log would persist arbitrary strings and
  identifiers outside Oxid's reviewed state boundaries.
- Enabling durable capture by default would create hidden longitudinal
  retention for users who never asked for support.
- Reusing profile, backup, or transaction storage would confuse operational
  evidence with authoritative or recoverable wallet state.
- Hash chaining without protected authenticated framing would overstate
  integrity and still fail to prevent deletion or rollback.
- Automatic upload or live support streaming would require a separate hosted
  service, authentication, abuse, privacy, retention, and deletion design.
- Persisting sanitised errors or tracing fields would recreate an unbounded
  payload channel under a different name.
