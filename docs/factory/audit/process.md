<!-- SPDX-License-Identifier: Apache-2.0 -->

# Process Audit — `OXA-PRC`

Criteria version: `1`.

## Objective

Establish whether the factory's own gates detect what they claim to detect.

Every other audit type relies on this one. When `OXA-SEC` concludes that an
invariant holds because a gate is green, that conclusion inherits the gate's
reliability. A process audit is therefore the only type whose findings can
invalidate another audit's conclusions retroactively, and the reason
`OXA-ANY-01` is promoted into every type is that no audit should have to wait
for this one to distrust a gate it just used.

The precedent in this repository is
[`pi-runtime-audit.md`](../pi-runtime-audit.md), which examined the agent
runtime rather than the product and found the effective factory unbounded while
every individual component was healthy.

## Scope selector

| Element | Value |
| --- | --- |
| Artifacts | workflows, gate scripts, merge tooling, policy configuration, agent contracts, taskflows, skills, budgets |
| Branches | every mainline, because gate infrastructure diverges and the divergence decides what runs |
| Method | for each gate, determine what state would make it fail, then determine whether that state can reach it |
| Excluded | credentials, session transcripts, exact private telemetry figures, any write, stopping any process |

The method line is the whole type. Reading a gate to confirm it *contains* a
check establishes nothing; the question is whether a failing input reaches the
check and whether the check's verdict reaches the merge decision.

## Criteria

| ID | Requirement | Verification |
| --- | --- | --- |
| `OXA-PRC-01` | Every check a merge tool treats as critical is capable of reporting failure, and the verdict it reports is derived from the check's own outcome. | `gate.cannotFail` anchor. Read each critical context back to the step that publishes it; a hard-coded state, or a step that warns where it should fail, is `must-fix` / `class`. |
| `OXA-PRC-02` | Policy that is computed is enforced. A threshold measured and written to a report while nothing consumes the verdict is not a policy. | `coverage.policyDrift` anchor. Trace the enforcing flag from the workflow to the runner; confirm every declared scope carries a floor. |
| `OXA-PRC-03` | A committed test executes and can fail. It does not return early on unset environment, assert against its own source, or pass without assertions. | Sample committed tests for early returns on environment, self-referential greps, and assertion-free bodies. For a named-behaviour test, delete the behaviour and confirm the test notices. |
| `OXA-PRC-04` | Every committed test is reachable from a target, workflow, or derivation that runs. | Resolve each test file to an invoking target; an unreferenced test file is a finding. |
| `OXA-PRC-05` | Repository access control matches policy: protected mainlines, required review, signature requirements, least-privilege workflow tokens. | `branch.protection` anchor. Maps to `OSPS-AC`. |
| `OXA-PRC-06` | Gate infrastructure is equivalent across mainlines, or each difference is a recorded, justified exception. | `gate.branchCoverage` and `mainline.divergence` anchors. A gate absent from the branch holding the material it governs is a finding. |
| `OXA-PRC-07` | Every path an agent contract instructs an agent to prefer exists in the commit that references it. | Resolve each referenced helper path against the tree and against history. A path that has never been committed is a finding, and an instruction naming one routes the agent to the alternative the same instruction forbids. |
| `OXA-PRC-08` | Agent-facing documentation is factually accurate about gates, thresholds, lanes, and budgets. | For each factual claim in agent-facing docs, read its source of truth. Agent-facing drift is worse than user-facing drift: it is executed, not read. |
| `OXA-PRC-09` | Declared resource and concurrency budgets are respected by the flows that run, without the flow modifying the policy to fit. | Read each taskflow's spawn, concurrency, and fan-out shape against the recorded policy caps; a flow that raises a cap is a finding against the flow. |
| `OXA-PRC-10` | The claim and lease protocol is enforced where it is claimed to be enforced, and human-only merge rules are enforced in code rather than asserted in prose. | Read the guard; confirm it throws rather than warns. |
| `OXA-PRC-11` | Recorded budgets and metrics have current measurements, and a red measurement has an owner and a tracking item. | Read metric baselines against latest recorded values; an indefinitely red metric with no item is a finding. |
| `OXA-ANY-01` | Every gate this audit relied on fails against a known-bad input. | Name the gate, the known-bad state, and the observed result. For this type the criterion is recursive and must be answered for the gates used to check gates. |

### On `OXA-PRC-01`

The concrete shape, recorded because this repository has an instance of it. A
workflow runs two validation steps with `continue-on-error: true`, computes
whether each passed, and then publishes both as commit statuses with `state`
set to a literal success value — the computed result reaching only the
description text. A merge tool lists both contexts among its critical checks
and observes them green on every run. Seven declared critical gates, five of
which can actually go red.

Both halves are individually defensible: advisory validation is a reasonable
choice, and requiring critical checks is a reasonable choice. The defect is
that one file's advisory intent and another file's critical list were never
reconciled, and nothing detects the disagreement. This is the general form —
**process defects live between two files that each look correct.**

### On `OXA-PRC-03`

Three distinct failure modes, which need separate detection:

- **Environment-gated** — the test returns early unless a variable is set, and
  no CI shell sets it. It reports success having executed nothing.
- **Self-referential** — the test greps its own source file for the strings it
  expects, so the needles exist because the test contains them. Deleting the
  implementation leaves it green.
- **Assertion-free** — the test exercises a path and asserts nothing, failing
  only on panic.

For the second, the check is mechanical: confirm the needle exists outside the
test's own file. A sibling test that already applies the correct guard is
strong evidence the author knew, which makes the finding cheap to argue and
cheap to fix.

## Roles

| Role | Angle | Primary criteria |
| --- | --- | --- |
| `gate-integrity` | Critical checks, enforcement reality, branch coverage | `01`, `02`, `06` |
| `test-honesty` | Tests that cannot fail, tests that never run | `03`, `04` |
| `agent-contract` | Referenced paths, agent-doc accuracy, budget conformance | `07`, `08`, `09` |
| `authority-metrics` | Access control, claim protocol, human-only guards, metric currency | `05`, `10`, `11` |

Four roles, two passes of two.

## Collectors

`gate.cannotFail`, `gate.branchCoverage`, `coverage.policyDrift`,
`branch.protection`, `mainline.divergence`.

## Exit questions

1. How many declared critical checks can actually report failure?
2. Which policy is computed but not enforced?
3. Which committed tests cannot fail, and which never run?
4. Which referenced helper path does not exist?
5. Which gate is missing from the branch holding the material it governs?
6. Which taskflow exceeds a declared budget, and did it raise the cap to fit?
7. Which recorded metric has been red longest without an owner?

## A standing caution

This type audits the machinery the auditor runs on, so its conclusions are the
easiest to reach carelessly. Two specific traps.

**Do not treat an absent failure as a working gate.** "Zero failures in one
hundred runs" is equally consistent with a healthy repository and with a check
that cannot fail. Distinguishing them requires reading the publishing step, not
counting outcomes.

**Do not raise a budget to complete the audit.** If a pass does not fit the
caps, split the pass. A process audit that modified the policy it was auditing
has produced no finding and destroyed the evidence.
