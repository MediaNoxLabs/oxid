<!-- SPDX-License-Identifier: Apache-2.0 -->

# Milestone Audit — `OXA-MIL`

Criteria version: `1`.

## Objective

Answer two questions about one milestone train: **is it releasable**, and
**what debt did it accumulate**? Both, together — a train that passes every
gate while leaving the default branch carrying a defect it already fixed is not
releasable in any useful sense, and a debt inventory with no release verdict is
not an audit of a milestone.

This type runs a bounded subset of the four domain types' criteria against one
train and cites their IDs rather than restating them. A finding whose natural
home is `OXA-SEC` or `OXA-ARC` keeps that ID.

## Scope selector

| Element | Value |
| --- | --- |
| Primary branch | the named `milestone-<x.y.z>` train |
| Comparison branches | the default branch, and `main` |
| Window | merge-base of train and default branch, to the tip of each |
| Unit | merged pull requests and the merged tree; never open branches |
| Excluded | open pull requests, unmerged branches, untracked files, credentials, session transcripts, exact private telemetry figures |

The window is stated as SHAs in the anchor, not as dates. A date window is not
reproducible once branches move.

## Criteria

Each criterion is a requirement plus the method that verifies it. A finding
cites the ID.

| ID | Requirement | Verification |
| --- | --- | --- |
| `OXA-MIL-01` | Every branch receiving product work carries the protection contract documented in [`issue-branch-delivery.md`](../../issue-branch-delivery.md): required checks, review requirement, and no force-push or deletion. | `branch.protection` anchor, compared against the documented contract for each train, default branch, and `main`. |
| `OXA-MIL-02` | Every gate workflow that governs the default branch also runs on the train, or its absence is a recorded, justified exception. | `gate.branchCoverage` anchor; diff each workflow's branch filter against the train name. |
| `OXA-MIL-03` | Every check a merge tool treats as critical is capable of reporting failure. | `gate.cannotFail` anchor. See `OXA-PRC-01`; a violation here is always `must-fix` / `class`. |
| `OXA-MIL-04` | No defect fixed on the train remains present on the default branch without a tracked promotion work item. | `mainline.divergence` anchor plus the fix commits' touched paths, re-read on the default branch. |
| `OXA-MIL-05` | Every issue whose fix has merged is closed, or the closure gap is tracked with its cause. | `issue.closureGap` anchor. Closing keywords fire only on default-branch merges; a train merge never closes anything. |
| `OXA-MIL-06` | Release metadata — workspace version, changelog, tag — reflects the train's contents. | Read manifest version and changelog against `pr.census`; list tags. |
| `OXA-MIL-07` | Merges into the train carry independent review evidence, and the tool-enforced human-only rules are actually enforced in code. | `pr.census` review counts; read the merge wrapper's guard. |
| `OXA-MIL-08` | Quality policy that exists is enforced rather than merely computed. | `coverage.policyDrift` anchor; confirm the enforcing flag reaches the runner from a workflow. |
| `OXA-MIL-09` | Documentation claims about the train's behaviour, gates, and thresholds match the code. | Read each claim's source of truth; report the pair. |
| `OXA-MIL-10` | The decision-record corpus is internally consistent across both mainlines. | `adr.collisions` anchor. See `OXA-ARC-04`. |
| `OXA-MIL-11` | Files differing between mainlines differ intentionally, and cross-train targeting follows the documented rule. | `mainline.divergence` anchor; classify each differing path as intended or stranded. |
| `OXA-MIL-12` | Tests introduced in the window can fail. | Sample new test files for tests that return early on unset environment, grep their own source, or assert nothing. See `OXA-PRC-03`. |
| `OXA-MIL-13` | Every committed test is reachable from a target, workflow, or derivation that runs. | Resolve each new test file to an invoking target; unreferenced files are findings. |
| `OXA-ANY-01` | Every gate this audit relied on to conclude soundness fails against a known-bad input. | Name the gate, the known-bad state, and the observed result. |

`OXA-MIL-12` and `OXA-MIL-13` are separated deliberately. A test that runs but
cannot fail and a test that never runs at all are different defects with
different fixes, and an audit that merges them reports neither clearly.

## Roles

Six roles, split into bounded passes sized from the live
`.pi/subagent-policy.json` limits. The supervisor must stay within the current
per-session, per-run, and global-concurrency caps. Each child receives the
evidence artifact and its angle only.

| Role | Angle | Primary criteria |
| --- | --- | --- |
| `release-readiness` | Protection, gates, promotion, closure, release metadata | `01`–`07` |
| `security-custody` | Key derivation, seed handling, consent paths, trust anchors | `OXA-SEC-*` |
| `architecture-boundaries` | Boundary conformance, façade ratchets, state modelling | `OXA-ARC-*` |
| `reliability-concurrency` | Admission control, deadlines, cancellation, error discard | `OXA-ARC-05`, product code |
| `tests-ci-release` | Test enforcement, reachability, coverage, budgets | `08`, `12`, `13` |
| `docs-adr-consistency` | Documentation truth, decision-record integrity | `09`, `10` |

Pass assignment is recorded in the taskflow, not here, so the split can change
with the caps without a criteria-version bump.

## Collectors

`branch.protection`, `pr.census`, `gate.branchCoverage`, `gate.cannotFail`,
`coverage.policyDrift`, `adr.collisions`, `facade.headroom`, `advisory.state`,
`mainline.divergence`, `issue.closureGap`.

All ten. A milestone audit is the only type that consumes the full evidence
artifact, which is why the collector's anchor set is defined by this type.

## Exit questions

The report answers each explicitly. "Unknown" is a permitted answer and is
itself a finding about the train's observability.

1. Can this train be released today? If not, name the blocking findings by ID.
2. Does the default branch carry any defect this train has already fixed?
3. Which issues are closable now, and which require the promotion first?
4. Which gates did this audit trust, and which of those were shown able to fail?
5. What did the window's churn cost — what is the fix-to-feature ratio, and
   does it indicate repair or delivery?
6. What could not be verified read-only, and what would be needed to verify it?

## Notes from the first run

The audit of the window after issue #156 exercised these criteria before they
were written, and two lessons are recorded here so they are not relearned.

The two highest-ranked findings were both `class` radius and both cheap:
a train that had taken twenty product merges with no protection whatsoever,
and two declared critical checks published as unconditional success. Neither
was discoverable by reading product code; both fell out of a single API query
and one workflow file. The mechanical layer exists because of this.

`OXA-MIL-05` was nearly reported backwards. The closure gap was first
attributed to the default branch being `main`, which had been true weeks
earlier and was no longer. The anchor now records the default branch as a
collected fact rather than as auditor knowledge, because a stale premise
produces confident, wrong findings faster than no premise at all.
