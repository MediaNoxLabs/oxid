# How this project is built

Oxid is an experiment in **AI-driven software engineering with human
authority**: nearly all code is written by AI agents, yet every line answers
to machine-enforced gates, recorded decisions, and a human who owns merges
and releases. This page documents the process as it actually operates.

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
   with denied warnings, the full test suite, a coverage floor, dependency
   audits, and secret-hygiene checks run on every push — the same script
   locally and in CI. A multi-persona review policy
   ([`.devloops`](https://github.com/MediaNoxLabs/oxid/blob/integration/.devloops))
   defines fan-out review angles: scope, correctness, coverage,
   architecture, security.
4. **An independent steward audits the stream.** A second agent reviews
   `integration` deltas on a schedule, measures build/CI budgets, verifies
   security claims against the code, and files findings as issues the build
   agent can pick up. The baseline audit — 11 dimensions, adversarially
   verified findings — is public:
   [Discussion #37](https://github.com/MediaNoxLabs/oxid/discussions/37).
5. **Humans keep the irreversible parts.** Merge authority, releases,
   repository settings, and ADR acceptance stay human
   (`humanMergeOnly: true`).

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
