# Factory Charter — Roles and Authority

Every factory participant acts in exactly one role per work item. Roles are
capability descriptions, not model or vendor names: any agent harness (pi,
Codex, Claude Code, or a future one) can fill any role if it can execute the
role's verification duties on its host machine.

## Roles

### Planner
Turns product intent (blueprint, milestone goals, review findings) into
factory work items. Duties:

- Write items with the `.github/ISSUE_TEMPLATE/factory-work-item.yml`
  structure: goal, acceptance criteria, explicit out-of-scope list, and
  verification commands.
- Keep items **bounded**: one reviewable slice, sized S/M/L (see fsm.md).
- Assign exactly one delivery target: a criteria-backed
  `milestone-<x.y.z>` for product work, or `develop` for factory, harness, CI,
  documentation, dependency, and governance work.
- Order the backlog and mark items `factory:ready` only after the ready-check
  (fsm.md §Ready) passes.

### Implementer
Claims a `factory:ready` item and delivers a draft PR. Duties:

- Follow the claim/lease protocol (claim-protocol.md) before touching code.
- Work on a branch named `<type>/issue-<number>`, using the Conventional
  Commit type that leads the pull-request title and no descriptive suffix.
- Respect AGENT.md architecture rules and every accepted ADR.
- Deliver through the `.devloops` draft gate, exact-head critical CI, and final
  finding triage. Blocking findings return to implementation. Non-critical
  findings move forward only through linked follow-up issues.
- Never push directly to a milestone, `develop`, or `main`. The guarded merge
  path is milestone-only; every `develop` or `main` merge is handed to a human.
  A separate fresh Claude current-head review is required only for high-risk
  work, an owner request, or a disputed finding.

### Reviewer
The bounded `.devloops` route uses one correctness pass at draft and one
security pass at pre-approval. Additional angles require high risk, a disputed
finding, or an explicit owner request. Reviewers only ever produce findings
with file/line references and classify them against the blocking contract;
they never edit the branch. A bounded non-critical finding is advisory once
its follow-up issue and visible PR mapping are recorded.

### Quality Steward
A standing role, independent of any single work item. Duties:

- Review active milestone and `develop` deltas on a schedule; verify
  architecture/security/testing claims against the actual code.
- Measure local target and CI durations against the budgets in metrics.md;
  flag regressions before they hit the CI time bound.
- Run the read-only metrics audit weekly and after a harness incident. Review
  median/p90 and SLO violations monthly; file one bounded issue for each
  confirmed regression instead of tuning the harness inline.
- File confirmed findings as factory work items; never fix-and-push directly.

### Release Manager (human)
Owns milestone lifecycle, tags, releases, ADR acceptance, repository settings,
and every merge to `develop` or `main`. An authorized factory worker may merge
only to `milestone-<x.y.z>` through the exact-head guard. Humans decide when and
in which order concurrent trains promote.

## Authority boundaries

| Action | Who may do it |
| --- | --- |
| Create/refine/order work items and assign one delivery target | Planner, Quality Steward |
| Claim work, push a `<type>/issue-<number>` branch, open a draft PR | Implementer holding a valid lease |
| Post gate findings | Reviewers |
| Merge an exact-head green issue PR to its declared `milestone-<x.y.z>` | Authorized factory worker through the guarded milestone wrapper |
| Merge to `develop` or `main` | Human delivery operator only |
| Create, close, synchronize, or delete a milestone train | Human delivery operator; agents may prepare the issue-backed PR |
| Tag, release, change repo settings, accept ADRs | Release Manager (human) only |
| Modify factory protocol docs | Via a normal factory work item and the same delivery gates |

## Provider agnosticism

Role-to-model assignment is configuration:

- `.pi/settings.json` — packages and defaults for pi-based participants.
- `.devloops` persona `defaultModel` — per-gate-angle model overrides.
- Other harnesses map roles to their own configuration; the protocol only
  requires that the *evidence* (lease comments, gate findings, CI results) be
  present on GitHub in the documented format.
