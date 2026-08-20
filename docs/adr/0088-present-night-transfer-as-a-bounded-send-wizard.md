# ADR-0088: Present NIGHT transfer as a bounded Send wizard

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 12–13, 16, and 18
- Design source: `docs/design/journeys.md` Send journey and `docs/design/rollout.md` Phase 2a
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/headless.rs` and `mobile-bench/wallet-core/src/wallet.rs`
- Tracking: issues #2, #65, and #80
- Implementation state: Dioxus presents the existing protected public/shielded NIGHT transfer lifecycle as a recipient → amount/privacy → review → confirmation/status ceremony

## Context

The reviewed prototype exposes a one-shot unshielded NIGHT transfer through its
headless wallet, while its mobile wallet concentrates on account, identity, and
Passport Vault flows. Oxid already exceeds that behavior: its application and
Midnight adapters prepare an exact public or shielded transfer, retain the
draft, require explicit authorization, prove and submit on a worker, persist
before broadcast, permit acknowledged pre-broadcast cancellation, and block a
blind retry when the outcome is ambiguous.

The existing Dioxus panel places recipient, privacy, amount, preview,
authorization, submission, and recovery in one morphing card. It is functionally
complete but asks several decisions at once and renders the complete seven-row
preview before a human summary. The Phase 2 design calls for a bounded Send
wizard without replacing the reviewed transaction state machine.

The design also names paste, payment-address scanning, and recent recipients.
Oxid currently has no clipboard-import port, payment-address scanner/classifier,
or reviewed recent-recipient repository. Pretending those operations exist in
Dioxus would bypass the platform and persistence boundaries.

## Decision

Keep `TransferPanelState` and every application use case unchanged. Add only
presentation-local state for two editable screens:

1. **Recipient** accepts one bounded address, offers the existing development
   self-address affordance, and proceeds only after non-empty input.
2. **Amount and privacy** accepts exact decimal NIGHT and makes Public versus
   Shielded a visible choice with plain-language consequences. Back returns to
   the recipient without discarding it.

Preparation remains the transition from editable input to the exact application
preview. The review screen leads with one human sentence derived only from that
preview. Network, change, input count, and fee timing remain available in a
collapsed native `details` disclosure.

Continue opens a confirmation sheet that repeats the exact amount and recipient.
Its affirmative action executes the existing authorization command and exact
`SensitiveOperationConfirmation`; device protection remains inside the custody
port. Authorization and submission stay separate. After device authorization,
the same sheet explicitly asks the holder to prove and submit, using the
existing second confirmation. The presentation must not silently submit just
because authorization succeeded.

Submitting is labelled **Sending** and retains the pre-broadcast cancel action.
Inclusion is labelled **Confirmed**. Failure offers only the recovery permitted
by `TransferRecovery`:

- **Edit and try again** for pre-authorization/input failures;
- **Retry safely — nothing was broadcast** when the retained draft is still
  authorized after acknowledged cancellation or a safe failure; or
- **Check with the network** for an ambiguous outcome, pointing to the existing
  durable reconciliation surface and never preparing a replacement.

Paste import, payment QR capture, and persisted recent recipients remain follow-
up work behind focused ports. They are not rendered as inert or misleading
controls in this slice.

## Security and architecture boundaries

- Dioxus remains an incoming adapter. It receives public application views and
  invokes the same use cases; it does not parse Midnight transactions, access
  custody, prove, submit, or reconcile directly.
- Editable strings cannot change the prepared preview. Authorization and
  submission summaries are derived only from that retained preview.
- The worker boundary, 50 ms cancellation polling, persist-before-broadcast
  ordering, and unknown-outcome duplicate prevention are unchanged.
- Public and Shielded are presentation labels for reviewed recipient kinds, not
  claims about anonymity. Exact chain validation remains in the Midnight
  adapter.
- Raw transactions, witnesses, proofs, signatures, key references, opaque
  challenges, cursors, and machine states remain absent from the wizard.
- Simulated and live submission modes keep their existing truthful labels.

## Consequences

- Sending becomes one decision per screen while the core transaction lifecycle
  and its recovery guarantees remain identical.
- Exact review details stay available without competing with the human summary.
- Separate authorization and submission add one deliberate action beyond a
  consumer payment flow that treats biometric approval as implicit submission.
  This is the accepted cost of preserving Oxid's current authority split.
- Clipboard import, payment scanning, and recent-recipient persistence require
  explicit future decisions instead of presentation-layer shortcuts.

## Validation

- Unit tests cover bounded editable steps, preview-derived summary copy, and the
  three closed recovery actions.
- Dioxus copy, CSS vocabulary, design-token, accessibility, and architecture
  gates cover the new presentation.
- iOS XCUITest and Android standalone smoke traverse recipient, amount/privacy,
  review, confirmation, cancellation-safe retry, and confirmed inclusion.

## Rejected alternatives

- Replacing `TransferPanelState` with a UI-owned transaction machine would
  duplicate application authority and regress reviewed recovery semantics.
- Automatically submitting after device authorization would collapse two
  explicit intents and make authorization stronger than the user-visible act.
- Rendering non-functional Paste, Scan, or Recent controls would overstate
  platform capabilities and invite later boundary bypasses.
- Showing the full preview by default would preserve the current cognitive load
  and leave the Phase 2a ceremony incomplete.
