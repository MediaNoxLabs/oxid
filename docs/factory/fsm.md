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
                            factory:gate-draft
                              │               │
                    blocking finding    non-critical finding
                              │               │ linked follow-up issue
                              ▼               ▼
                         in-progress    factory:gate-preapproval
                                              │ exact-head critical CI
                                              │ + complete finding triage
                                     ▼
                            factory:merge-ready
                              │                        │
                    guarded milestone merge     human develop/main merge
                                     ▼
                                   done ──▶ bounded PR closeout comment
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
| `factory:gate-draft` | Bounded direction review | Implementer request | Blocking finding → in-progress; otherwise → pre-approval with follow-up links |
| `factory:gate-preapproval` | Exact-head critical gate and final finding triage | Draft gate classified | Blocking finding → in-progress; critical CI and triage complete → merge-ready |
| `factory:merge-ready` | Critical delivery evidence complete | Exact-head required CI green, every finding classified, every deferral linked, and any risk-required evidence posted | Guarded milestone merge, human durable-branch merge, or remediation |
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
5. Delivery target is explicit: one existing `milestone-<x.y.z>` for product
   work, or `develop` for eligible factory work. A worker never infers it.
6. No unresolved dependency on another open factory item (else `blocked`).

## Gate conditions

Gates reuse `.devloops` for angles, critical classifications, draft
`requireCi: false`, final CI, and `maxFanoutReviewers`. Gate evidence is a PR
comment containing the angle, reviewer identity/harness, findings (or “No
findings”), classification, linked follow-up issue where applicable, and CI
run. Review agreement is not a second merge authority: only critical required
contexts and blocking findings stop the current increment.

The milestone wrapper rechecks the exact head, declared milestone base,
conflict-free merge tree, required checks, and complete finding triage. It may
merge only to `milestone-<x.y.z>`. Humans alone merge milestone promotions and
direct factory PRs to `develop`, and promote `develop` to `main`. Fresh Claude
current-head evidence is added for high-risk work, an owner request, or a
disputed classification.

The blocking/follow-up boundary is closed in
[`issue-branch-delivery.md`](../issue-branch-delivery.md). An agent cannot
defer security, privacy, custody, cryptographic, data-integrity, accepted
architecture-invariant, changed-capability correctness, compilation,
critical-test, authenticity, freshness, conflict, or secret-exposure findings.
Other bounded findings are
mergeable only after the follow-up issue and mapping comment exist.

## Retrospective and closeout

Every work item leaves one bounded PR closeout comment stating whether the
private final-head metric record was captured, whether an incident or SLO miss
occurred, and which follow-up issue owns any confirmed regression. This is the
routine retrospective and requires no additional model call. A deeper
retrospective is required after an incident, an SLO miss, high-risk delivery,
or an owner request; it records what the gates caught, wall-clock and CI time,
and protocol friction without publishing private raw telemetry.
