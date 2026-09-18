<!-- SPDX-License-Identifier: Apache-2.0 -->

# Audit Charter

An audit is a bounded, evidence-bearing examination of the repository against
declared criteria, producing a triaged slate rather than a grade. This charter
defines what an audit is, what it may and may not do, and how any conforming
worker — Pi, another agent runtime, or a human — reproduces one.

Audits complement rather than replace pull request review.
[`.pi/agents/review.agent.md`](../../../.pi/agents/review.agent.md) owns the
verdict on a single change. An audit owns what no single change is accountable
for: accumulated debt, systemic drift, and gates that stopped working.

| Document | Contents |
| --- | --- |
| [rubric.md](rubric.md) | Severity, remediation cost, blast radius, ranking, the consolidation rule, and the issue cap. |
| [milestone.md](milestone.md) | `OXA-MIL-*`: release readiness and accumulated debt for one train. |
| [security.md](security.md) | `OXA-SEC-*`: baseline control conformance, custody and crypto invariants, consent-path integrity. |
| [supply-chain.md](supply-chain.md) | `OXA-SUP-*`: advisories, pin discipline, provenance, repository posture. |
| [architecture.md](architecture.md) | `OXA-ARC-*`: boundary conformance, façade ratchets, decision-record corpus health. |
| [process.md](process.md) | `OXA-PRC-*`: the factory's own gates, enforcement reality, and agent-contract integrity. |
| [report-template.md](report-template.md) | The Discussion body an audit publishes, including its machine-readable block. |
| [audit-evidence-v1.schema.json](audit-evidence-v1.schema.json) | Closed contract for mechanically collected facts. |
| [audit-report-v1.schema.json](audit-report-v1.schema.json) | Closed contract for a published audit report. |
| [examples/](examples/) | A conforming report and evidence artifact, drawn from the first milestone audit. Worked reference, and the fixtures the validator is tested against. |

## The triad

Every audit declares three things before it begins, and publishes them in its
report. An audit that cannot state all three is not scoped and must not run.

- **Objective** — the question the audit answers. Not "look at the repository"
  but, for example, "is `milestone-0.2.0` releasable, and what debt did it
  accumulate?"
- **Scope** — the exact commits, branches, paths, and time window examined,
  pinned as an anchor. Scope states what was excluded as explicitly as what
  was included.
- **Criteria** — the declared, citable requirements conformance is judged
  against. Criteria are versioned; a finding cites the criterion it violates.

## Principles

Six principles bind an audit. Four are ordinary professional conduct; two are
hard rules that the validator and the report template enforce mechanically,
because in practice they are the ones that lapse.

1. **Evidence-based.** *Hard rule.* Every finding cites either an evidence
   anchor from the collected artifact or a `file:line` the auditor read. A
   finding that cites neither is rejected by
   `scripts/audit/check-audit-report.mjs`,
   not by reviewer taste. Sampling is stated wherever the audit sampled.
2. **Fair presentation.** *Hard rule.* Completeness is part of accuracy. A
   report that lists only findings is non-conforming: what was examined and
   found sound, and what could not be verified, are required sections. Without
   them, coverage is unknown, a reader cannot tell absence of findings from
   absence of looking, and no later audit can run a delta.
3. **Independence.** Each judgment role runs in fresh context and receives the
   evidence artifact plus its own angle — never the orchestrator's conversation,
   opinions, or prior conclusions. An auditor does not audit its own
   remediation; if a role authored the code under examination, it says so in
   the report's limitations.
4. **Due care.** Findings above `worth-fixing-now` are re-verified against the
   live repository before publication, independently of the role that raised
   them. An unreproduced high-severity claim is downgraded or withdrawn, not
   published with a hedge.
5. **Risk-based.** Depth follows consequence. The audit spends its budget where
   failure is expensive and detection is weak, and says where it chose not to
   look.
6. **Proportionality.** An audit is bounded by the cap it declared. It reports
   the most consequential findings that fit, not every finding it can produce.
   Volume is not thoroughness; see [rubric.md](rubric.md).

## Three layers

Judgment must never re-derive fact. The separation below is the reason an audit
is reproducible at all, and it is also what makes the audit affordable: a role
that has to discover repository facts for itself exhausts its turn budget
before reaching a finding.

### Layer 1 — mechanical evidence

`scripts/audit/collect.mjs` is
deterministic, read-only, and free of any language model. It emits an
`audit-evidence-v1` artifact in which every fact carries a stable anchor key.

The inclusion rule: **anything two runs must agree on belongs here.** Branch
protection posture, pull request census, coverage floors versus documented
claims, decision-record collisions, façade headroom, advisory classes, which
gate runs on which branch. If two auditors could reasonably disagree about a
fact, it is not a fact and does not belong in this layer.

Two consecutive runs at one anchor produce byte-identical output apart from the
generation timestamp. This is a tested property, not an intention.

### Layer 2 — judgment

Each role receives the same evidence artifact plus one angle, holds read-only
tools, and returns an `audit-findings-v1` artifact at a deterministic path.
Roles are declared per audit type. A role reads source to interpret and to
confirm, and records any file it opened beyond its briefing.

### Layer 3 — consolidation and publication

One consolidator fans in every role's findings, deduplicates across roles,
applies the consolidation rule, ranks, and renders **one** source into both the
Discussion body and, when asked, a review artifact. It then stops at the owner
gate.

## Audit types

| ID prefix | Type | Objective |
| --- | --- | --- |
| `OXA-MIL` | [milestone](milestone.md) | Is this train releasable, and what debt did it accumulate? |
| `OXA-SEC` | [security](security.md) | Do the security invariants the product claims actually hold? |
| `OXA-SUP` | [supply-chain](supply-chain.md) | Is what we ship made of what we think, from sources we trust? |
| `OXA-ARC` | [architecture](architecture.md) | Do the declared boundaries still constrain the code? |
| `OXA-PRC` | [process](process.md) | Do the factory's own gates detect what they claim to detect? |

Types compose: a milestone audit runs a bounded subset of the other four types'
criteria against one train, and cites their IDs rather than restating them.
Running a full domain audit is a separate, deeper act.

One criterion is promoted into **every** type, from this repository's own most
expensive lesson — recorded in [the factory README](../README.md) as *a check
that cannot fail is worse than no check*:

> **OXA-ANY-01.** For each gate this audit relied upon to conclude that
> something is sound, does that gate fail against a known-bad input? Name the
> gate, the known-bad state, and the observed result.

An audit that relies on a green gate without ever establishing that the gate
can go red has not verified the thing; it has verified that nothing objected.

## Anchors and delta audits

Every audit pins an **anchor** and publishes it: the audit type, the criteria
version, a UTC timestamp, and the exact commit SHA of every branch examined.

The next audit of the same type defaults to a **delta** since the last
published anchor of that type, and must classify every prior finding:

| Classification | Meaning |
| --- | --- |
| `fixed` | Evidence shows the finding no longer holds. |
| `still-present` | Reproduced unchanged. |
| `regressed` | Was fixed, and has returned. |
| `withdrawn` | The finding was wrong, or its criterion changed. Say which. |

A delta that silently drops a prior finding is non-conforming. This is what
makes the framework compound instead of restarting, and it mirrors the
delta-review contract already in
[`review.agent.md`](../../../.pi/agents/review.agent.md).

A **full** audit runs when no prior anchor exists for the type, when the
criteria version changed, or when the owner asks for one. It says which.

## Authority

An audit is an instrument of observation. Its authority is bounded accordingly.

| Act | Permitted |
| --- | --- |
| Read any tracked file, branch, workflow, issue, or pull request | yes |
| Run read-only local commands and read-only GitHub queries | yes |
| Publish the report as a Discussion | yes, on owner request |
| Propose an issue slate | yes, in the report |
| Create, label, or comment on issues and pull requests | **no** — owner approval, per slate |
| Modify any tracked file, including remediating its own findings | **no** |
| Merge, tag, promote, or change settings, protection, or rulesets | **no** |
| Modify `.pi/subagent-policy.json` or `.pi/settings.json` | **no** |
| Stop a running process, delete a worktree, or remove build output | **no** |

The gate on filing is deliberate rather than cautious. An audit that writes to
the backlog unattended converts every false positive into permanent noise, and
the volume that makes an audit feel productive is exactly what makes the
backlog unusable. The owner cuts the slate; the audit argues for it.

Two further boundaries follow from repository policy rather than from this
charter. Only `MediaNoxLabs` repositories are writable at all, so an audit
never publishes to an upstream repository; and an audit reads no credentials,
no session transcripts, and no private telemetry, reporting operational
quantities as posture rather than exact figures where
[`pi-runtime-audit.md`](../pi-runtime-audit.md) established that precedent.

## Publication protocol

1. The consolidator renders the report from
   [report-template.md](report-template.md). Section order is fixed so other
   agents can parse it positionally.
2. The report carries a fenced ` ```json audit-report-v1 ` block conforming to
   [audit-report-v1.schema.json](audit-report-v1.schema.json). This block is
   the machine-readable contract; the prose above it is for humans and the two
   must agree, which the validator checks.
3. `scripts/audit/check-audit-report.mjs` must pass before publication.
4. The audit is published as a Discussion in the **Audits** category when one
   exists, and **General** until then. Creating that category is a
   repository-settings action outside audit authority.
5. The report states its proposed slate and stops. On owner approval, issues
   are filed carrying `follow`, the existing `type:` / `scope:` / priority
   taxonomy, an `audit:<type>` label, and a backlink to the Discussion.
6. The anchor is recorded so the next audit of that type can delta against it.

## Invocation

```bash
# Layer 1: collect mechanical evidence at an anchor.
node scripts/audit/collect.mjs --type milestone --branch milestone-0.2.0 \
  --since <anchor-sha> --out tmp/audit/milestone/<anchor>/evidence.json

# Layer 3: validate a rendered report before publishing it.
node scripts/audit/check-audit-report.mjs tmp/audit/milestone/<anchor>/report.json
```

Under Pi, `.pi/skills/oxid-audit/SKILL.md`
wraps the sequence as `/audit <type> [--since <anchor>]`.

Artifacts live under `tmp/audit/<type>/<anchor>/` — evidence, one findings file
per role, and the rendered report. The directory is the audit's state: a run
interrupted at any point resumes from it without re-collecting, which is a
requirement rather than a convenience, for the reason in the next section.

## What refuses to work by design

- **A six-role audit in one Pi session.** `.pi/subagent-policy.json` caps four
  spawns per session, two concurrent, `dynamicFanout.maxItems: 2`, sixteen
  turns and a 120k hard token ceiling per child. These caps exist because this
  host has frozen from aggregate overcommit; they are not raised to fit an
  audit. Audits run as resumable passes over the artifact directory instead,
  which is also why the directory, not a session, holds the state.
- **A finding without a citation.** The validator rejects it. This blocks
  well-meant, true observations that the auditor did not evidence, and that is
  the intended cost.
- **An audit that reports only problems.** Missing sound-and-unverified
  sections fail validation, because a findings-only report makes coverage
  unknowable and the next delta impossible.
- **Self-remediation.** An audit cannot fix what it finds. Remediation is a
  separate work item under the normal claim protocol, with its own review.
- **An uncapped slate.** Exceeding the declared cap collapses the remainder
  into a single residual-debt entry. The cap cannot be raised mid-audit.
