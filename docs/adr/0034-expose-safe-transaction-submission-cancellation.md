# ADR-0034: Expose transaction submission status and safe cancellation

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§7–8, 12–13, 16–18 and [issue #19](https://github.com/MediaNoxLabs/oxid/issues/19)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/node/client.rs`, transaction construction/proving services, and the Dioxus worker boundary
- Amends: ADR-0024, ADR-0027, ADR-0028, and ADR-0029
- Implementation state: Oxid-owned submission status/cancel use cases, adapter-owned cooperative control, asynchronous headless lifecycle, and mobile cancel/retry presentation implemented; durable restart reconciliation remains separate work

## Context

ADR-0027 deliberately keeps a draft in `submitting` when an external side
effect may have occurred. It also makes dropping the submit future request
cooperative cancellation while the adapter worker owns the final transition.
That protects against concurrent duplicate sends, but neither the headless nor
Dioxus incoming adapter can intentionally observe or cancel the active attempt.

The prototype waits for a node inclusion result and performs expensive work off
the renderer. Oxid needs that behavior plus an explicit user control. A cancel
button must not imply that a transaction already handed to the node can be
retracted.

## Decision

Keep retained draft lifecycle and submission-attempt lifecycle distinct. The
application owns safe states for not started, running, cancellation requested,
broadcasting, cancelled, included, and outcome unknown. Only public
transaction/block identifiers, final fee, mode, cancellation eligibility, and
retryability may cross the port.

The Midnight adapter retains one control object with each active profile/draft
attempt. Cancellation atomically wins only while the worker is before the
broadcast boundary. The live completer marks that boundary immediately before
calling the node submission operation; the deterministic standalone completer
uses the same boundary. Once broadcasting begins, cancellation returns a safe
`SubmissionCancellationUnsafe` failure and does not set the cancellation flag.

A worker that acknowledges pre-broadcast cancellation restores the retained
draft to `Authorized` and records the attempt as `Cancelled`, making a later
retry explicit. Failures known to precede broadcast remain retryable. An
ambiguous node or worker outcome records `OutcomeUnknown`, keeps the draft
non-retryable, and awaits the later durable reconciliation slice.

Keep the existing blocking `wallet.transaction.submit_unshielded` and
prototype-named `send_unshielded` alias for compatibility. Add
`wallet.transaction.start_submission`, `submission_status`, and
`cancel_submission` for controllable headless flows. The start method validates
human-readable confirmation before spawning work. Dioxus starts the existing
async submit use case, exposes cancellation only while the attempt is safe, and
waits for adapter acknowledgement before offering retry.

## Consequences

- Incoming adapters can exercise cancellation without gaining access to the
  proof, signed transaction, DUST child, or cancellation primitive.
- Draft state alone no longer has to encode whether an authorized draft has
  never run or was safely cancelled; the status boundary carries that fact.
- Cancellation is idempotent while requested or acknowledged, but fails when
  no attempt exists or broadcast safety has been lost.
- The simulated completer includes a short deterministic pre-broadcast window
  solely so headless and Tier-1 mobile harnesses can cover cancel/retry.
- Active attempts remain process-local. Persisted broadcast identity,
  confirmation depth, restart reconciliation, and replacement policy require a
  separate ADR and implementation.
