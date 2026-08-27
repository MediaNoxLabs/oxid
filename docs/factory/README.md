# AI Software Factory

The factory is Oxid's formalized agent-driven delivery system, proposed in
[issue #35](https://github.com/MediaNoxLabs/oxid/issues/35). The repository
itself is the coordination plane: issues are the backlog, pull requests are
work units, labels carry finite-state-machine state, and checks plus recorded
current-head review evidence are the delivery gates.

| Document | Contents |
| --- | --- |
| [charter.md](charter.md) | Roles, responsibilities, and authority boundaries. |
| [fsm.md](fsm.md) | The work-item finite state machine: states, transitions, gate conditions, failure edges. |
| [claim-protocol.md](claim-protocol.md) | Decentralized claim/lease protocol so agents on different machines never double-work an item. |
| [metrics.md](metrics.md) | The measurements the factory watches and the current baselines. |
| [ci-target-matrix.md](ci-target-matrix.md) | Assurance levels, component routing, branch profiles, target dependencies, budgets, and promotion gaps. |
| [runbook.md](runbook.md) | Phase 1 operations: what is installed, the three concurrency mechanisms, the label profile, and what refuses to work by design. |
| [productive-loop.md](productive-loop.md) | Time-to-main SLOs, target routing, review limits, and worktree/disk lifecycle. |

Tooling lives in [`.pi/extensions/factory.ts`](../../.pi/extensions/factory.ts)
(a [pi](https://pi.dev) repo-local extension) so any engineer or agent with the
repository checkout and a `gh` login can participate — from any machine, with
any LLM provider.

## Design constraints

1. **Zero behavior change until opted in.** The factory formalizes around the
   existing development flow; a work item enters the factory only when it
   carries a `factory:*` label.
2. **No coordination infrastructure.** GitHub is the lock service, the queue,
   and the audit log. Any agent that can run `gh` can participate.
3. **Provider-agnostic.** Roles reference capabilities, never a specific LLM.
   Model selection is configuration (`.pi/settings.json`, `.devloops`
   persona `defaultModel`), not process.
4. **Proportional evidence before a human merge.** Stable required aggregators
   run the impact-planned hosted targets. Independent Claude CLI
   current-head evidence is added for high-risk work, an owner request, or a
   disputed finding. `.devloops` always stops at merge; release, settings, and
   ADR authority remain human.

## Historical field experience, 2026-08-18 to 2026-08-20

The protocol in these documents was exercised before it was accepted, by two
agents working the same repository in parallel. During that recorded period,
`develop` was the active delivery branch: a build agent delivered the feature
backlog there while a quality steward reviewed that stream and executed
separate backlog items on branches. Current work instead targets `integration`
as defined by [the delivery authority](../integration-delivery.md). What held
up and what did not is recorded here so the proposal is judged on evidence
rather than intent.

**What worked.**

- *Issues as the coordination plane.* Steward findings filed as issues were
  picked up and fixed by the build agent without any direct channel between
  the two agents — eight of them inside two days, including a manifest
  truthfulness gap and a UI-thread blocking defect. The backlog was sufficient
  coordination; no shared state was needed.
- *Claim leases.* Before executing a backlog item the steward posted a lease
  comment naming the worker, the branch, and an expiry, and released it
  publicly when it abandoned an item it could not finish. No item was
  double-worked.
- *Hot-file avoidance.* Choosing items outside the other agent's active files
  (`crates/ui-dioxus/src/lib.rs`, `AGENT.md`, `crates/adapters/midnight`)
  produced zero merge conflicts across five merged pull requests, while a
  sixth deliberately stopped at the boundary of an actively-edited crate.
- *Gate evidence in the pull request.* Every steward pull request stated the
  command run and its output; twice this caught the author's own error before
  review.

**What needed correcting.**

- *Numbered proposals collide.* A decision record that sits in review takes a
  number the other agent then also takes: one proposal collided three times in
  36 hours. Proposals now stay unnumbered until they are accepted
  (`draft-<slug>.md`), and `scripts/check-adr-links.sh` enforces it.
- *Shared checkouts do not work.* An implementation agent dispatched into the
  steward's own worktree left uncommitted changes on the wrong branch. Parallel
  workers need separate worktrees, and `git add` with explicit paths rather
  than `-A`.
- *Merge cadence hides verification.* During that historical `develop` period,
  merging five pull requests in quick succession made `cancel-in-progress`
  cancel each intermediate run, so those commits carry no completed verdict
  even though every pull request was green before merge. This lesson now
  applies to `integration`: space merges, or state plainly that verification
  is tip-only.
- *A check that cannot fail is worse than no check.* A first version of the
  decision-record lint used `sed` alternation that BSD `sed` ignores: it
  reported a clean corpus while matching nothing. Prove a new gate fails
  against the known-bad state before landing it.
