#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { deliveryTargetFromIssueBody } from "../lib/delivery-target.mjs";
import { validateFollowUpIssue } from "./review-triage.mjs";

const ORIGIN_PR = /(?:PR\s+#|\/pull\/)([1-9]\d*)/iu;
const ISSUE_REFERENCE = /#([1-9]\d*)/gu;

function section(body, heading) {
  const source = typeof body === "string" ? body : "";
  const match = source.match(new RegExp(`^## ${heading}[ \\t]*$([\\s\\S]*?)(?=^## |(?![\\s\\S]))`, "imu"));
  return match?.[1] ?? "";
}

export function followUpDebtRow(issue, { now = Date.now(), staleDays = 30, dependencyStates = new Map() } = {}) {
  const validation = validateFollowUpIssue(issue, { requireOpen: false });
  const body = typeof issue?.body === "string" ? issue.body : "";
  const labels = Array.isArray(issue?.labels)
    ? issue.labels.map((label) => typeof label === "string" ? label : label?.name).filter(Boolean)
    : [];
  const originPr = Number(body.match(ORIGIN_PR)?.[1] ?? 0) || null;
  const dependencies = [...section(body, "Dependencies").matchAll(ISSUE_REFERENCE)]
    .map((match) => Number(match[1])).filter((number) => number !== originPr);
  const uniqueDependencies = [...new Set(dependencies)].sort((left, right) => left - right);
  const createdAt = Date.parse(issue?.createdAt ?? "");
  const ageDays = Number.isFinite(createdAt) ? Math.floor((now - createdAt) / 86_400_000) : null;
  let deliveryTarget = null;
  try { deliveryTarget = deliveryTargetFromIssueBody(body).branch; } catch { /* reported by validation */ }
  const missingDependencies = uniqueDependencies.filter((number) => !dependencyStates.has(number));
  const stale = issue?.state === "OPEN" && ageDays !== null && ageDays >= staleDays;
  const closedWithoutEvidence = issue?.state === "CLOSED"
    && !issue?.closedByPullRequest
    && issue?.deliveryEvidence !== true;
  const problems = [
    ...validation.failures,
    ...(originPr === null ? ["must identify an origin PR"] : []),
    ...(missingDependencies.length > 0 ? [`unknown dependencies: ${missingDependencies.map((number) => `#${number}`).join(", ")}`] : []),
    ...(closedWithoutEvidence ? ["closed without linked delivery evidence"] : []),
  ];
  return {
    number: issue?.number,
    state: issue?.state,
    ageDays,
    stale,
    deliveryTarget,
    originPr,
    dependencies: uniqueDependencies.map((number) => ({ number, state: dependencyStates.get(number) ?? "UNKNOWN" })),
    technicalDebt: labels.includes("technical-debt"),
    valid: problems.length === 0,
    problems,
  };
}

export function auditFollowUpDebt(issues, options = {}) {
  if (!Array.isArray(issues)) throw new Error("follow-up issues must be an array");
  const rows = issues.map((issue) => followUpDebtRow(issue, options));
  return {
    ok: rows.every((row) => row.valid),
    total: rows.length,
    open: rows.filter((row) => row.state === "OPEN").length,
    closed: rows.filter((row) => row.state === "CLOSED").length,
    stale: rows.filter((row) => row.stale).length,
    invalid: rows.filter((row) => !row.valid).length,
    technicalDebt: rows.filter((row) => row.technicalDebt).length,
    rows,
  };
}

function run(args) {
  try {
    return execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (error) {
    throw new Error(error?.stderr?.trim() || error.message, { cause: error });
  }
}

function parseCli(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" }, "stale-days": { type: "string" }, json: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.repo !== "MediaNoxLabs/oxid") throw new Error("--repo must be MediaNoxLabs/oxid");
  const staleDays = Number(values["stale-days"] ?? "30");
  if (!Number.isSafeInteger(staleDays) || staleDays < 1 || staleDays > 365) throw new Error("--stale-days must be between 1 and 365");
  return { help: false, repo: values.repo, staleDays, json: values.json === true };
}

export function cli(argv = process.argv.slice(2)) {
  const options = parseCli(argv);
  if (options.help) {
    process.stdout.write("Usage: audit-follow-up-debt.mjs --repo MediaNoxLabs/oxid [--stale-days 30] [--json]\n");
    return;
  }
  const issues = JSON.parse(run([
    "issue", "list", "--repo", options.repo, "--label", "factory:follow-up", "--state", "all", "--limit", "1000",
    "--json", "number,title,state,createdAt,closedAt,labels,body,url",
  ]));
  const dependencyNumbers = new Set();
  for (const issue of issues) {
    for (const match of section(issue.body, "Dependencies").matchAll(ISSUE_REFERENCE)) dependencyNumbers.add(Number(match[1]));
  }
  const dependencyStates = new Map();
  for (const number of dependencyNumbers) {
    try {
      const dependency = JSON.parse(run(["issue", "view", String(number), "--repo", options.repo, "--json", "state"]));
      dependencyStates.set(number, dependency.state);
    } catch { /* surfaced as UNKNOWN */ }
  }
  for (const issue of issues.filter((candidate) => candidate.state === "CLOSED")) {
    const evidence = JSON.parse(run([
      "issue", "view", String(issue.number), "--repo", options.repo,
      "--json", "closedByPullRequestsReferences,comments",
    ]));
    issue.closedByPullRequest = evidence.closedByPullRequestsReferences?.length > 0;
    issue.deliveryEvidence = evidence.comments?.some((comment) => /\/pull\/[1-9]\d*/u.test(comment.body ?? "")) ?? false;
  }
  const result = auditFollowUpDebt(issues, { staleDays: options.staleDays, dependencyStates });
  if (options.json) process.stdout.write(`${JSON.stringify(result)}\n`);
  else {
    process.stdout.write(`Follow-up debt: ${result.open} open, ${result.stale} stale, ${result.invalid} invalid, ${result.technicalDebt} technical-debt\n`);
    for (const row of result.rows.filter((entry) => entry.stale || !entry.valid)) {
      process.stdout.write(`#${row.number} ${row.state} age=${row.ageDays ?? "unknown"}d target=${row.deliveryTarget ?? "invalid"}: ${row.problems.join("; ") || "stale"}\n`);
    }
  }
  if (!result.ok) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { cli(); } catch (error) { process.stderr.write(`[audit-follow-up-debt] ${error.message}\n`); process.exitCode = 1; }
}
