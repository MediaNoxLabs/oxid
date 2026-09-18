<!-- SPDX-License-Identifier: Apache-2.0 -->

# Audit Report Template

The Discussion body an audit publishes. **Section order is fixed** so another
agent can parse the report positionally without interpreting prose, and so a
reader always finds the verdict before the detail and the limitations before
the slate.

Text in `<angle brackets>` is a substitution point. A section may not be
omitted; a section with nothing to report says so explicitly, because "no
findings" and "did not look" must never render identically.

Validate with `node scripts/audit/check-audit-report.mjs <report.json>` before
publishing. The validator checks the fenced block against
[audit-report-v1.schema.json](audit-report-v1.schema.json) and confirms every
finding carries a citation.

A conforming report object is in
[examples/milestone-report.example.json](examples/milestone-report.example.json).
Read it alongside this template: the example is the shape, the template is the
rendering.

---

## Title

```
Audit: <type> — <scope label> — <YYYY-MM-DD>
```

For example: `Audit: milestone — milestone-0.2.0 — 2026-09-09`.

---

## Body

````markdown
# Audit: <type> — <scope label>

<One or two sentences: the objective, in the auditor's own words, and the mode.>

## Anchor

| Field | Value |
| --- | --- |
| Audit type | `<milestone \| security \| supply-chain \| architecture \| process>` |
| Criteria version | `<n>` |
| Mode | `<full \| delta>`<if delta: ` since <prior anchor timestamp>`> |
| Recorded at | `<UTC ISO-8601>` |
| Default branch | `<name>` |
| Primary | `<branch>@<sha>` |
| Comparison | `<branch>@<sha>` |
| Baseline | `<branch>@<sha>` |
| Issue cap | `<n>` |
| Roles | `<role>`, `<role>`, … |

**Excluded from scope.** <What the audit deliberately did not examine.>

## Verdict

<One paragraph. For a milestone audit, state plainly whether the scope is
releasable and name the blocking finding ids. "Unknown" is a permitted verdict
and is itself a finding about observability — if used, say what would resolve
it.>

| Exit question | Answer |
| --- | --- |
| <the type's first exit question> | <answer> |
| … | … |

## Highest-ranked findings

<The top three, at most, each as a short subsection: what it is, the evidence
verbatim, and why it ranks where it does. Everything else goes in the table
below. Three is a ceiling, not a target — publish one if one is what the audit
found.>

### <F-NN> · <severity> · <radius> · <cost> — <headline>

<What it is, in prose a reader who has not opened the repository can follow.>

```
<evidence, verbatim — command and output, or path:line and the lines>
```

<Why it ranks here. If it carries an expiry, state the trigger and the cost
after it fires.>

## Findings

| Rank | ID | Criterion | Sev | Radius | Cost | Finding | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `F-01` | `OXA-…-NN` | `must-fix` | `class` | `hours` | <one sentence stating the defect, not its consequence> | `<path:line>` or `<anchor>` |

<Ordered by the ranking rules in [rubric.md](rubric.md). Every row's Evidence
cell is populated; the validator rejects the report otherwise.>

## Verified sound

<Required, and may not be empty. What was examined and found conforming, with
the criterion and the evidence. This is a coverage statement: without it a
reader cannot distinguish absence of findings from absence of looking, and no
later audit can delta against this one.>

| Criterion | Claim | Sampling | Evidence |
| --- | --- | --- | --- |
| `OXA-…-NN` | <what holds> | <exhaustive, or how much of the population> | `<anchor>` or `<path:line>` |

## Not verified

<Required. Claims the audit could not settle, why, and what would settle them.
Be specific: "no builds, emulators or containers were started" bounds a
conclusion; "did not fully investigate" does not.>

| Claim | Why not | Would require |
| --- | --- | --- |
| <claim> | <reason> | <what would settle it> |

## Delta since `<prior anchor>`

<Required when mode is delta; omit the whole section when mode is full. Every
prior finding appears. Silently dropping one is non-conforming.>

| Prior | Classification | Evidence | Note |
| --- | --- | --- | --- |
| `F-03` | `fixed` | `<path:line>` | |
| `F-07` | `still-present` | `<anchor>` | |
| `F-11` | `withdrawn` | | <whether the finding was wrong or the criterion changed> |

## Consolidation

<Reported, not silent. A reader must be able to see how many raw findings
became how many proposed issues, and which rule did the work.>

| Measure | Count |
| --- | --- |
| Raw findings from roles | <n> |
| Merged by shared cause | <n> |
| Not filed — under ~10 lines, one file | <n> |
| Duplicates of open issues | <n> |
| Collapsed into residual debt | <n> |
| **Proposed issues** | **<n>** |

<Name the notable merges: "five endpoint validators → one issue", and which
existing issues absorbed which findings.>

## Proposed slate

<Nothing here is filed. This is a proposal awaiting owner approval.>

| Rank | Title | Findings | Sev | Cost | Labels |
| --- | --- | --- | --- | --- | --- |
| 1 | <imperative title, ≤100 chars> | `F-01` | `must-fix` | `hours` | `follow`, `audit:<type>`, `P0`, `scope:<x>` |

## Decisions required

<Questions only the owner can settle, with a recommendation each. These are
deliberately not proposed as issues: the backlog cannot answer a question.
Say "none" if none.>

- **<question>** — <recommendation, and what it blocks.>

## Limitations

<Independence caveats. Name any role that examined code it authored, any
criterion skipped and why, and any gate the audit trusted without establishing
that it can fail.>

```json audit-report-v1
{
  "schemaVersion": 1,
  "anchor": { "…": "…" },
  "plan": { "…": "…" },
  "verdict": { "…": "…" },
  "findings": [],
  "verifiedSound": [],
  "notVerified": [],
  "slate": [],
  "consolidation": { "…": "…" }
}
```
````

---

## Rendering rules

- **One source, two outputs.** The prose and the fenced block are rendered from
  the same report object; they may not be authored separately. The validator
  compares finding counts, ids, severities, and costs between them and fails on
  disagreement — a report whose table says `must-fix` and whose block says
  `defer` is worse than either alone.
- **Evidence verbatim.** Quote commands and output as they appeared. A
  paraphrased command is not reproducible, and the reader's ability to re-run it
  is the point.
- **`path:line`, not `path`.** A bare path makes a reader re-derive the
  location, which is exactly the re-derivation the framework exists to remove.
- **No severity in prose that the block contradicts.** Words like "critical" or
  "urgent" in the narrative must correspond to `must-fix` in the data.
- **Title states the defect.** Slate titles are imperative and name the fix
  ("Protect every milestone train by ruleset"), not the symptom.
- **Link, do not restate.** Criteria live in the type documents. A report cites
  `OXA-MIL-03`; it does not reproduce its requirement text.

## Publishing

```bash
# Validate first. Publication without a passing validator run is non-conforming.
node scripts/audit/check-audit-report.mjs tmp/audit/<type>/<anchor>/report.json

# Then publish, on owner request, to the Audits category when one exists and
# General until then.
gh api graphql -f query='...createDiscussion...' -F body=@tmp/audit/<type>/<anchor>/report.md
```

Record the resulting URL in the report's `discussionUrl` and keep the anchor, so
the next audit of this type can delta against it.
