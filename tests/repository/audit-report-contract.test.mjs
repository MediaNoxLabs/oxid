// SPDX-License-Identifier: Apache-2.0

// Contract tests for the audit framework's durable contracts: the report
// validator, both schemas, and the factual claims the framework's own
// documents make about the runtime policy they cite.
//
// Every test here is a known-bad fixture the validator must reject. The
// factory's rule is that a gate is proven against known-bad state before it
// lands, and the framework these fixtures guard defines a criterion
// (OXA-PRC-03) forbidding tests that cannot fail — so each assertion names the
// clause it exercises and mutates a conforming fixture rather than hand-rolling
// an invalid document that might fail for an unrelated reason.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { assertSupported, validate } from "../../scripts/audit/lib/json-schema.mjs";
import { checkReport, crossCheck, extractReportBlock, findingIdsInProse, findingsInProse } from "../../scripts/audit/check-audit-report.mjs";

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const AUDIT_DOCS = path.join(ROOT, "docs", "factory", "audit");

const readJson = (relative) => JSON.parse(readFileSync(path.join(AUDIT_DOCS, relative), "utf8"));
const reportSchema = readJson("audit-report-v1.schema.json");
const evidenceSchema = readJson("audit-evidence-v1.schema.json");
const exampleReport = () => readJson("examples/milestone-report.example.json");
const exampleEvidence = () => readJson("examples/milestone-evidence.example.json");

test("both shipped schemas use only keywords the validator enforces", () => {
  // A schema keyword nothing enforces is a constraint the author believes they
  // wrote and does not have: the same defect class as a check that cannot fail.
  assert.deepEqual(assertSupported(reportSchema), [], "report schema has unenforced keywords");
  assert.deepEqual(assertSupported(evidenceSchema), [], "evidence schema has unenforced keywords");
});

test("the shipped example artifacts conform to their schemas", () => {
  assert.deepEqual(validate(reportSchema, exampleReport()), []);
  assert.deepEqual(validate(evidenceSchema, exampleEvidence()), []);
});

test("the example report passes the full validator, cross-references included", () => {
  const result = checkReport(exampleReport(), { evidence: exampleEvidence() });
  assert.deepEqual(result.schemaErrors, []);
  assert.deepEqual(result.crossErrors, []);
  assert.equal(result.ok, true);
});

// --- the framework's one hard mechanical rule --------------------------------

test("a finding citing no evidence is rejected", () => {
  const report = exampleReport();
  report.findings[0].evidence = [];
  const errors = validate(reportSchema, report);
  assert.ok(
    errors.some((error) => error.path.startsWith("findings[0].evidence") && /at least 1 item/u.test(error.message)),
    `expected an evidence minItems violation, received ${JSON.stringify(errors)}`,
  );
});

test("an evidence entry with neither an anchor nor a path is rejected", () => {
  const report = exampleReport();
  report.findings[0].evidence = [{ line: 5 }];
  const errors = validate(reportSchema, report);
  assert.ok(errors.some((error) => /exactly one/u.test(error.message)), JSON.stringify(errors));
});

test("a citation naming an anchor no collector produced is rejected", () => {
  // Worse than an uncited finding: it reads as verified.
  const report = exampleReport();
  report.findings[0].evidence = [{ anchor: "gate.vibes" }];
  const problems = crossCheck(report, { evidence: exampleEvidence() });
  assert.ok(
    problems.some((problem) => /unknown evidence anchor "gate\.vibes"/u.test(problem)),
    JSON.stringify(problems),
  );
});

test("an anchor citation without a supplied evidence artifact is rejected", () => {
  const problems = crossCheck(exampleReport());
  assert.ok(problems.some((problem) => /no evidence artifact was supplied/u.test(problem)));
});

test("a citation setting both line and lines is rejected", () => {
  const report = exampleReport();
  report.findings[0].evidence = [{ path: "a.rs", line: 3, lines: "3-9" }];
  assert.ok(crossCheck(report, {}).some((problem) => /both line and lines/u.test(problem)));
});

test("a bare path citation without a line range is rejected", () => {
  const report = exampleReport();
  report.findings[0].evidence = [{ path: "a.rs" }];
  assert.ok(crossCheck(report, {}).some((problem) => /must set exactly one of line or lines/u.test(problem)));
});

// --- fair presentation -------------------------------------------------------

test("a findings-only report is rejected for an empty verifiedSound", () => {
  const report = exampleReport();
  report.verifiedSound = [];
  const errors = validate(reportSchema, report);
  assert.ok(errors.some((error) => error.path === "verifiedSound" && /at least 1 item/u.test(error.message)));
});

test("a report omitting notVerified entirely is rejected", () => {
  const report = exampleReport();
  delete report.notVerified;
  const errors = validate(reportSchema, report);
  assert.ok(errors.some((error) => /missing required property "notVerified"/u.test(error.message)));
});

// --- due care ----------------------------------------------------------------

test("a must-fix finding without a failure scenario and re-verification is rejected", () => {
  const report = exampleReport();
  delete report.findings[0].failureScenario;
  delete report.findings[0].reverifiedBy;
  const errors = validate(reportSchema, report);
  const messages = errors.map((error) => error.message).join("|");
  assert.match(messages, /failureScenario/u);
  assert.match(messages, /reverifiedBy/u);
});

test("the same omissions on a defer finding are accepted", () => {
  // The conditional must be conditional: proving the negative case stops the
  // requirement from silently applying to every severity.
  const report = exampleReport();
  report.findings[0].severity = "defer";
  report.verdict.blocking = report.verdict.blocking.filter((id) => id !== report.findings[0].id);
  report.slate = report.slate.filter((entry) => !entry.findings.includes(report.findings[0].id));
  report.consolidation.proposedIssues = report.slate.length;
  delete report.findings[0].failureScenario;
  delete report.findings[0].reverifiedBy;
  assert.deepEqual(validate(reportSchema, report), []);
});

// --- rubric vocabularies -----------------------------------------------------

for (const [field, value] of [["severity", "critical"], ["cost", "medium"], ["radius", "wide"]]) {
  test(`a finding whose ${field} is outside the rubric vocabulary is rejected`, () => {
    const report = exampleReport();
    report.findings[0][field] = value;
    const errors = validate(reportSchema, report);
    assert.ok(
      errors.some((error) => error.path === `findings[0].${field}`),
      `expected a ${field} enum violation, received ${JSON.stringify(errors)}`,
    );
  });
}

test("a malformed criterion id is rejected", () => {
  const report = exampleReport();
  report.findings[0].criterion = "OXA-FOO-1";
  assert.ok(validate(reportSchema, report).some((error) => error.path === "findings[0].criterion"));
});

// --- cross-references --------------------------------------------------------

test("a verdict blocking on an unknown finding is rejected", () => {
  const report = exampleReport();
  report.verdict.blocking = ["F-99"];
  assert.ok(crossCheck(report, {}).some((problem) => /blocks on unknown finding "F-99"/u.test(problem)));
});

test("a verdict blocking on a non-must-fix finding is rejected", () => {
  const report = exampleReport();
  report.findings[2].severity = "defer";
  assert.ok(crossCheck(report, {}).some((problem) => /rather than must-fix/u.test(problem)));
});

test("a slate entry referencing an unknown finding is rejected", () => {
  const report = exampleReport();
  report.slate[0].findings = ["F-77"];
  assert.ok(crossCheck(report, {}).some((problem) => /references unknown finding "F-77"/u.test(problem)));
});

test("a must-fix finding that reaches no slate entry and blocks nothing is rejected", () => {
  // Reporting a must-fix and then dropping it is the failure the framework exists to stop.
  const report = exampleReport();
  report.verdict.blocking = [];
  report.slate = [];
  report.consolidation.proposedIssues = 0;
  const problems = crossCheck(report, {});
  assert.equal(problems.filter((problem) => /appears in no slate entry/u.test(problem)).length, 3);
});

test("duplicate finding ids are rejected", () => {
  const report = exampleReport();
  report.findings[1].id = report.findings[0].id;
  assert.ok(crossCheck(report, {}).some((problem) => /ids are not unique/u.test(problem)));
});

// --- the declared cap --------------------------------------------------------

test("a slate exceeding the declared issue cap is rejected", () => {
  const report = exampleReport();
  report.plan.issueCap = 1;
  assert.ok(crossCheck(report, {}).some((problem) => /against a declared cap of 1/u.test(problem)));
});

test("a single residual entry is exempt from the cap but a second is not", () => {
  const report = exampleReport();
  report.plan.issueCap = 1;
  report.slate[1].residual = true;
  assert.equal(crossCheck(report, {}).filter((problem) => /declared cap/u.test(problem)).length, 0);

  report.slate.push({ ...report.slate[1], title: "Second residual", residual: true });
  report.consolidation.proposedIssues = report.slate.length;
  assert.ok(crossCheck(report, {}).some((problem) => /residual entries; at most one/u.test(problem)));
});

test("a consolidation count disagreeing with the slate is rejected", () => {
  const report = exampleReport();
  report.consolidation.proposedIssues = 99;
  assert.ok(crossCheck(report, {}).some((problem) => /proposedIssues is 99/u.test(problem)));
});

test("raw findings below the number reported is rejected", () => {
  const report = exampleReport();
  report.consolidation.rawFindings = 1;
  assert.ok(crossCheck(report, {}).some((problem) => /below the 3 findings reported/u.test(problem)));
});

// --- ranking -----------------------------------------------------------------

test("a slate ordering that inverts the rubric is rejected", () => {
  // F-03 is must-fix/product/minutes and F-01 is must-fix/class/hours, so the
  // class finding must outrank the cheaper product one. This is precisely the
  // comparison the rubric's worked example calls counter-intuitive.
  const report = exampleReport();
  report.slate = [
    { title: "Seal custody v4 under the strong policy", findings: ["F-03"], severity: "must-fix", cost: "minutes", rank: 1 },
    { title: "Protect every milestone train by ruleset", findings: ["F-01"], severity: "must-fix", cost: "hours", rank: 2 },
  ];
  report.consolidation.proposedIssues = 2;
  report.verdict.blocking = ["F-01", "F-02", "F-03"];
  report.slate.push({ title: "Advisory checks", findings: ["F-02"], severity: "must-fix", cost: "hours", rank: 3 });
  report.consolidation.proposedIssues = 3;
  const problems = crossCheck(report, {});
  assert.ok(
    problems.some((problem) => /loses on blast radius/u.test(problem)),
    `expected a blast-radius inversion, received ${JSON.stringify(problems)}`,
  );
});

test("a slate ordered by the rubric is accepted", () => {
  const report = exampleReport();
  assert.deepEqual(crossCheck(report, { evidence: exampleEvidence() }), []);
});

test("missing, duplicate, and non-consecutive slate ranks are rejected", () => {
  const missing = exampleReport();
  delete missing.slate[0].rank;
  assert.ok(crossCheck(missing, {}).some((problem) => /must declare an integer rank/u.test(problem)));

  const duplicate = exampleReport();
  duplicate.slate[1].rank = 1;
  assert.ok(crossCheck(duplicate, {}).some((problem) => /unique and consecutive/u.test(problem)));
});

// --- delta completeness ------------------------------------------------------

test("delta mode without sinceAnchor or a delta section is rejected", () => {
  const report = exampleReport();
  report.anchor.mode = "delta";
  const errors = validate(reportSchema, report).map((error) => error.message).join("|");
  assert.match(errors, /sinceAnchor/u);
  assert.match(errors, /missing required property "delta"/u);
});

test("a withdrawn delta entry without a note is rejected", () => {
  const report = exampleReport();
  report.anchor.mode = "delta";
  report.anchor.sinceAnchor = "2026-08-01T00:00:00Z";
  report.delta = [{ priorId: "F-09", classification: "withdrawn" }];
  assert.ok(validate(reportSchema, report).some((error) => /missing required property "note"/u.test(error.message)));
});

test("a delta omitting a prior finding is rejected", () => {
  const report = exampleReport();
  report.anchor.mode = "delta";
  report.anchor.sinceAnchor = "2026-08-01T00:00:00Z";
  report.delta = [{ priorId: "F-01", classification: "fixed" }];
  const prior = exampleReport(); // carries F-01, F-02, F-03
  const problems = crossCheck(report, { prior });
  assert.ok(problems.some((problem) => /omits prior finding F-02/u.test(problem)));
  assert.ok(problems.some((problem) => /omits prior finding F-03/u.test(problem)));
});

test("supplying a prior report while not in delta mode is rejected", () => {
  assert.ok(
    crossCheck(exampleReport(), { prior: exampleReport() }).some((problem) => /anchor\.mode is not delta/u.test(problem)),
  );
});

// --- anchor integrity --------------------------------------------------------

test("an abbreviated commit sha is rejected", () => {
  const report = exampleReport();
  report.anchor.branches[0].sha = "d34e65e";
  assert.ok(validate(reportSchema, report).some((error) => error.path === "anchor.branches[0].sha"));
});

test("an unknown audit type is rejected", () => {
  const report = exampleReport();
  report.anchor.auditType = "vibes";
  assert.ok(validate(reportSchema, report).some((error) => error.path === "anchor.auditType"));
});

test("an unexpected top-level property is rejected", () => {
  const report = exampleReport();
  report.score = 7;
  // The output is a triaged slate, not a grade; the schema is closed so a
  // scoring field cannot be smuggled in.
  assert.ok(validate(reportSchema, report).some((error) => /unexpected property "score"/u.test(error.message)));
});

test("report branch scope must match the supplied evidence artifact", () => {
  const report = exampleReport();
  report.anchor.branches[0].sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  assert.ok(crossCheck(report, { evidence: exampleEvidence() }).some((problem) => /branch scope does not match/u.test(problem)));
});

// --- markdown extraction and prose agreement ---------------------------------

test("the report block is extracted from a rendered body", () => {
  const body = ["# Audit", "", "```json audit-report-v1", '{ "schemaVersion": 1 }', "```", ""].join("\n");
  const extracted = extractReportBlock(body);
  assert.equal(extracted.ok, true);
  assert.equal(extracted.report.schemaVersion, 1);
});

test("a body with no report block is rejected", () => {
  const extracted = extractReportBlock("# Audit\n\nNo block here.\n");
  assert.equal(extracted.ok, false);
  assert.match(extracted.error, /no ```json audit-report-v1 block/u);
});

test("an unterminated report block is rejected", () => {
  const extracted = extractReportBlock("```json audit-report-v1\n{}\n");
  assert.equal(extracted.ok, false);
  assert.match(extracted.error, /not closed/u);
});

test("a report block that is not valid JSON is rejected", () => {
  const extracted = extractReportBlock("```json audit-report-v1\n{ oops\n```\n");
  assert.equal(extracted.ok, false);
  assert.match(extracted.error, /not valid JSON/u);
});

test("finding ids are read from prose while ignoring the data block", () => {
  const body = [
    "| 1 | `F-01` | ... |",
    "```json audit-report-v1",
    '{ "findings": [{ "id": "F-42" }] }',
    "```",
  ].join("\n");
  assert.deepEqual([...findingIdsInProse(body)], ["F-01"]);
});

test("finding rows expose rendered severity and cost", () => {
  const body = "| 1 | `F-01` | `OXA-MIL-01` | `defer` | `local` | `minutes` | x | y |\n";
  assert.deepEqual(findingsInProse(body).get("F-01"), { severity: "defer", cost: "minutes" });
});

test("rendered severity and cost must agree with the data block", () => {
  const report = exampleReport();
  const rows = report.findings.map((finding, index) => [
    "|", index + 1, `| \`${finding.id}\` | \`${finding.criterion}\` |`,
    `\`${finding.id === "F-01" ? "defer" : finding.severity}\` | \`${finding.radius}\` |`,
    `\`${finding.id === "F-02" ? "minutes" : finding.cost}\` | x | y |`,
  ].join(" ")).join("\n");
  const result = checkReport(report, { evidence: exampleEvidence(), markdown: rows });
  assert.ok(result.proseErrors.some((problem) => /F-01 renders severity defer/u.test(problem)));
  assert.ok(result.proseErrors.some((problem) => /F-02 renders cost minutes/u.test(problem)));
});

test("a finding present in the data block but not the prose is rejected", () => {
  const report = exampleReport();
  const markdown = "| 1 | `F-01` | x |\n\n```json audit-report-v1\n{}\n```\n";
  const result = checkReport(report, { evidence: exampleEvidence(), markdown });
  assert.ok(result.proseErrors.some((problem) => /F-02 is in the data block but not rendered/u.test(problem)));
});

test("a finding rendered in the prose but absent from the data block is rejected", () => {
  const report = exampleReport();
  const markdown = `${report.findings.map((finding) => `| \`${finding.id}\` |`).join("\n")}\n| \`F-88\` |\n`;
  const result = checkReport(report, { evidence: exampleEvidence(), markdown });
  assert.ok(result.proseErrors.some((problem) => /F-88 is rendered in the prose but absent/u.test(problem)));
});

// --- documented policy matches the live policy -------------------------------

test("the charter and the skill cite the live subagent policy, not remembered figures", () => {
  // OXA-PRC-08, enforced against this framework's own documents. An earlier
  // draft of the charter stated four spawns per session and a sixteen-turn
  // budget, read from a stale checkout; the live policy allowed one spawn. The
  // framework names agent-facing factual drift as a finding class precisely
  // because it is executed rather than read, so its own docs are held to it.
  const policy = JSON.parse(readFileSync(path.join(ROOT, ".pi", "subagent-policy.json"), "utf8"));
  const cited = [
    "maxSubagentSpawnsPerSession",
    "maxSubagentSpawnsPerRun",
    "globalConcurrencyLimit",
  ];
  const documents = [
    path.join(AUDIT_DOCS, "README.md"),
    path.join(ROOT, ".pi", "skills", "oxid-audit", "SKILL.md"),
  ];

  for (const document of documents) {
    const text = readFileSync(document, "utf8");
    for (const key of cited) {
      assert.ok(
        Object.hasOwn(policy, key),
        `${key} is no longer in the policy file; update the documents that cite it`,
      );
      assert.ok(
        text.includes(`"${key}": ${policy[key]}`),
        `${path.basename(document)} does not cite the live ${key} (${policy[key]})`,
      );
    }
    // Figures from the superseded grammar must not linger.
    assert.ok(!/sixteen turns/iu.test(text), `${path.basename(document)} still cites a turn budget that no longer exists`);
    assert.ok(!/four spawns/iu.test(text), `${path.basename(document)} still cites four spawns`);
  }
});

test("every audit agent contract satisfies the repository's own budget validator", () => {
  // The contracts first shipped with `turnBudget`, the grammar the dev-loops
  // 1.0.2 upgrade replaced with `toolBudget`, and CI caught it rather than
  // review. Reuse the repository's validator verbatim so the two cannot drift.
  const source = readFileSync(path.join(ROOT, "scripts", "factory", "audit-pi.mjs"), "utf8");
  const extract = (name) => {
    const start = source.indexOf(`function ${name}`);
    assert.notEqual(start, -1, `audit-pi.mjs no longer defines ${name}`);
    return source.slice(start, source.indexOf("\n}", start) + 2);
  };
  const frontmatterField = /^toolBudget: (\{.*\})$/mu;

  for (const file of ["auditor.agent.md", "audit-consolidator.agent.md"]) {
    const text = readFileSync(path.join(ROOT, ".pi", "agents", file), "utf8");
    const budget = text.match(frontmatterField);
    assert.ok(budget, `${file} declares no toolBudget in the tracked grammar`);

    const parsed = JSON.parse(budget[1]);
    // Mirror the validator's bounds rather than restating chosen values, so a
    // change to the bounds fails here instead of passing silently.
    assert.ok(Number.isInteger(parsed.soft) && parsed.soft >= 1 && parsed.soft <= 64, `${file}: toolBudget.soft out of bounds`);
    assert.ok(Number.isInteger(parsed.hard) && parsed.hard >= parsed.soft && parsed.hard <= 96, `${file}: toolBudget.hard out of bounds`);
    assert.equal(parsed.block, "*", `${file}: toolBudget.block must be "*"`);

    const timeout = Number(text.match(/^timeoutMs: (\d+)$/mu)?.[1]);
    assert.ok(timeout >= 60_000 && timeout <= 3_600_000, `${file}: timeoutMs out of bounds`);

    // No mutation tool may reach an auditor.
    const tools = text.match(/^tools: (.*)$/mu)?.[1] ?? "";
    assert.equal(tools.trim(), "read, grep, find, ls", `${file} grants tools beyond read-only inspection`);
  }
  void extract;
});
