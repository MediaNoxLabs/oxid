# ADR-0032: Expose resumable DUST synchronization as an adapter-owned session

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 5–8, 12–13, 16–18 and [issue #17](https://github.com/MediaNoxLabs/oxid/issues/17)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/dust/syncer.rs` and `mobile-bench/dioxus-wallet/src/app.rs`
- Amends: ADR-0024 and ADR-0031
- Implementation state: Oxid-owned status/use cases, simulated and native standalone controllers, v1 headless methods, Assets-page progress/cancellation, partial checkpoint resume, and native GraphQL-WebSocket worker fixtures implemented; production mobile custody remains pending
- Amended by: ADR-0080

## Context

ADR-0031 made a completed key-scoped DUST checkpoint available internally to
transaction submission, but it did not expose the prototype's explicit DUST
sync operation. The reviewed mobile application shows DUST progress separately
from public NIGHT synchronization, reports the current and target event IDs,
and derives the displayed balance from the official `DustLocalState`.

Treating this as a generic account refresh would hide three material facts:
DUST state is key-scoped, a cached state is not live enough to spend, and a
long replay must be cancellable without leaving corrupt state. Passing ledger
events, database types, or transport streams into the application or Dioxus
layers would violate the Oxid-owned type boundary.

## Decision

Add a focused `WalletDustSyncPort` with status, start, and cancel operations.
Its domain snapshot contains only network identity, lifecycle state, optional
current/target cursors, events processed in the current run, exact `u128`
atomic balance, freshness timestamp, and a bounded failure category. The
application maps exact amounts to decimal strings and stable names. No ledger,
event, database, endpoint, key, or transport type crosses the port.

The native Midnight adapter owns each profile/network session and runs chain
tip retrieval, WebSocket input, official ledger folding, and checkpoint I/O on
a dedicated worker thread. It scopes DUST derivation to the active account
index recorded when the profile's account was derived, defaulting to account
zero only before a profile binds a different account. Incoming adapters only
start, poll, or cancel that session. One profile/network cannot start two
concurrent sessions. Cancellation is checked before network work and around
every replay batch; the DUST seed is borrowed only for the worker's custody
callback and is never retained or returned.

Persist each successfully folded bounded batch as a partial checkpoint with
`current_cursor <= target_cursor`. This amends ADR-0031's completed-cursor-only
validation. A restart or cancelled session resumes at `current_cursor + 1`.
Network, public-key, schema, or live-parameter mismatch remains a clean miss;
an incompatible delta may replay once from zero only before it emits new
progress. A failure after emitted progress retains that consistent partial
checkpoint instead of rewinding the visible cursor.

Cold catch-up separates bounded network receive from official ledger replay.
Each subscription accepts at most 16,384 events or 16 MiB of decoded serialized
event input and may close after a one-second quiet period once at least one
256-event batch has arrived. The client sends GraphQL `complete` and drops the
socket before folding, invoking the checkpoint observer, or publishing
progress. Replay retains the 256-event/4 MiB checkpoint cadence; only an
observer-accepted cursor becomes the start of the next subscription. Sparse
cursor ordering, target monotonicity, cancellation, incompatible-checkpoint
fallback, and the existing one-million-event/512 MiB/30-minute whole-run limits
span every reconnect rather than resetting per segment.

A cached checkpoint may be displayed and may seed a later replay, but it is
never represented as `synced` after an unavailable or failed live catch-up and
never independently authorizes DUST spending. Transaction submission still
requires the live target under ADR-0031. Status failures are sanitized into
protection, network, transport, timeout, chain-state, or storage categories.

Extend `oxid.headless.v1` compatibly with:

- `wallet.dust.sync.status`;
- `wallet.dust.sync.start`; and
- `wallet.dust.sync.cancel`.

The deterministic standalone simulator advances only when status is polled,
which makes fresh, progress, cancellation, resume, and already-current flows
race-free in the headless harness. The Dioxus Assets page polls the same use
case while a session is running, renders exact DUST and bounded progress, and
labels cached/stalled state as non-spend-ready. Normal production composition
continues to return `unavailable` until approved native custody is composed.

The native controller contract suite uses a fixed internal chain-tip source so
pure Nix does not depend on HTTP loopback, then exercises the real bounded
`graphql-transport-ws` path. It verifies an owned official DUST event produces
the exact 12 DUST projection, a later run subscribes from `cursor + 1`, a
256-event batch is checkpointed before cancellation, the server observes
subscription completion before the first replay observer, a saturated segment
resumes from its accepted cursor, target regression across reconnect fails
closed, and observer failure is not reclassified as an incompatible cached
delta. Transport failures publish only the stable redacted category.
Production construction continues to use the existing bounded HTTP chain-tip
source.

## Consequences

- Headless and mobile use the same DUST lifecycle instead of duplicating wallet
  or SDK logic.
- Long native sync work cannot block the renderer or the NDJSON request loop.
- Cancellation loses at most the not-yet-folded bounded batch and resumes from
  the latest durable cursor.
- Partial private checkpoints increase write frequency, but remain bounded,
  owner-private, atomically replaced, and scoped by network plus public-key
  fingerprint.
- Exact cached balance is useful for display, while state and failure fields
  prevent it from being confused with live spend readiness.
- DUST history is still not retained; only the official folded state and its
  validated offset are persisted.
