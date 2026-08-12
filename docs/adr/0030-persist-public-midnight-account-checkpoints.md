# ADR-0030: Persist public Midnight account checkpoints outside wallet core

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 5–8, 12–13, 17–18 and [issue #15](https://github.com/MediaNoxLabs/oxid/issues/15)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/BACKLOG.md` Path B
- Implementation state: Versioned public unshielded checkpoint, incremental resume, clean-replay recovery, and executable restart/outage coverage implemented; shielded Zswap and DUST checkpoints remain pending

## Context

The live Midnight adapter previously rebuilt every unshielded account from
cursor zero and retained the result only in process memory. That is correct but
makes every restart repeat the complete public history and leaves no balance or
activity view during an indexer outage. The prototype backlog identifies the
same limitation and describes the upstream wallet pattern: persist a public
state snapshot with its replay offset, then subscribe from the following event.

Oxid's application and domain layers intentionally own chain-neutral account
views. Midnight replay state also contains protocol-specific UTXO identifiers,
transaction result shapes, and cursor semantics needed for safe incremental
folding. Moving those types inward or placing them in the public profile store
would weaken both the dependency boundary and the rule that profile metadata
must not become an unrestricted wallet database.

## Decision

Keep replay persistence inside the native Midnight outgoing adapter behind a
private checkpoint-store interface. A version-1 JSON document is keyed by
network identity and validated public unshielded address, not UI profile. It
contains only:

- available public UTXOs;
- public transaction history and exact fees;
- current and target transaction cursors plus chain-tip height; and
- the last successful update timestamp.

It must never contain roots, child scalars, opaque key records, signing
payloads, signatures, drafts, proofs, witnesses, endpoints, credentials, or
authorization state. Headless composition enables the store only through the
explicit `OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH` absolute file path. Default
production composition stays fail-closed, and zero-configuration simulation
does not create this file.

On a read, the adapter validates schema, size, count, uniqueness, normalized
hex, exact decimal `u128` encoding, cursor/target consistency, and derived tip
before projecting the checkpoint as `cached`. Invalid, incompatible,
wrong-scope, oversized, overly permissive, or directly symlinked state never
enters a wallet view. A malformed regular file may be replaced by a later
successful refresh. Reads and writes are bounded to 16 MiB and 128 account
records. Writes use a same-directory owner-only temporary file, flush it,
atomically rename it, and sync the containing directory on Unix.

For a valid checkpoint at cursor `n`, the GraphQL v4 subscription starts at
`n + 1` and seeds the fold with its UTXO and history state. Cursor overflow,
protocol inconsistency, or invalid delta data causes one replay from zero so a
reorg-like missing spend cannot corrupt the retained view. Connection and
timeout failures do not discard the last successful values; the current
process reports them as `cached` and `stalled`.

Checkpoint hydration is read-only. It does not populate the transaction
adapter's spendable-input gate. Starting a synchronization clears that gate,
and only a successful live catch-up repopulates it. A cached view therefore
cannot silently authorize a new spend after restart or a failed refresh.
Persistence is best-effort: disk failure may cause a later full replay but does
not turn a valid live response into a failed account refresh.

## Consequences

- A headless wallet can restore exact public account state immediately and
  truthfully operate in read-only cached mode during an indexer outage.
- Subsequent healthy launches process only the unshielded delta, while an
  incompatible checkpoint self-recovers through a bounded full replay.
- Midnight protocol and UTXO persistence types remain outside Oxid-owned domain
  and application crates.
- The JSON format is transparent and portable but intentionally supports only
  one process writer; production mobile hosts naturally satisfy that boundary.
- This decision does not persist ephemeral development custody. A derived
  development address changes after restart and therefore does not match the
  old checkpoint; durable native custody remains a separate requirement.
- DUST and shielded Zswap use different official state machines and serialization
  formats. They require their own bounded checkpoint decisions before being
  added, rather than being forced into this unshielded JSON schema.
