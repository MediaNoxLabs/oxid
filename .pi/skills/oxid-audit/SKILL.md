---
name: oxid-audit
description: "Load to run a repository audit: milestone, security, supply-chain, architecture, or process. Produces a validated, unpublished report for owner review."
---

# Oxid repository audit

Read the authoritative [audit charter](../../../docs/factory/audit/README.md),
[rubric](../../../docs/factory/audit/rubric.md), and the criteria document for
the type you are running before collecting anything. This skill owns the
execution shape only; the charter owns what an audit may do and the rubric owns
how a finding is classified.

An audit **reads, proposes, and stops.** It never files, labels, comments,
merges, tags, remediates its own findings, or modifies the policy it audits.
The only mutation is publishing the Discussion, and that waits for the owner.

## Pass structure

The per-session subagent caps in
[`.pi/subagent-policy.json`](../../subagent-policy.json) are four spawns, two
concurrent, `dynamicFanout.maxItems: 2`, sixteen turns and a 120k hard token
ceiling per child. A six-role audit therefore **does not fit one session**, and
the caps are not raised to make it fit — they exist because this host has
frozen from aggregate overcommit.

Run the audit as resumable passes over one anchor directory. The directory is
the audit's state, so an interrupted run resumes without re-collecting.

```
tmp/audit/<type>/<anchor>/
  evidence.json            # pass 0, deterministic, no model
  findings/<angle>.json    # one per role, written by `auditor`
  report.json              # pass N, written by `audit-consolidator`
  report.md
```

| Pass | Spawns | Work |
| --- | --- | --- |
| 0 | 0 | Collect evidence. No model runs. |
| 1 | 2 | Two `auditor` children, one angle each. |
| 2 | 2 | The next two angles. |
| 3 | 2 | Remaining angles, if the type declares more than four. |
| final | 1 | One `audit-consolidator`, then stop at the owner gate. |

Never exceed two spawns in a pass, and never start a pass before the previous
one has written its findings files. Each `auditor` receives the evidence path
and **one** angle — never the orchestrator's conversation, opinions, or another
role's conclusions.

## Pass 0 — collect

```bash
node scripts/audit/collect.mjs \
  --branch milestone-0.2.0 \
  --compare develop --compare main \
  --since 2026-08-26T11:48:00Z \
  --out tmp/audit/milestone/<anchor>/evidence.json
```

Add `--offline` to skip the advisory scan; the collector then reports
`advisory.state` as `degraded` rather than clean, which is the point.

Read the status line it prints to stderr before dispatching any role. A
collector reporting `degraded` or `unavailable` has not told you the fact is
absent — it has told you it could not look, and no role may infer conformance
from it.

Fetch the branches first. The collector reads `refs/remotes/origin/<branch>`,
so an unfetched branch silently reduces coverage:

```bash
git fetch origin develop main 'refs/heads/milestone-*:refs/remotes/origin/milestone-*'
```

## Passes 1..N — judge

Dispatch `auditor` per angle, with the angles listed in the type's criteria
document. Each child writes
`tmp/audit/<type>/<anchor>/findings/<angle>.json`.

If a child exhausts its turn budget, re-dispatch that angle alone rather than
widening another role's scope. Findings derived per-role from re-discovered
facts are exactly what the evidence artifact exists to prevent.

## Final pass — consolidate and validate

Dispatch `audit-consolidator` once, then validate before doing anything else:

```bash
node scripts/audit/check-audit-report.mjs \
  tmp/audit/milestone/<anchor>/report.md \
  --evidence tmp/audit/milestone/<anchor>/evidence.json
```

For a delta audit, add `--prior <previous-report.json>`; the validator then
requires every prior finding to be classified.

Publication without a passing validator run is non-conforming. The validator
enforces the charter's two hard rules — every finding carries a resolvable
citation, and the verified-sound and not-verified sections are present — plus
the cross-references the schema cannot express: that the slate and verdict
point at findings that exist, that the declared cap held, that the ranking does
not invert the rubric, and that the rendered prose agrees with the data block.

## Publication

Only on owner request, and only to the **Audits** category when one exists,
**General** until then. Creating that category is a repository-settings action
outside audit authority.

Issues are filed only on owner approval of the slate, carrying `follow`,
`audit:<type>`, the existing `type:` / `scope:` / priority labels, and a
backlink to the Discussion.

Record the anchor afterwards so the next audit of this type can delta against
it. An audit that does not leave a delta-able anchor makes the next one start
from zero.

## Refusals

- **No evidence artifact, no audit.** If pass 0 produced nothing, stop.
- **No angle, no dispatch.** An `auditor` without a declared angle audits
  everything badly.
- **No cap raise.** The issue cap is declared before the audit runs and cannot
  move mid-audit; overflow collapses into one residual entry.
- **No self-remediation.** Fixing a finding is separate work under the claim
  protocol, with its own review.
- **No budget edit.** If a pass does not fit the caps, split the pass. An audit
  that modified the policy it was auditing has produced no finding and
  destroyed its own evidence.
