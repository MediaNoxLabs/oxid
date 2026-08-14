<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0055: Replay canonical Passport Vault history before mutation

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts`
- Consensus source: `midnight-ledger` commit `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, ledger transaction structures, verifier, and on-chain runtime
- Node source: `midnight-node` commit `06858f9a7fe40866c2c074ff07eecc39d7d35ef7`, `pallets/midnight/src/lib.rs`
- Related: ADR-0003, ADR-0004, ADR-0006, ADR-0013, ADR-0015, ADR-0018, ADR-0020, ADR-0027, ADR-0035, ADR-0051, ADR-0052, ADR-0054, and issue #31
- Supersedes: ADR-0054's open choice between deterministic replay and a reviewed storage proof for Passport Vault state authentication
- Implementation state: bounded native transaction decoding, outcome authentication, and exact contract-local replay are implemented as a transport-independent verifier; the complete finalized-node block scanner, cache, authenticated read composition, and contract calls remain issue #31

## Context

ADR-0054 proves that an indexer action block is canonical and finalized, but it
does not authenticate the state bytes returned by that indexer. The Midnight
node does authenticate the raw inner transaction and emits canonical pallet
events for contract operations that actually applied. Calls, deployments, and
maintenance are emitted in typed batches while preserving order within each
batch. The transaction carries
the exact public transcripts and effects needed to reconstruct contract-local
state from the authenticated deployment.

Partial success is subtle. Midnight applies every guaranteed transcript before
attempting fallible intent segments, so a failed fallible segment does not roll
back its guaranteed effect. Pallet operation events identify applied actions,
but repeated actions against the same address can make the failed segment
ambiguous. Indexer `transactionResult` data is useful for diagnostics but is not
the consensus authority for that choice.

The prototype composes and proves calls in a WebView from indexer state. Its
claim path also derives a holder signing scalar from the public credential
claim root and uses a fixed presentation nonce. Those shortcuts are not safe
custody or signing behavior and cannot be migrated into Oxid.

## Decision

Oxid selects deterministic replay from the canonical deployment. A pure native
verifier consumes a complete, canonically ordered sequence of successful
Midnight extrinsics supplied by a future finalized-node scanner. For every
transaction it:

1. strictly decodes one official tagged proven transaction with no trailing
   bytes and enforces per-transaction, per-action, and aggregate bounds;
2. recomputes the official inner transaction hash and requires an exact match
   with the node's pallet outcome event;
3. matches applied actions to the pallet's canonical event batches: ordered
   `ContractCall` events, then ordered `ContractDeploy` events, then ordered
   `ContractMaintain` events;
4. for partial success, derives every event-compatible intent outcome and
   proceeds only when all outcomes produce the same target action set;
5. applies all target guaranteed transcripts, then only uniquely authenticated
   target fallible actions, using the official `QueryContext`, exact consensus
   `BlockContext`, prior official `ContractState`, and exact transcript effects;
6. applies checked unshielded balance effects and serializes the resulting
   official state exactly.

The verifier does not independently re-verify signatures or ZK proofs. It
relies on the finalized node's consensus outcome for those checks and uses the
raw transaction hash plus operation events to bind the replay input to that
outcome. It has no transport and does not itself establish history
completeness.

The node adapter must validate the deployment in a canonical event and scan
every canonical finalized block from deployment through the target head. It
must extract the raw `Midnight.send_mn_transaction` payload, extrinsic success,
one `TxApplied` or `TxPartialSuccess` outcome, matching action events, and the
exact block context: timestamp in seconds, 30-second uncertainty, parent block
hash, and prior-block timestamp. An indexer may provide a deployment hint or an
independent comparison, but it may not omit history or choose applied
segments.

Target maintenance, non-canonical ordering, duplicate deployment, hash/event
mismatch, ambiguous target outcomes, unsupported global commitment-index
dependencies, transcript/effect mismatch, and arithmetic overflow all fail
closed. Contract mutation remains closed until the finalized scanner feeds this
verifier and the resulting state is exposed with a truthful authenticated
source/freshness label.

Holder authorization for future claims must use an opaque managed holder key
and fresh randomness. No scalar, nonce, private-state witness, or browser bridge
may cross an incoming adapter boundary.

## Rejected alternatives

- Trusting indexer state or failed-segment metadata would retain ADR-0054's
  unresolved state-authentication gap.
- Treating inclusion or `ExtrinsicSuccess` as “all actions applied” would replay
  partial-success transactions incorrectly.
- Choosing the first event-compatible segment assignment would permit repeated
  same-address operations to change target state nondeterministically.
- Re-verifying only public transcripts while ignoring exact proven effects
  would accept a different state transition from the one committed by the
  transaction.
- Copying the WebView composer would reintroduce JavaScript secret handling,
  public-data-derived holder keys, and deterministic signing nonces.
- Opening calls in the same slice would make an unimplemented history-complete
  transport appear authoritative.

## Consequences

- State authentication is split cleanly into a complete canonical observation
  adapter and a deterministic, transport-independent replay verifier.
- The verifier can be tested with official serialized transaction/state types
  without node or indexer availability.
- A compromised indexer cannot choose state bytes or partial outcomes once the
  scanner is composed; the independently configured finalized node is the
  consensus trust anchor.
- Scanning from deployment is more expensive than a latest-state query, so a
  later authenticated cache must retain its finalized cursor and canonical
  block anchor without weakening replay-on-reorg and freshness rules.
- Maintenance and transcripts requiring unavailable global commitment indices
  remain explicit compatibility gates rather than guessed behavior.
- `node_anchored_indexer` remains read-only and
  `indexer_supplied_not_proven` until the scanner is complete.

## Validation

- Exact deployment fixture round-trip through official `ContractState`.
- Exact official public-transcript execution preserving Passport Vault layout.
- Guaranteed transcript replay when the fallible segment failed.
- Rejection of inner-hash mismatch, operation mismatch, trailing bytes,
  non-canonical order, duplicate deployment, and ambiguous target outcomes.
- Acceptance of ambiguity outside the target when the target action set is
  identical for every valid outcome.
- `cargo test -p oxid-adapter-passport-vault replay::`
- `./run.sh --light --strict`
- `nix flake check --print-build-logs`
