# ADR-0090: Compose onboarding, backup receipts, and account sync

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–8, 12–13, 16, and 18
- Design source: `docs/design/journeys.md` Onboarding, Sync, and Backup & recovery journeys; `docs/design/rollout.md` Phase 2c
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet` and `wallet-core`
- Tracking: issues #2, #33, #65, and #82
- Implementation state: Dioxus owns the bounded onboarding and sync presentation; wallet application owns a profile-scoped successful complete-backup receipt

## Context

Fresh Oxid installations currently stack profile creation and complete-wallet
recovery on one long page. Creating a profile immediately enters the wallet,
so progressive device protection is offered later and the opaque public profile
identifier is briefly rendered. The complete backup implementation already
authenticates one all-store archive and transfers it through fixed-kind native
document pickers, but the UI can only report that backup is supported. It has
no application fact proving that the document picker actually completed.

The Wallet page also presents account refresh, DUST replay, and shielded replay
as three separate chores. DUST and shielded panes expose cursor-derived progress
and per-run event counts even though those are adapter diagnostics rather than
holder decisions. Their independent application state machines, resumable
checkpoints, cancellation, and live-before-spend authority must remain intact.

The design requires a create-or-restore onboarding fork, a truthful backup
celebration, and sync as account-card state. Inferring **Backed up** from
`portable_backup_supported` would be false: capability availability says
nothing about whether encryption and native document transfer succeeded.

## Decision

### Onboarding

Dioxus owns a bounded, non-persisted onboarding route:

1. Welcome offers exactly **Create new wallet** or **Restore from backup**.
2. Create names and selects a profile through the existing application use
   cases. The normal user surface never renders its opaque identifier.
3. A skippable protection offer may invoke only the existing profile-scoped
   initialization use case. Failure remains visible and does not prevent the
   user from entering Home or protecting the wallet later.
4. Restore re-houses the existing complete-wallet recovery component as its own
   path. Back discards component-local secret state. Recovery remains
   authenticated, exact-confirmation-gated, empty-install-only, and non-merging.

No onboarding step selects adapters, changes backup contents, invents biometric
support, or creates a seed-phrase flow.

### Successful complete-backup receipt

Add a focused wallet-application port and use cases for one public fact:
the latest time this profile successfully completed a complete-wallet document
export. The repository stores only profile identifier and UTC millisecond
timestamp. It stores no secret, path, filename, archive/ciphertext bytes,
document-provider identifier, platform authorization result, or key metadata.

Dioxus records the receipt only after both boundaries succeed in order:

1. `ExportCompleteWalletBackupUseCase` returns an authenticated encrypted
   package after its exact confirmation and fresh custody authorization; and
2. `PortableWalletBackupDocumentPort::export(CompleteWallet, ...)` returns
   success after the fixed-kind native picker completes.

Cancellation or failure at either boundary records nothing. Legacy
custody-only export/recovery records nothing. A complete archive deliberately
excludes this receipt, so restoring an old document never imports a stale
claim that the new installation has already saved a new backup.

The repository must reject unknown profiles, keep the timestamp monotonic, and
remove the receipt with its profile. JSON persistence advances to a new schema
with strict legacy reads; the in-memory adapter implements the same contract.
Home and Settings may say **Backed up** only when the query returns a receipt.
Supporting copy states that this is the last successful export and does not
claim that the external file remains present, readable, current, or safely
stored. A successful export can render a bounded accessible celebration.

### Account sync composition

Dioxus replaces the standalone account-refresh control plus symmetric DUST and
shielded panes with one account sync card. One **Sync now** action first invokes
the existing account refresh and then starts the existing DUST and shielded
sessions that are available for the active protected profile. The card polls
their existing public status ports and renders short human freshness states and
progress. While either session is syncing, the same action becomes **Cancel
sync** and sends only the existing non-waiting cancellation requests.

This visual composition is not an atomic cross-port transaction: each
application state machine remains independently authoritative and a partial
failure is reported without relabelling another successful session. Cursor and
event-count fields are not rendered in the user profile. Cached, cancelled, or
stalled state remains display/resume information only. Transfer availability
continues to depend on the existing fresh account and shielded authority.
Background OS scheduling and pull-to-refresh gestures require separate platform
events and are not fabricated in this slice.

## Security and architecture boundaries

- Dioxus remains an incoming adapter and owns only route/view state plus the
  ordering of existing commands.
- The backup receipt is public metadata, never backup authority. It cannot
  authorize recovery, suppress re-authentication, prove external file
  availability, or enter an encrypted archive.
- Receipt storage is profile-scoped, bounded by the profile-count limit,
  owner-private where persisted, strict, atomic, and symlink-resistant under
  the existing profile repository rules.
- Back/cancel clears component-local recovery-secret state; secrets remain
  zeroizing and absent from logs, DTOs, receipts, and diagnostics.
- Protection initialization remains explicit and skippable. Copy names device
  protection rather than promising biometrics or hardware backing.
- Unified sync never receives keys, endpoints, checkpoints, cursors, ledger
  events, notes, nullifiers, or proofs. It uses only existing commands and safe
  views.
- Production composition keeps fail-closed security and chain adapters; this
  decision adds no development fallback.

## Consequences

- Fresh-install users make one decision per screen and reach Home without an
  opaque identifier or seed phrase.
- Recovery is discoverable from the first screen without competing with the
  create form.
- **Backed up** becomes an evidence-based application fact instead of a UI
  inference. The fact is intentionally weaker than “recoverable now.”
- Account synchronization reads as one wallet concern while DUST and shielded
  correctness, checkpointing, cancellation, and spend gates remain separate.
- A later dev UI profile may expose cursors and per-run event counts from the
  same views; the user profile does not.

## Validation

- Wallet-application contract tests cover missing, recorded, monotonic, invalid
  profile, clock, and persistence outcomes.
- In-memory and JSON adapter tests cover profile scope, schema migration,
  restart, unknown-profile rejection, removal, and portable-snapshot exclusion.
- Dioxus unit/compile gates cover onboarding transitions, receipt labels, sync
  summaries, combined progress, and truthful failure copy.
- iOS and Android standalone flows exercise the onboarding fork and one-card
  sync. Native document-picker backup flows assert celebration and the receipt
  only after successful export.
- Strict architecture, source, test, coverage, advisory, license, and docs
  gates remain required.

## Rejected alternatives

- Treating `portable_backup_supported` as **Backed up** would turn adapter
  availability into a false user-state claim.
- Recording a receipt before the native picker completes would celebrate a
  package that the user may have cancelled or failed to save.
- Putting the receipt inside the complete archive would import stale success
  into a fresh installation.
- Persisting a path or document-provider identifier would exceed the public
  metadata need and is not portable across OS document providers.
- Replacing DUST and shielded ports with one aggregate core sync service would
  couple distinct privacy, checkpoint, and spend-authority state machines for a
  presentation requirement.
- Claiming background sync or pull-to-refresh without a reviewed platform event
  would add inert or misleading UI.
