# How this project is built

Oxid is an experiment in **AI-driven software engineering with owner
authority**: nearly all code is written by AI agents, yet every line answers
to machine-enforced gates, recorded decisions, and explicit owner policy.
This page documents the process as it actually operates.

## The delivery loop

1. **The backlog is the contract.** Work is defined in GitHub issues with
   ordered acceptance criteria (the parity epic,
   [issue #2](https://github.com/MediaNoxLabs/oxid/issues/2), is the
   long-running example). Agents implement exactly the bounded slice an
   issue describes.
2. **A build agent implements slices.** Each slice lands with its tests, its
   documentation updates, and — for any architectural change — a new ADR
   whose *binding status* and *delivery state* are tracked separately in the
   [ADR index](adr-catalog.md).
3. **Gates do not negotiate.** Formatting, architecture allowlists, clippy
   with denied warnings, focused tests, and stable protected contexts run on
   every push. A conservative path classifier escalates sensitive changes to
   the full suite, coverage floor, dependency audits, Nix builds, and scanners;
   nightly validation runs the full hermetic closure. A bounded review policy
   ([`.devloops`](https://github.com/MediaNoxLabs/oxid/blob/integration/.devloops))
   uses scope/correctness at draft and correctness/security at pre-approval.
4. **An independent steward audits the stream.** A second agent reviews
   `integration` deltas on a schedule, measures build/CI budgets, verifies
   security claims against the code, and files findings as issues the build
   agent can pick up. The baseline audit — 11 dimensions, adversarially
   verified findings — is public:
   [Discussion #37](https://github.com/MediaNoxLabs/oxid/discussions/37).
5. **Proportional review evidence precedes a human delivery.** Integration
   branch protection uses zero hosted approvals, while `.devloops` always stops
   at merge. A manually invoked independent Claude CLI review is pinned to the
   exact current head for high-risk work, an owner request, or a disputed
   finding. Releases, repository settings, and ADR acceptance stay human-owned.

## Why it holds together

- **Provenance over trust.** Migrated behavior cites the exact upstream
  commit it was reviewed from
  ([`docs/migration/`](https://github.com/MediaNoxLabs/oxid/tree/integration/docs/migration));
  pinned digests make "the same bytes" a checkable claim.
- **Honesty as a gate.** Capability labels (`deterministic_simulation`,
  `indexer_supplied_not_proven`, `proof_unavailable`) are part of the
  contract; an agent overclaiming a capability fails review the same way a
  failing test does.
- **The steward loop closes.** Findings become issues; issues become fixed
  slices, often within hours — the executor-blocking fix (ADR-0077) and the
  backup-KDF hardening (ADR-0078) both started as steward findings.

## Where this is heading

[Issue #35](https://github.com/MediaNoxLabs/oxid/issues/35) proposes
formalizing this into an **AI Software Factory**: a work-item state machine
carried in labels, a decentralized claim/lease protocol so agents on
different machines never double-work an item, provider-agnostic role
configuration (via [pi](https://pi.dev)), and budgets that flag complexity
growth before it hurts. The protocol drafts live in
[PR #36](https://github.com/MediaNoxLabs/oxid/pull/36).
