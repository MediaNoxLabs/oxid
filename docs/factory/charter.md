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
- Work on a branch named `<type>/issue-<number>`, using the Conventional
  Commit type that leads the pull-request title and no descriptive suffix.
- Respect AGENT.md architecture rules and every accepted ADR.
- Deliver through the `.devloops` `draft` gate with fan-out evidence, then the
  `preApproval` gate with CI green.
- Never push to `develop` directly. Merge only through the guarded
  develop wrapper when the active owner request explicitly authorizes it;
  otherwise stop for a human. A separate fresh Claude current-head review is
  required only for high-risk `full` work, an owner request, or a disputed
  finding.

### Reviewer (fan-out)
The bounded `.devloops` fan-out uses scope/correctness at draft and
correctness/security at pre-approval, with no more than two concurrent
reviewers. Reviewers only ever produce findings with file/line references;
they never edit the branch.

### Quality Steward
A standing role, independent of any single work item. Duties:

- Review `develop` deltas on a schedule; verify
  architecture/security/testing claims against the actual code.
- Measure local target and CI durations against the budgets in metrics.md;
  flag regressions before they hit the CI time bound.
- Run the read-only metrics audit weekly and after a harness incident. Review
  median/p90 and SLO violations monthly; file one bounded issue for each
  confirmed regression instead of tuning the harness inline.
- File confirmed findings as factory work items; never fix-and-push directly.

### Release Manager (human)
Owns tags, releases, ADR acceptance, repository settings, and every `main`
promotion decision. Clean `develop` PRs may be merged by a human, or
by an agent under explicit active owner authorization, after all current-head
gates and any risk-required independent review evidence are posted.

## Authority boundaries

| Action | Who may do it |
| --- | --- |
| Create/refine/order work items | Planner, Quality Steward |
| Claim work, push a `<type>/issue-<number>` branch, open a draft PR | Implementer holding a valid lease |
| Post gate findings | Reviewers |
| Merge a clean `develop` PR | Human delivery operator, or an explicitly owner-authorized agent through the guarded wrapper, after all current-head evidence is posted |
| Merge to `main` | Human delivery operator only |
| Tag, release, change repo settings, accept ADRs | Release Manager (human) only |
| Modify factory protocol docs | Via a normal factory work item and the same delivery gates |

## Provider agnosticism

Role-to-model assignment is configuration:

- `.pi/settings.json` — packages and defaults for pi-based participants.
- `.devloops` persona `defaultModel` — per-gate-angle model overrides.
- Other harnesses map roles to their own configuration; the protocol only
  requires that the *evidence* (lease comments, gate findings, CI results) be
  present on GitHub in the documented format.
