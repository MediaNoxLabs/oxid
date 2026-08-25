# Work-Item Finite State Machine

Factory state is carried by issue/PR labels so that it is visible, auditable,
and machine-readable with nothing but `gh`. Exactly one `factory:*` state
label is present at a time.

```text
                 ready-check
   backlog ───────────────────▶ factory:ready
                                     │ claim (lease posted, assignee set)
                                     ▼
                              factory:claimed
                                     │ branch pushed + draft PR opened
                                     ▼
                            factory:in-progress
                                     │ implementer requests draft gate
                                     ▼
                            factory:gate-draft ── findings ──▶ back to in-progress
                                     │ all draft angles pass + CI green
                                     ▼
                         factory:gate-preapproval ── findings ──▶ back to in-progress
                                     │ all pre-approval angles pass + CI green
                                     │ + fresh Claude current-head evidence posted
                                     ▼
                            factory:merge-ready
                                     │ authorized clean merge
                                     ▼
                                   done ──▶ retrospective note on the issue
```

Failure edges (from any active state):

- `factory:blocked` — unresolvable dependency; the blocking reason is a
  comment; Planner triages.
- **Lease expiry** — item returns to `factory:ready`; the stale branch is
  left in place and referenced in a comment for the next claimant.
- **Abandon** — implementer posts an abandon comment, releases the lease,
  and the item returns to `factory:ready`.

## States

| State | Meaning | Entry condition | Exit |
| --- | --- | --- | --- |
| `backlog` (no label) | Idea or finding, not yet executable | — | Ready-check passes |
| `factory:ready` | Executable by any implementer | Ready-check (below) | Valid claim |
| `factory:claimed` | Lease held, work not yet visible | Lease comment + assignee | Draft PR opened, or lease expiry |
| `factory:in-progress` | Draft PR exists | PR links the issue | Gate request, abandon, or lease expiry |
| `factory:gate-draft` | Fan-out review of the draft | Implementer request | Pass → pre-approval; findings → in-progress |
| `factory:gate-preapproval` | Final delivery gate | Draft gate passed | Pass → merge-ready; findings → in-progress |
| `factory:merge-ready` | Delivery evidence complete | Both gates, CI green, and fresh Claude current-head evidence posted | Owner-authorized clean merge or remediation |
| `factory:blocked` | Cannot proceed | Blocking comment | Planner triage |

## Ready-check

An item may be labeled `factory:ready` only if all of the following hold:

1. Goal is one sentence; acceptance criteria are enumerated and each is
   objectively checkable.
2. Verification commands are listed and runnable from a clean checkout
   (`just`/`run.sh` targets or explicit commands).
3. Out-of-scope list exists (what this slice deliberately does not do).
4. Size class assigned: **S** (≤1 crate touched), **M** (≤3 crates, no new
   ADR), **L** (new ADR or new crate; must link the ADR draft).
5. No unresolved dependency on another open factory item (else `blocked`).

## Gate conditions

Gates reuse the `.devloops` policy verbatim — angles, mandatory angles,
`requireCi: true`, `maxFanoutReviewers`, and the mandatory `external-review`
angle are the source of truth. The FSM only adds the label transitions and the
rule that **gate evidence is a PR comment** containing: angle name, reviewer
identity/harness, findings (or "No findings"), and the CI run link. The owner
policy sets `humanMergeOnly: false`; branch protection intentionally requires
zero hosted approvals, so fresh posted Claude current-head evidence is the
review control before an authorized clean merge.

## Retrospective

After merge, the implementer (or steward) posts a short retrospective
comment: what the gates caught, wall-clock and CI time consumed, and any
protocol friction. The metrics loop aggregates these into metrics.md.
