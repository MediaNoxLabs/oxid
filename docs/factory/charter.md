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
- Order the backlog and mark items `factory:ready` only after the ready-check
  (fsm.md §Ready) passes.

### Implementer
Claims a `factory:ready` item and delivers a draft PR. Duties:

- Follow the claim/lease protocol (claim-protocol.md) before touching code.
- Work on a branch named `factory/<issue-number>-<slug>`.
- Respect AGENT.md architecture rules and every accepted ADR.
- Deliver through the `.devloops` `draft` gate with fan-out evidence, then the
  `preApproval` gate with CI green.
- Never merge; never push to `integration` directly.

### Reviewer (fan-out)
The `.devloops` refinement fan-out (scope, correctness, coverage,
architecture, security personas) plus `external-review`. Reviewers only ever
produce findings with file/line references; they never edit the branch.

### Quality Steward
A standing role, independent of any single work item. Duties:

- Review `integration` deltas on a schedule; verify
  architecture/security/testing claims against the actual code.
- Measure local target and CI durations against the budgets in metrics.md;
  flag regressions before they hit the CI time bound.
- File confirmed findings as factory work items; never fix-and-push directly.

### Release Manager (human)
The only role that merges, tags, accepts ADRs, or changes repository
settings. Agents stop at `factory:awaiting-human-merge`.

## Authority boundaries

| Action | Who may do it |
| --- | --- |
| Create/refine/order work items | Planner, Quality Steward |
| Claim work, push a `factory/*` branch, open a draft PR | Implementer holding a valid lease |
| Post gate findings | Reviewers |
| Merge, tag, release, change repo settings, accept ADRs | Release Manager (human) only |
| Modify factory protocol docs | Via a normal factory work item + human merge |

## Provider agnosticism

Role-to-model assignment is configuration:

- `.pi/settings.json` — packages and defaults for pi-based participants.
- `.devloops` persona `defaultModel` — per-gate-angle model overrides.
- Other harnesses map roles to their own configuration; the protocol only
  requires that the *evidence* (lease comments, gate findings, CI results) be
  present on GitHub in the documented format.
