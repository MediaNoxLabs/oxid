# ADR-0035: Persist and reconcile Midnight transaction submissions

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 5–8, 12–13, 16–18 and [issue #20](https://github.com/MediaNoxLabs/oxid/issues/20)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/node/client.rs`, transaction construction/proving services, and the Dioxus submission flow
- Amends: ADR-0024, ADR-0027, ADR-0029, and ADR-0034
- Implementation state: Bounded public journal, persist-before-broadcast boundary, restart restore/duplicate prevention, finalized-chain reconciliation, headless methods, and mobile recovery presentation implemented
- Amended by: ADR-0079, ADR-0080

## Context

ADR-0034 makes cancellation safe inside one process, but an application exit
after node broadcast loses the retained draft and its attempt state. Treating
that restart as a fresh send could duplicate a transfer. Treating every missing
draft as permanently unknown would prevent the user from recovering confirmed,
rejected, or expired outcomes.

The signed transaction, proof, witnesses, DUST secret, and custody material are
not needed to identify a submission on-chain. A narrowly scoped public record
is sufficient if it is committed before the network side effect and later
checked only against finalized chain state.

## Decision

The Midnight outgoing adapter owns a versioned submission journal separate
from profile storage and retained signing drafts. Each entry is scoped by
profile, network, and draft and contains only:

- a one-way planning fingerprint and draft expiry;
- the final public DUST fee;
- the public extrinsic hash and the finalized block hash observed immediately
  before submission;
- an optional finalized inclusion block, update time, submission mode, and
  lifecycle state.

The store never contains a signed or sealed transaction, proof, witness, seed,
key, route, or authorization payload. It accepts a normalized absolute path,
rejects symlinks and permissive existing files, caps the document at 128
records and 256 KiB, and uses owner-only atomic replacement plus directory
sync. Standalone mobile development composition colocates the journal in a
private subdirectory beside the resolved public profile store. Headless runs
may override it with `OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH`.

For live submission, the adapter constructs the unsigned extrinsic, obtains its
hash and the latest finalized anchor, and durably saves `Broadcasting` before
calling the node. A journal failure prevents broadcast. The returned node hash
must equal the recorded hash. Final inclusion or rejection updates the same
entry; transport or worker ambiguity becomes `OutcomeUnknown`. Deterministic
simulation uses the identical persistence boundary for restart tests.

On restart, status and history can be reconstructed from the journal without
restoring custody. A matching `Broadcasting` or `OutcomeUnknown` planning
fingerprint blocks a new draft; an included fingerprint conflicts with a new
send. Only a finalized rejection or expiry permits a newly prepared
replacement. Oxid never retries a retained signed transaction from the
journal.

Explicit live reconciliation walks finalized blocks backward from the current
head to the recorded anchor, examining at most 2,048 blocks for the exact
extrinsic hash and its `System.ExtrinsicSuccess` or
`System.ExtrinsicFailed` event. A successful match records `Included`; a failed
match records `Rejected`. If the anchor is reached without a match, the
authoritative indexer tip timestamp may mark the attempt `Expired`; otherwise
it remains `OutcomeUnknown`. If the anchor is not reached within the bound, the
result also remains unresolved. No best-block observation is promoted to a
final outcome.

The application exposes list and asynchronous reconcile use cases. Headless v1
adds `wallet.transaction.submission_history` and
`wallet.transaction.reconcile_submission`. The Assets page shows the latest
restored public attempt even when process-local custody has been lost and
offers “Reconcile with Midnight” only for reconcilable states.

## Consequences

- A crash after the durable boundary cannot silently make the same intent
  eligible for another broadcast.
- Included transaction and block identifiers survive process restart without
  persisting transaction secrets or proof material.
- Reconciliation latency is bounded and finalized-only; old or reorganized
  anchors fail safely as unresolved rather than authorizing replacement.
- Rejected and expired attempts remain auditable public metadata while allowing
  a freshly planned replacement.
- The journal improves development and headless recovery but does not make
  process-local development custody production-ready or restore expired draft
  signing material.
