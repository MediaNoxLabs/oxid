<!-- SPDX-License-Identifier: Apache-2.0 -->

# Audit Rubric

How a finding is classified, ranked, merged, and capped. This document exists
to make two audits of the same repository produce the same ordering, and to
stop an audit from converting its own volume into backlog noise.

## Severity

The vocabulary is **not new**. It is the gate vocabulary already defined in
[`.pi/agents/review.agent.md`](../../../.pi/agents/review.agent.md), reused
verbatim so a finding can move between a review verdict and an audit report
without translation. Do not introduce a parallel scale.

| Severity | Meaning in an audit |
| --- | --- |
| `must-fix` | A correctness, security, data-integrity, or evidence-integrity defect; or a gate that cannot detect the class it exists to detect. Blocks the release of the audited scope. |
| `worth-fixing-now` | A bounded, high-value improvement that fits the current cycle. Does not block release. |
| `defer` | Real debt with a real cost, deliberately scheduled later. |

Two constraints on `must-fix`. It is reserved for the classes named above, so
polish and taste never reach it; and every `must-fix` is re-verified against
the live repository by someone other than the role that raised it, per the
charter's due-care principle. An unreproduced `must-fix` is downgraded or
withdrawn — never published with a hedge.

## Remediation cost

The second axis, after [SQALE](https://ieeexplore.ieee.org/document/6225997),
which measures technical debt as the cost of reaching conformity rather than as
a count of violations. Cost is the *whole* cost of conformity: the edit, its
test, and its review.

| Cost | Bound | Typical shape |
| --- | --- | --- |
| `minutes` | under ~10 lines, one file, no new test | move a constant, fix a match arm, add a missing flag |
| `hours` | one file or one tight cluster, one new test | a new gate, a corrected policy, a pinned literal |
| `days` | several files, a new abstraction, or a migration with no format change | extract a shared primitive, enforce an existing policy |
| `weeks` | cross-cutting, or a change to a persisted or published format | a custody format migration, a mainline promotion |

Cost is an estimate and is allowed to be wrong. It is recorded anyway, because
a slate sorted by consequence alone hides the fact that the top item is often
also the cheapest — and because a `minutes` fix that is still open after two
audits is evidence about the process, not about the fix.

## Blast radius

A promoter, borrowed from [OpenSSF
Scorecard](https://github.com/ossf/scorecard/blob/main/docs/checks.md)'s
weighting, where the checks with the widest consequence outrank the
informational ones.

> A finding that makes a **gate** unreliable outranks an equally severe finding
> in product code, because it disables detection of an entire class rather than
> causing one defect.

| Radius | Definition |
| --- | --- |
| `class` | Disables or weakens detection for a whole class of defects: an unprotected branch, a check that cannot fail, an unenforced policy, a test that passes without executing. |
| `product` | Affects shipped behaviour or user-held data at one or more sites. |
| `local` | Confined to one call path, one document, or one developer-facing tool. |

The promotion is the point. A `class` finding is frequently cheap and looks
administrative next to a vivid product bug, so an audit ranked on severity
alone reliably buries it. Both of the first two findings of the post-#156
milestone audit were `class`, one of them a single-API-call fact that had gone
unobserved for four days.

## Ranking

Findings sort by, in order:

1. **Severity** — `must-fix`, then `worth-fixing-now`, then `defer`.
2. **Blast radius** — `class`, then `product`, then `local`.
3. **Cost ascending** — `minutes`, then `hours`, then `days`, then `weeks`.
4. **Expiry** — a finding whose remediation cost *rises* if deferred sorts
   above an equal peer. State the expiry and why.
5. **Criterion ID** ascending, as a deterministic tiebreak.

Rule 4 is the only judgment-bearing step and must be justified in the finding.
It exists for a specific and recurring shape: a defect in a format, envelope,
or schema that nothing has yet written with. Before first use it is a constant;
after first use it is a migration. Two audits must produce the same order, so
rules 1–3 and 5 are mechanical and rule 4 requires a stated reason.

### Worked example

Four findings, deliberately chosen so each rule decides one comparison:

| # | Finding | Severity | Radius | Cost | Expiry |
| --- | --- | --- | --- | --- | --- |
| A | Train branch has no protection: no required check, review, or force-push rule | `must-fix` | `class` | `hours` | no |
| B | New seed-bearing envelope version mapped to the legacy key-derivation policy | `must-fix` | `product` | `minutes` | yes — becomes a format migration after first export |
| C | Two declared critical merge checks are published as unconditional success | `must-fix` | `class` | `hours` | no |
| D | Developer benchmark has no deadline or cancellation | `worth-fixing-now` | `local` | `hours` | no |

Resolution: **A, C, B, D**. A and C precede B on radius (rule 2) even though B
is the cheapest, because both disable a detection class while B is one product
defect. A precedes C on criterion ID (rule 5), severity, radius and cost all
being equal. B precedes D on severity (rule 1); its expiry never comes into
play, since rule 4 only breaks ties among peers that rules 1–3 left equal.

The lesson to take from the example is the one that bites: B *feels* like the
most urgent finding — it concerns a master seed — and it is nonetheless third.
An audit that ranks on alarm rather than on this rubric would have led with B
and buried A, which is how a completely unprotected release branch survives
twenty merges.

## The consolidation rule

An audit proposes issues, and every issue costs the reader attention whether or
not it is ever worked. These rules are mechanical so that "is this noise?" is
not re-litigated per finding.

1. **One issue per cause, not per occurrence.** Five endpoint validators with
   five divergent policies is *one* issue naming five sites, because one fix
   resolves all five. Five unrelated defects that happen to share a directory
   are five issues.
2. **Findings that a single edit resolves must merge.** If fixing A necessarily
   fixes B, they are one finding with two symptoms.
3. **A fix under ~10 lines in one file gets no issue.** It rides the audit's
   own remediation pull request, or the next pull request that touches the file.
   An issue whose body is longer than its diff is a net loss.
4. **No issue without an owner-actionable next step.** If nobody can act
   without a decision first, it is an open question in the report's
   decisions-required section, not a backlog item. Filing a question as an
   issue makes the backlog answer it, which it cannot.
5. **A finding that duplicates an open issue is not filed.** It is reported as
   evidence on that issue, with the delta. Check before proposing.
6. **Declare the cap up front.** Default **15** proposed issues per audit.
   Findings beyond the cap collapse into one residual-debt entry listing them
   in rank order. The cap is declared in the audit plan and cannot be raised
   mid-audit.

Rule 6 is the load-bearing one. An uncapped audit reports everything it found,
which transfers the prioritisation work to the reader and, done twice, teaches
the reader to skip audits. A capped audit must decide what matters, which is
the labour the audit exists to perform.

### Consolidation is reported, not silent

The report states how many raw findings the roles produced and how many
proposed issues survived, with the merges named. A reader must be able to see
that forty-one raw findings became fourteen issues and which rule did the work.
Silent consolidation is indistinguishable from not having looked.

## What is not a finding

Recording these keeps roles from spending budget on them:

- **A preference with no defect.** Naming, ordering, or structure that differs
  from the auditor's taste while conforming to every declared criterion.
- **A criterion the repository has explicitly declined.** A documented,
  accepted decision to the contrary is conformance, not violation. If the
  decision looks wrong, that is a proposal to amend the decision record —
  itself a legitimate finding, filed against the record, not the code.
- **A defect in an unmerged branch or an open pull request.** Review owns
  those. Audits examine merged state.
- **A restatement of an open issue.** See rule 5.
- **An absence the audit did not verify.** "There appear to be no tests for X"
  is a finding only once the auditor has established that none exist.
  Otherwise it belongs in the unverified section.

## Finding record

The fields a finding must carry. The closed contract is
[audit-report-v1.schema.json](audit-report-v1.schema.json); this is the
human-readable statement of the same shape.

| Field | Required | Notes |
| --- | --- | --- |
| `id` | yes | Stable within the audit, e.g. `F-07`. |
| `criterion` | yes | The `OXA-*` ID violated. |
| `severity` | yes | `must-fix` \| `worth-fixing-now` \| `defer`. |
| `radius` | yes | `class` \| `product` \| `local`. |
| `cost` | yes | `minutes` \| `hours` \| `days` \| `weeks`. |
| `summary` | yes | One sentence stating the defect, not its consequence. |
| `evidence` | yes | At least one entry: an evidence anchor key, or `path:line`. **Validated.** |
| `failureScenario` | yes for `must-fix` | Concrete inputs or state leading to the wrong outcome. |
| `recommendation` | yes | The fix, concretely enough to estimate. |
| `expiry` | no | Why cost rises if deferred. Required to invoke ranking rule 4. |
| `mergedFrom` | no | Finding IDs consolidated into this one. |
| `duplicateOf` | no | An existing open issue this evidences. |
| `reverifiedBy` | yes for `must-fix` | The role or person who independently reproduced it. |

`summary` states the defect rather than its consequence because consequence
invites inflation. "v4 envelopes seal the master seed under the legacy
derivation policy" is checkable; "attackers could crack user wallets" is not,
and the difference is what keeps severity honest.
