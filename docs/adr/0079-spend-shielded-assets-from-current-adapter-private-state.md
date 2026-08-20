# ADR-0079: Spend shielded assets from current adapter-private state

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–8, 12–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/tx/balance.rs`, `tx/prove.rs`, and the shielded wallet state
- Canonical source: `midnight-ledger` commit `d9414884db9da9e9b1f6f3a7f742d79a5732f817`
- Tracking: issues #2, #59, #91, and #93
- Amends: ADR-0026 through ADR-0029, ADR-0033 through ADR-0035, and ADR-0077
- Implementation state: canonical Zswap planning, exact safe previews,
  fresh-sync admission, shared authorization/submission recovery, combined
  DUST/Zswap proving resolver, headless lifecycle, deterministic standalone
  flow, Dioxus privacy selection, guarded funded standalone finality/adapter-
  reconstruction/nullifier evidence, and the repository/headless/Dioxus
  protected-DUST registration prerequisite are implemented; funded fresh-
  wallet registration-to-recovery/spend, production mobile custody, and
  physical-device proving remain governed by their existing gates

## Context

ADR-0033 deliberately stopped at shielded address derivation, official event
replay, private checkpointing, and safe balance projection. It kept the Zswap
secret keys, qualified coins, nullifiers, Merkle paths, and witnesses inside the
Midnight adapter so a later spending flow would not need to widen application
DTOs. The migrated prototype contains the canonical operation needed for its
Passport Vault deposit: select a qualified wallet coin, call the official
Zswap state `spend`, construct recipient/change outputs, and prove the result
with a ledger resolver that combines Zswap and DUST artifacts.

Oxid already has a safer staged transfer lifecycle than the prototype's
aggregate wallet method: public preparation, exact authorization, retained
adapter-private bytes, DUST balancing/proving, persist-before-broadcast,
cooperative pre-broadcast cancellation, and finalized reconciliation. A
separate shielded submission subsystem would duplicate those safety rules and
create inconsistent recovery semantics.

Cached state cannot authorize a private spend. It may be incomplete, stale, or
from a source that is no longer current even when its cursor once equalled its
target. The app must complete the configured live catch-up for the same
profile, network, key, and source before qualified notes are eligible.

## Decision

Extend the Oxid-owned wallet transaction port with a focused shielded prepare
operation. Incoming callers provide a canonical shielded recipient, exact
32-byte lowercase token type, and decimal-string atomic amount. The resulting
public preview adds the recipient privacy kind and exposes only network,
account, asset, amount, change, input count, expiry, fee state, draft handle,
and authorization challenge. It never exposes selected coins, nullifiers,
commitment paths, output nonces, ciphertexts, proof preimages, keys, or
serialized transaction bytes.

The Midnight adapter admits preparation only when the profile/network shielded
snapshot is `synced`, has no failure, has a present and equal current/target
cursor, and has a completion timestamp. Live preparation reopens the
owner-private checkpoint through the same role-3 derived key and source
fingerprint, then verifies that its cursors exactly match the published
snapshot. `cached`, `syncing`, `cancelled`, `stalled`, unavailable, mismatched,
or missing state fails closed as `ShieldedStateNotCurrent` or invalid chain
state.

Inside that boundary, select qualified coins of the requested official
`ShieldedTokenType` until the exact amount is covered. Call
`midnight_zswap::local::State::spend` for every selected coin, create one
encrypted output for the recipient, create an encrypted role-3 wallet change
output when necessary, normalize an official `ZswapOffer`, and retain the
unproven standard transaction. The all-zero native shielded token is rendered
as NIGHT with six decimals; unknown token types remain exact atomic values with
no invented decimal precision.

Serialize shielded planning per profile while a draft is prepared, authorized,
or submitting. An identical request returns the retained preview; a competing
request fails with `DraftConflict`. This prevents two process-local drafts from
selecting the same adapter-private note before authoritative replay advances.
The existing journal retains only a domain-separated one-way fingerprint of
the synchronized owned-note state, never raw nullifiers or coins. A
`broadcasting`, `outcome_unknown`, or `included` record blocks every new plan
from that unchanged private state until a fresh replay advances it; rejected or
expired records do not. Fingerprint lookup must select an included barrier over
an unresolved barrier, and either barrier over rejected/expired attempts,
regardless of record order or timestamp. At bounded capacity, the journal may
evict only rejected or expired attempts. If all 128 records remain duplicate-
submission barriers, a new attempt fails unavailable before broadcast rather
than silently discarding replay protection. Checkpoint-acknowledged safe
compaction is deferred to issue #93.

The live indexer v4 `zswapLedgerEvents` envelope reports the GraphQL object
typename `ZswapLedgerEvent`; the input/output variant exists only inside the
tagged official ledger event. Decode that exact envelope typename, then accept
only deserialized `ZswapInput` or `ZswapOutput` details. Its event IDs are
sparse global cursors, so transport accepts gaps while requiring strict forward
movement, a non-regressing advertised target, and exact equality with the
target before publishing `synced`.

Authorization promotes only that retained transaction after the existing
bounded human-readable confirmation. Shielded inputs do not use an unshielded
Schnorr signature; their ownership proof is produced later from the retained
Zswap proof preimages. Submission then reuses the existing DUST synchronization
and balancing, proof, journal, broadcast, cancellation, terminal result, and
reconciliation machinery.

Local proving uses the official ledger `Resolver` with authenticated Zswap and
DUST artifact manifests. The owner-private cache stays symlink-resistant and
is bounded to 64 entries and 256 MiB, with individual bounded downloads and
hash verification before install. The explicit loopback/HTTPS proof-server
mode remains a development alternative and receives only the proof requests
already selected by standalone configuration.

`oxid.headless.v1` adds `prepare_shielded`, `authorize_shielded`,
`submit_shielded`, and the prototype-style `send_shielded` alias. Draft,
submission status/history, cancellation, and reconciliation remain shared.
Dioxus presents public and shielded NIGHT as an explicit privacy choice in the
same staged transfer panel. All blocking preparation and completion continue
through ADR-0077's worker boundary.

The deterministic standalone controller constructs a real official Zswap
offer from a protected seeded test note after its simulated sync reaches
`synced`. Its terminal transaction and block identifiers remain simulation
evidence, not claims of chain inclusion. Normal production composition remains
fail-closed unless approved custody, live endpoints, checkpoints, and proving
configuration are supplied. This decision does not waive physical-device
latency, peak-memory, lifecycle, thermal, or custody release gates.

A separate ignored acceptance test may receive the reviewed public standalone
genesis root only through ADR-0098's double opt-in, one-shot zeroizing random
adapter. It synchronizes that protected authority's public account and real
native Zswap notes, sends one exact shielded amount to a fresh OS-random
protected recipient, proves finalized inclusion, rejects an unchanged-state
duplicate, reconstructs the adapter from the owner-private checkpoint and
public journal, returns the restored included status idempotently through the
reconciliation use case, and proves exact sender/recipient balances after
nullifier replay. Because the stored attempt is already included, this run does
not exercise unknown-outcome chain rescanning. It is adapter reconstruction
with reused in-process development custody, not process restart or native-
custody recovery evidence.
The recipient cannot yet originate a second transaction because Oxid does not
implement typed DUST registration; issue #92 owns that prerequisite and the
stronger fresh-wallet spend proof.

## Validation

```bash
nix develop -c cargo test -p oxid-adapter-midnight shielded
nix develop -c cargo test -p oxid-adapter-midnight submission_journal::tests
OXID_ENABLE_LIVE_STANDALONE_FUNDING=1 \
  OXID_STANDALONE_FUNDER_SEED_HEX=<operator-supplied-development-seed> \
  nix develop -c just standalone-funded-shielded-finality
```

The guarded funded command passed against the repository-owned standalone
node, indexer v4, and proof server on 2026-08-20. No seed, note, nullifier,
witness, checkpoint, or transaction material is repository or issue evidence.

## Consequences

- Shielded receive, replay, and spending now share one key/network/source
  authority without exporting private local state.
- Public and shielded transfers have one authorization and recovery model,
  reducing duplicate-broadcast and cancellation divergence.
- A completed live catch-up is a hard spend precondition; a durable checkpoint
  alone remains display/resume input.
- Multiple token types are supported at exact atomic precision. The mobile
  first slice selects native shielded NIGHT; headless can name any exact token.
- Zswap proving artifacts increase the bounded local proving cache and make
  device measurement more important. No simulator result satisfies the
  physical-device release gates.
- Prepared shielded witness material is process-local and expires with its
  draft. Durable recovery begins only at the existing public pre-broadcast
  journal boundary; a lost prepared draft must be rebuilt after a fresh sync.
- Correctness currently takes priority over indefinite journal availability:
  128 retained included/unresolved barriers refuse a new broadcast until issue
  #93 proves checkpoint-aware compaction.

## Rejected alternatives

- Exposing qualified coins or Merkle witnesses through the application layer
  was rejected because it breaks ADR-0033 and makes privacy-sensitive ledger
  types public API.
- Spending from a cached checkpoint was rejected because cursor equality from
  an earlier session is not evidence of current chain state.
- Implementing a second shielded submission journal was rejected because the
  existing lifecycle already defines cancellation, ambiguity, and replacement
  safety.
- Treating explicit confirmation as an unshielded signature request was
  rejected because Zswap ownership is proven by its circuits, not the account's
  public UTXO signing key.
- Claiming mobile release readiness from deterministic or simulator proving was
  rejected because resource and custody evidence must come from physical
  devices under the existing gates.
