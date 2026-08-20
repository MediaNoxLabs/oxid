<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0063: Settle Passport Vault calls through the Midnight submission lifecycle

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/wallet.rs`, `mobile-bench/wallet-core/src/tx/balance.rs`, and `mobile-bench/wallet-core/src/tx/prove.rs`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0026 through ADR-0028, ADR-0033 through ADR-0035, ADR-0056 through ADR-0062, and issue #31
- Supersedes: ADR-0062's implementation-state boundary that DUST completion, proving, submission, and reconciliation are pending
- Implementation state: complete standalone composition settles native create/deposit/withdraw calls; protected claim composition remains pending
- Amended by: ADR-0064

## Context

ADR-0062 leaves an exact authorized Passport Vault transaction funded with
unshielded NIGHT and retained inside adapter custody. A useful wallet must next
synchronize DUST, balance the fee, prove, broadcast, and report a finalized
outcome. These steps are already implemented for ordinary wallet transfers,
including a persist-before-broadcast journal, cancellation boundary, ambiguous
outcome handling, and finalized-chain reconciliation.

Copying that lifecycle into the Passport Vault adapter would create two subtly
different authorities for the most failure-sensitive part of the wallet. Using
the ordinary transfer application port would be equally incorrect: a complete
Compact contract transaction is not a recipient transfer, and its serialized
transaction, proof, and signatures must never enter an incoming or application
view.

The prototype executes the same broad sequence but returns a transaction hash
directly and does not preserve Oxid's staged authorization and recovery
invariants. Oxid must migrate the useful proving/submission behavior without
migrating that weaker boundary.

## Decision

The Midnight adapter exposes a composition-only contract-call completion port.
Its request contains opaque public identifiers, the exact network and planning
fingerprint, expiry metadata, and a zeroizing funded transaction. The port
fully decodes the bounded tagged transaction, rejects trailing bytes, requires
the selected profile network, one or two contract/funding intents, no remaining
NIGHT deficit, and a synchronized protected account before borrowing the role
2 DUST child.

Completion then uses the existing Midnight transaction completer unchanged:
fresh DUST synchronization, exact fee balancing, local or configured remote
proving, proof sealing, fee verification, persist-before-broadcast, unsigned
node submission, and finalized success/failure inspection. The public result
contains only transaction hash, block hash, block height, DUST fee, and the
truthful live/simulated mode.

The existing public submission journal moves to schema version 2 by adding an
optional finalized block height. Schema version 1 remains readable. Passport
Vault records use a deterministic domain-separated profile key and a
`vault-`-prefixed draft key, so they can share one bounded owner-private journal
without appearing in ordinary wallet transfer status, history, or idempotency
lookups. Serialized transactions, proofs, signatures, endpoints, credentials,
and key material are never journaled.

Before broadcast, cancellation is accepted atomically and the Passport Vault
draft returns to `authorized`. The future's drop guard requests the same safe
cancellation. Once persist-before-broadcast crosses the boundary, cancellation
is refused. A finalized rejection or expiry permits replacement. A transport,
worker, or node ambiguity after broadcast becomes `outcome_unknown`; the
funded transaction is erased and another submission is forbidden until
finalized reconciliation resolves it. Included outcomes are idempotent and are
restored from a newly constructed adapter when a JSON journal is configured.

The native Passport Vault adapter runs completion and reconciliation on named
workers, merges process-local draft state with the Midnight journal, and erases
its retained transaction after inclusion, rejection, expiry, or ambiguous
broadcast. Incoming headless and mobile views continue to receive public status
only.

Complete standalone composition reports `native_settlement` and
`settlesOnMidnight: true` only when canonical replay, the packaged composer, and
the full standalone Midnight completion stack are composed. Capability
discovery lists only create, deposit, and withdraw for this native mode.
Protected claim remains unavailable because the prototype's public-derived
holder scalar and fixed presentation nonce are forbidden; it will not be
advertised until managed holder custody and fresh randomness are composed.

## Rejected alternatives

- Duplicating DUST/proving/submission inside the product adapter would fork
  cancellation, retry, and ambiguity semantics.
- Passing the transaction through the Passport Vault application service or
  headless protocol would expose private ledger material.
- Reusing un-namespaced transfer journal keys would mix product calls into
  ordinary wallet history and conflict detection.
- Retrying after a timeout or dropped node stream could submit the same value
  transition twice.
- Marking a node-accepted transaction included before finalized success events
  would overstate settlement.
- Enabling native claim with the prototype's deterministic shortcuts would
  violate the wallet's custody and unlinkability requirements.

## Consequences

- Native create, deposit, and withdraw now exercise the full standalone
  Midnight DUST/proving/finalized-submission path.
- One journal and reconciler define recovery semantics for wallet transfers and
  Passport Vault calls while domain-separated keys keep their public histories
  independent.
- Durable restart recovery requires the existing explicit
  `OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH`; without it, public recovery metadata
  remains process-local, as for ordinary transfers.
- The remaining Passport Vault settlement gap is protected claim composition,
  not generic proving or submission infrastructure.

## Validation

- `cargo test -p oxid-adapter-midnight --lib`
- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo test -p oxid-composition --lib`
- `cargo test -p oxid-headless`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`
