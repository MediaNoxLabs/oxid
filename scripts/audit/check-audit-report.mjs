#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

// Validates an audit report before publication, per docs/factory/audit/README.md.
//
// Two classes of check. The schema owns shape: required sections, closed
// property sets, the rubric vocabularies, and the conditional requirements on
// must-fix and delta entries. This file owns the cross-references the schema
// cannot express — that citations resolve, that the slate and verdict point at
// findings that exist, that the declared cap held, and that the rendered prose
// agrees with the machine-readable block.
//
// Usage:
//   node scripts/audit/check-audit-report.mjs <report.json|report.md>
//     [--evidence <evidence.json>]   resolve every anchor citation against it
//     [--prior <prior-report.json>]  require delta coverage of its findings
//     [--json]                       machine-readable result

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { formatErrors, validate } from "./lib/json-schema.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SCHEMA_PATH = path.join(HERE, "..", "..", "docs", "factory", "audit", "audit-report-v1.schema.json");
const FENCE = /^```json audit-report-v1\s*$/mu;

export function extractReportBlock(markdown) {
  const opening = markdown.match(FENCE);
  if (!opening) {
    return { ok: false, error: "no ```json audit-report-v1 block found in the report body" };
  }
  const start = opening.index + opening[0].length;
  const rest = markdown.slice(start);
  const closing = rest.match(/^```\s*$/mu);
  if (!closing) return { ok: false, error: "audit-report-v1 block is not closed" };
  const body = rest.slice(0, closing.index);
  try {
    return { ok: true, report: JSON.parse(body) };
  } catch (error) {
    return { ok: false, error: `audit-report-v1 block is not valid JSON: ${error.message}` };
  }
}

/** Finding ids referenced by a rendered markdown table, for prose/data agreement. */
export function findingIdsInProse(markdown) {
  const withoutBlock = markdown.replace(/^```json audit-report-v1[\s\S]*?^```\s*$/mu, "");
  const currentReport = withoutBlock.split(/^## Delta since\b/mu, 1)[0];
  return new Set(Array.from(currentReport.matchAll(/`(F-[0-9]{2,3})`/gu), (m) => m[1]));
}

/** Parse the fixed findings table into the fields duplicated from report.json. */
export function findingsInProse(markdown) {
  const withoutBlock = markdown.replace(/^```json audit-report-v1[\s\S]*?^```\s*$/mu, "");
  const rendered = new Map();
  for (const line of withoutBlock.split("\n")) {
    const cells = line.split("|").slice(1, -1).map((cell) => cell.trim().replace(/^`|`$/gu, ""));
    if (cells.length < 8 || !/^F-[0-9]{2,3}$/u.test(cells[1] ?? "")) continue;
    rendered.set(cells[1], { severity: cells[3], cost: cells[5] });
  }
  return rendered;
}

function collectAnchorKeys(evidence) {
  return new Set(Object.keys(evidence?.collectors ?? {}));
}

export function crossCheck(report, { evidence = null, prior = null } = {}) {
  const problems = [];
  const complain = (message) => problems.push(message);

  const findings = report.findings ?? [];
  const ids = new Set(findings.map((finding) => finding.id));

  if (ids.size !== findings.length) {
    complain("finding ids are not unique");
  }

  // Citations must resolve. The charter's hard rule is that a finding cites
  // evidence; a citation naming an anchor no collector produced is worse than
  // no citation, because it reads as verified.
  const anchors = evidence ? collectAnchorKeys(evidence) : null;
  const unavailableAnchors = new Set(
    Object.entries(evidence?.collectors ?? {})
      .filter(([, collector]) => collector?.status === "unavailable")
      .map(([key]) => key),
  );
  const cited = [
    ...findings.map((finding) => [`finding ${finding.id}`, finding.evidence]),
    ...(report.verifiedSound ?? []).map((entry, index) => [`verifiedSound[${index}]`, entry.evidence]),
    ...(report.delta ?? []).map((entry) => [`delta ${entry.priorId}`, entry.evidence ?? []]),
  ];
  for (const [owner, citations] of cited) {
    for (const citation of citations ?? []) {
      if (citation.anchor && !anchors) {
        complain(`${owner} cites evidence anchor "${citation.anchor}" but no evidence artifact was supplied`);
      } else if (citation.anchor && !anchors.has(citation.anchor)) {
        complain(`${owner} cites unknown evidence anchor "${citation.anchor}"`);
      } else if (citation.anchor && owner.startsWith("verifiedSound[") && unavailableAnchors.has(citation.anchor)) {
        complain(`${owner} cites unavailable evidence anchor "${citation.anchor}" as verified sound`);
      }
      if (citation.line !== undefined && citation.lines !== undefined) {
        complain(`${owner} citation sets both line and lines`);
      }
      if (citation.path && citation.line === undefined && citation.lines === undefined) {
        complain(`${owner} path citation must set exactly one of line or lines`);
      }
    }
  }

  if (evidence) {
    if (report.anchor?.defaultBranch !== evidence.defaultBranch) {
      complain(`report default branch ${JSON.stringify(report.anchor?.defaultBranch)} does not match evidence ${JSON.stringify(evidence.defaultBranch)}`);
    }
    const scope = (branches) => (branches ?? [])
      .map(({ name, sha, role }) => ({ name, sha, role }))
      .sort((left, right) => left.name.localeCompare(right.name));
    if (JSON.stringify(scope(report.anchor?.branches)) !== JSON.stringify(scope(evidence.branches))) {
      complain("report branch scope does not match the supplied evidence artifact");
    }
  }

  // Every id the verdict blocks on, and every id the slate proposes, must exist.
  for (const id of report.verdict?.blocking ?? []) {
    if (!ids.has(id)) complain(`verdict blocks on unknown finding "${id}"`);
  }
  for (const [index, entry] of (report.slate ?? []).entries()) {
    for (const id of entry.findings ?? []) {
      if (!ids.has(id)) complain(`slate[${index}] references unknown finding "${id}"`);
    }
  }
  for (const id of report.verdict?.blocking ?? []) {
    const finding = findings.find((candidate) => candidate.id === id);
    if (finding && finding.severity !== "must-fix") {
      complain(`verdict blocks on ${id}, which is ${finding.severity} rather than must-fix`);
    }
  }

  // A must-fix finding that reaches no slate entry and blocks nothing has been
  // reported and then dropped, which is the failure the framework exists to stop.
  const slated = new Set((report.slate ?? []).flatMap((entry) => entry.findings ?? []));
  const blocking = new Set(report.verdict?.blocking ?? []);
  for (const finding of findings) {
    if (finding.severity !== "must-fix") continue;
    if (!blocking.has(finding.id) && !finding.duplicateOf) {
      complain(`must-fix finding ${finding.id} is absent from verdict.blocking`);
    }
    if (!slated.has(finding.id) && !blocking.has(finding.id) && !finding.duplicateOf) {
      complain(`must-fix finding ${finding.id} appears in no slate entry, blocks nothing, and duplicates no open issue`);
    }
  }

  // The declared cap held. Residual entries are the overflow mechanism and are
  // exempt, but there may only be one.
  const cap = report.plan?.issueCap;
  const slate = report.slate ?? [];
  const residual = slate.filter((entry) => entry.residual === true);
  if (residual.length > 1) complain(`slate declares ${residual.length} residual entries; at most one is permitted`);
  if (cap !== undefined && slate.length - residual.length > cap) {
    complain(`slate proposes ${slate.length - residual.length} issues against a declared cap of ${cap}`);
  }

  // Consolidation is reported, not silent.
  const consolidation = report.consolidation ?? {};
  if (consolidation.proposedIssues !== undefined && consolidation.proposedIssues !== slate.length) {
    complain(`consolidation.proposedIssues is ${consolidation.proposedIssues} but the slate has ${slate.length} entries`);
  }
  if (consolidation.rawFindings !== undefined && consolidation.rawFindings < findings.length) {
    complain(`consolidation.rawFindings (${consolidation.rawFindings}) is below the ${findings.length} findings reported`);
  }

  // Ranking is mechanical; a declared rank that contradicts the rubric hides
  // the ordering the rubric exists to fix.
  const SEVERITY = { "must-fix": 0, "worth-fixing-now": 1, defer: 2 };
  const RADIUS = { class: 0, product: 1, local: 2 };
  const COST = { minutes: 0, hours: 1, days: 2, weeks: 3 };
  const byId = new Map(findings.map((finding) => [finding.id, finding]));
  const effectiveSeverity = (entry) => {
    const members = (entry.findings ?? []).map((id) => byId.get(id)).filter(Boolean);
    if (members.length === 0) return entry.severity;
    return members.reduce((highest, finding) => (
      (SEVERITY[finding.severity] ?? 9) < (SEVERITY[highest] ?? 9) ? finding.severity : highest
    ), members[0].severity);
  };
  for (const [index, entry] of slate.entries()) {
    const derived = effectiveSeverity(entry);
    if (derived !== entry.severity) {
      complain(`slate[${index}] severity ${entry.severity} softens its highest-severity finding ${derived}`);
    }
  }
  // An entry's radius is the widest radius among the findings it consolidates:
  // merging a class finding into an issue does not narrow that issue's reach.
  const rankKey = (entry) => {
    const members = (entry.findings ?? []).map((id) => byId.get(id)).filter(Boolean);
    const radius = members.length > 0
      ? Math.min(...members.map((finding) => RADIUS[finding.radius] ?? 9))
      : 9;
    return [SEVERITY[effectiveSeverity(entry)] ?? 9, radius, COST[entry.cost] ?? 9];
  };
  const nonResidual = slate.filter((entry) => entry.residual !== true);
  const ranks = nonResidual.map((entry) => entry.rank);
  if (ranks.some((rank) => !Number.isInteger(rank))) {
    complain("every non-residual slate entry must declare an integer rank");
  } else {
    const expected = Array.from({ length: ranks.length }, (_, index) => index + 1);
    const ordered = [...ranks].sort((a, b) => a - b);
    if (JSON.stringify(ordered) !== JSON.stringify(expected)) {
      complain("non-residual slate ranks must be unique and consecutive from 1");
    }
  }
  const ranked = nonResidual
    .filter((entry) => Number.isInteger(entry.rank))
    .sort((a, b) => a.rank - b.rank);
  for (let index = 1; index < ranked.length; index += 1) {
    const previous = ranked[index - 1];
    const current = ranked[index];
    const [pk, ck] = [rankKey(previous), rankKey(current)];
    // A lower key must not sort after a higher one. An expiry on the later
    // entry cannot justify it either: ranking rule 4 only breaks ties that
    // rules 1-3 left equal.
    const comparison = pk.findIndex((value, position) => value !== ck[position]);
    if (comparison !== -1 && pk[comparison] > ck[comparison]) {
      const axis = ["severity", "blast radius", "cost"][comparison];
      complain(
        `slate rank ${previous.rank} sorts above rank ${current.rank} but loses on ${axis}`
        + ` (${previous.severity}/${previous.cost} vs ${current.severity}/${current.cost}), which inverts the rubric`,
      );
    }
  }

  // Delta completeness: every prior finding is classified. Silently dropping
  // one is non-conforming.
  if (report.anchor?.mode === "delta" && !prior) {
    complain("delta report requires a prior report so every prior finding can be classified");
  }
  if (prior) {
    const classified = new Set((report.delta ?? []).map((entry) => entry.priorId));
    for (const finding of prior.findings ?? []) {
      if (!classified.has(finding.id)) {
        complain(`delta omits prior finding ${finding.id}; every prior finding must be classified`);
      }
    }
    if (report.anchor?.mode !== "delta") {
      complain("a prior report was supplied but anchor.mode is not delta");
    }
  }

  return problems;
}

export function checkReport(source, { evidence = null, prior = null, markdown = null } = {}) {
  const schema = JSON.parse(readFileSync(SCHEMA_PATH, "utf8"));
  const schemaErrors = validate(schema, source);
  const priorSchemaErrors = prior ? validate(schema, prior) : [];
  const crossErrors = schemaErrors.length === 0
    ? [
      ...priorSchemaErrors.map((error) => `prior report does not conform at ${error.path || "$"}: ${error.message}`),
      ...(priorSchemaErrors.length === 0 ? crossCheck(source, { evidence, prior }) : []),
    ]
    : [];

  const proseErrors = [];
  if (markdown && schemaErrors.length === 0) {
    const prose = findingIdsInProse(markdown);
    const proseFindings = findingsInProse(markdown);
    const data = new Set((source.findings ?? []).map((finding) => finding.id));
    for (const finding of source.findings ?? []) {
      if (!prose.has(finding.id)) proseErrors.push(`finding ${finding.id} is in the data block but not rendered in the prose`);
      const rendered = proseFindings.get(finding.id);
      if (!rendered) {
        proseErrors.push(`finding ${finding.id} has no row in the rendered findings table`);
      } else {
        if (rendered.severity !== finding.severity) {
          proseErrors.push(`finding ${finding.id} renders severity ${rendered.severity} but data declares ${finding.severity}`);
        }
        if (rendered.cost !== finding.cost) {
          proseErrors.push(`finding ${finding.id} renders cost ${rendered.cost} but data declares ${finding.cost}`);
        }
      }
    }
    for (const id of prose) {
      if (!data.has(id)) proseErrors.push(`finding ${id} is rendered in the prose but absent from the data block`);
    }
  }

  return {
    ok: schemaErrors.length === 0 && crossErrors.length === 0 && proseErrors.length === 0,
    schemaErrors,
    crossErrors,
    proseErrors,
  };
}

function parseArguments(argv) {
  const options = { target: null, evidence: null, prior: null, json: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--evidence") options.evidence = argv[++index];
    else if (argument === "--prior") options.prior = argv[++index];
    else if (argument === "--json") options.json = true;
    else if (argument.startsWith("--")) throw new Error(`unknown option ${argument}`);
    else options.target = argument;
  }
  if (!options.target) throw new Error("usage: check-audit-report.mjs <report.json|report.md> [--evidence f] [--prior f] [--json]");
  return options;
}

function main(argv) {
  const options = parseArguments(argv);
  const raw = readFileSync(options.target, "utf8");
  let report;
  let markdown = null;
  if (options.target.endsWith(".md")) {
    const extracted = extractReportBlock(raw);
    if (!extracted.ok) {
      process.stderr.write(`${extracted.error}\n`);
      return 1;
    }
    report = extracted.report;
    markdown = raw;
  } else {
    report = JSON.parse(raw);
  }

  const result = checkReport(report, {
    evidence: options.evidence ? JSON.parse(readFileSync(options.evidence, "utf8")) : null,
    prior: options.prior ? JSON.parse(readFileSync(options.prior, "utf8")) : null,
    markdown,
  });

  if (options.json) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    return result.ok ? 0 : 1;
  }

  if (result.ok) {
    const findings = report.findings?.length ?? 0;
    const slate = report.slate?.length ?? 0;
    process.stdout.write(
      `Audit report conforms: ${findings} finding${findings === 1 ? "" : "s"}, ${slate} proposed issue${slate === 1 ? "" : "s"}, `
      + `${report.anchor.mode} audit of ${report.anchor.auditType} at criteria version ${report.anchor.criteriaVersion}.\n`,
    );
    return 0;
  }

  if (result.schemaErrors.length > 0) {
    process.stderr.write(`Report does not conform to audit-report-v1:\n${formatErrors(result.schemaErrors)}\n`);
  }
  for (const problem of [...result.crossErrors, ...result.proseErrors]) {
    process.stderr.write(`  ${problem}\n`);
  }
  return 1;
}

if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exit(2);
  }
}
