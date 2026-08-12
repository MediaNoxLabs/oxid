# ADR-0031: Persist private Midnight DUST checkpoints behind live catch-up

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 5–8, 12–13, 17–18 and [issue #16](https://github.com/MediaNoxLabs/oxid/issues/16)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/dust/syncer.rs`
- Implementation state: Adapter implementation and standalone/headless wiring complete; shielded Zswap persistence is implemented separately under ADR-0033 and durable production custody remains pending
- Amended by: ADR-0032 permits bounded partial checkpoints for explicit cancellable synchronization

## Context

Standalone submission currently reconstructs the official ledger
`DustLocalState` from event zero before every transfer. That preserves
correctness but makes transaction preparation proportional to the entire DUST
history. The prototype persists tagged DUST state and resumes from the next
event, demonstrating the behavior needed for practical mobile use. It also
uses a network-only cache identity and a very large in-memory event queue,
neither of which is a safe boundary for Oxid.

DUST state is different from the public account projection covered by
ADR-0030. It contains key-specific wallet discovery state and official ledger
database structures used to select spendable DUST. It is not profile metadata,
must not be exposed through domain or application types, and must never contain
the DUST seed or secret scalar.

## Decision

Keep DUST checkpoint persistence inside the native Midnight outgoing adapter,
separate from the public account checkpoint store. Enable it only through the
explicit absolute `OXID_MIDNIGHT_DUST_CHECKPOINT_PATH` in a fully configured
standalone/headless composition. Default production composition, read-only
indexer mode, and zero-configuration simulation do not read or create it.

The version-1 binary envelope stores a bounded set of records keyed by the
validated network identity and a SHA-256 fingerprint of the tagged public DUST
key. Each record contains:

- a SHA-256 identity of the tagged current DUST parameters;
- the last durably folded event cursor and advertised target cursor;
- a best-effort update timestamp; and
- the official tagged serialization of `DustLocalState<DefaultDB>`.

The envelope contains no seed, secret scalar, endpoint, credential, draft,
authorization, proof, or proof witness. Reads reject unknown schema, duplicate
scope, invalid network, parameter/state mismatch, incomplete cursors, excessive
records or bytes, directly symlinked files, and owner-access violations. Writes
use an owner-only same-directory temporary file, flush it, atomically rename
it, and sync the parent directory on Unix. A malformed regular file can be
replaced after a later successful live replay.

Every submission first fetches fresh chain parameters. A matching checkpoint
at cursor `n` seeds the official ledger state machine and subscribes from
`n + 1`; mismatched network, public-key fingerprint, schema, or parameter
identity starts from zero. ADR-0032 additionally permits
`current_cursor < target_cursor` after a bounded batch so explicit cancellation
can resume without replaying already folded history. Incoming events are checked
for exact contiguous IDs and a nondecreasing target, folded into
`DustLocalState` in small bounded batches, and never accumulated in a
history-sized queue. Total event count, event size, processed bytes, message
size, idle time, and overall catch-up time remain
bounded.

A checkpoint can authorize balancing only after the live subscription proves
that its cursor reaches the current advertised target. Incompatible cached
state or delta replay gets exactly one clean replay from zero. Connection and
timeout failures do not trigger that replay and fail submission closed. Saving
a newly synchronized state is best-effort: persistence failure may make a
later attempt replay more history but cannot invalidate an already verified
live state.

## Consequences

- A stable protected DUST key can resume at the next event instead of replaying
  all history, while first use and incompatible state recover from zero.
- Cached DUST state alone can never authorize a transaction while the indexer
  is unavailable or its current parameters cannot be obtained.
- Key-specific private wallet state remains out of Oxid domain types and out of
  the public profile/account JSON stores.
- Memory consumption is bounded by a small replay batch rather than the
  prototype's large event channel; long histories remain bounded by explicit
  total work and time limits.
- Development custody is intentionally process-local today. Cross-process
  checkpoint hits become useful once the native durable-custody adapter keeps
  the same protected root; this decision does not weaken that production gate.
- Shielded Zswap has a different official state machine and remains a separate
  migration decision.
