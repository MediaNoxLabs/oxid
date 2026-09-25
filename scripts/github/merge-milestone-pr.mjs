#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { validatePullRequest } from "../ci/contribution-policy.mjs";
import { assertIssueTarget, parseDeliveryTarget } from "../lib/delivery-target.mjs";
import { currentTriageReceipt, validateFollowUpIssue } from "./review-triage.mjs";
import { assertReviewActionAllowed, currentReviewControl } from "./review-control.mjs";
import { classifyOptionalSarifChecks, CRITICAL_CHECKS, OPTIONAL_SARIF_PROJECTIONS } from "./optional-sarif-policy.mjs";

const REPOSITORY = "MediaNoxLabs/oxid";
const BLOCKING_TITLE_MARKERS = /(?:\[?\bWIP\b\]?|\bDRAFT\b|DO NOT MERGE|🚧)/iu;
const CLOSING_ISSUE = /\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#([1-9]\d*)\b/iu;
const ELIGIBLE_MERGE_STATES = new Set(["CLEAN", "UNSTABLE"]);
export { CRITICAL_CHECKS, OPTIONAL_SARIF_PROJECTIONS };

function parseJson(source, label) {
  try {
    return JSON.parse(source);
  } catch (error) {
    throw new Error(`${label} returned invalid JSON: ${error.message}`, { cause: error });
  }
}

export function parseMergeMilestoneArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      pr: { type: "string" },
      execute: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.repo !== REPOSITORY) throw new Error(`--repo must be ${REPOSITORY}`);
  const pr = Number(values.pr);
  if (!Number.isInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  return { help: false, repo: values.repo, pr, execute: values.execute === true };
}

export function closingIssueNumber(body) {
  const match = typeof body === "string" ? body.match(CLOSING_ISSUE) : null;
  return match ? Number(match[1]) : null;
}

export function validateMilestonePr(pr) {
  const failures = [];
  let target = null;
  try {
    target = parseDeliveryTarget(pr?.baseRefName ?? "");
    if (target.kind !== "milestone") failures.push("base must be milestone-<x.y.z>; develop and main are human-only");
  } catch {
    failures.push("base must be milestone-<x.y.z>; develop and main are human-only");
  }
  if (pr?.state !== "OPEN") failures.push("pull request is not open");
  if (pr?.isDraft !== false) failures.push("pull request is still a draft");
  if (pr?.isCrossRepository === true) failures.push("cross-repository heads are not eligible for automated merge");
  if (BLOCKING_TITLE_MARKERS.test(pr?.title ?? "")) failures.push("title contains a merge-blocking marker");
  if (pr?.mergeable !== "MERGEABLE") failures.push(`mergeable is ${pr?.mergeable ?? "unknown"}`);
  if (!ELIGIBLE_MERGE_STATES.has(pr?.mergeStateStatus)) {
    failures.push(`mergeStateStatus is ${pr?.mergeStateStatus ?? "unknown"}, expected CLEAN or UNSTABLE`);
  }
  for (const field of ["headRefOid", "baseRefOid"]) {
    if (typeof pr?.[field] !== "string" || !/^[0-9a-f]{40}$/u.test(pr[field])) failures.push(`${field} is missing or malformed`);
  }
  const issue = closingIssueNumber(pr?.body);
  if (issue === null) failures.push("PR body must close an issue with a problem statement");
  const contribution = validatePullRequest({ title: pr?.title, body: pr?.body, branch: pr?.headRefName });
  if (!contribution.ok) failures.push(...contribution.errors);
  if (issue !== null && contribution.branch.issue !== issue) failures.push("PR head branch and closing issue number do not match");
  return { ok: failures.length === 0, failures, issue, target };
}

export function validateCriticalChecks(checks) {
  const failures = [];
  if (!Array.isArray(checks)) return { ok: false, failures: ["check results are unavailable"] };
  for (const name of CRITICAL_CHECKS) {
    const matches = checks.filter((check) => check?.name === name);
    if (matches.length !== 1) failures.push(`${name}: expected exactly one current check`);
    else if (matches[0].bucket !== "pass") failures.push(`${name}: ${matches[0].state ?? matches[0].bucket ?? "unknown"}`);
  }
  return { ok: failures.length === 0, failures };
}

export function validateMilestoneChecks(checks) {
  const critical = validateCriticalChecks(checks);
  const failures = [...critical.failures];
  if (!Array.isArray(checks)) return { ok: false, failures };
  const policy = classifyOptionalSarifChecks(checks);
  for (const check of policy.retained) {
    if (CRITICAL_CHECKS.includes(check?.name)
      || check?.bucket === "pass"
      || check?.bucket === "skipping") continue;
    failures.push(`${check?.name ?? "unnamed check"}: ${check?.state ?? check?.bucket ?? "unknown"}`);
  }
  return { ok: failures.length === 0, failures };
}

function defaultRun(command, args, { cwd, label = command } = {}) {
  try {
    return execFileSync(command, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (error) {
    const detail = error?.stderr?.trim() || error?.message || "unknown failure";
    throw new Error(`${label} failed: ${detail}`, { cause: error });
  }
}

function ghJson(run, args, cwd, label) {
  return parseJson(run("gh", args, { cwd, label }), label);
}

export function auditMilestoneMerge(options, { cwd = process.cwd(), run = defaultRun } = {}) {
  const root = run("git", ["rev-parse", "--show-toplevel"], { cwd, label: "resolve repository root" }).trim();
  const fields = "state,baseRefName,baseRefOid,headRefName,headRefOid,isDraft,isCrossRepository,mergeable,mergeStateStatus,title,body";
  const pr = ghJson(run, ["pr", "view", String(options.pr), "--repo", options.repo, "--json", fields], root, "read pull request facts");
  const eligibility = validateMilestonePr(pr);
  if (!eligibility.ok) throw new Error(`automated milestone merge denied: ${eligibility.failures.join("; ")}`);

  const issue = ghJson(run, ["issue", "view", String(eligibility.issue), "--repo", options.repo, "--json", "state,body"], root, "read backing issue");
  if (issue?.state !== "OPEN" || typeof issue?.body !== "string" || issue.body.trim().length < 40) {
    throw new Error(`backing issue #${eligibility.issue} must be open and contain a problem statement`);
  }
  assertIssueTarget(issue.body, eligibility.target);

  run("git", ["fetch", "--no-tags", "origin", eligibility.target.branch, pr.headRefOid], { cwd: root, label: "refresh milestone and PR head" });
  const localBase = run("git", ["rev-parse", `refs/remotes/origin/${eligibility.target.branch}`], { cwd: root, label: "resolve fetched milestone" }).trim();
  if (localBase !== pr.baseRefOid) throw new Error(`base changed during audit: GitHub ${pr.baseRefOid}, fetched ${localBase}`);
  run("git", ["merge-base", "--is-ancestor", localBase, pr.headRefOid], { cwd: root, label: "verify current-head freshness" });
  run("git", ["merge-tree", "--write-tree", localBase, pr.headRefOid], { cwd: root, label: "verify conflict-free merge tree" });

  const checks = ghJson(run, ["pr", "checks", String(options.pr), "--repo", options.repo, "--json", "bucket,name,state,workflow"], root, "read current checks");
  const checkResult = validateMilestoneChecks(checks);
  if (!checkResult.ok) throw new Error(`pull request checks are not green: ${checkResult.failures.join("; ")}`);

  const comments = ghJson(run, ["api", `repos/${options.repo}/issues/${options.pr}/comments`, "--paginate", "--slurp"], root, "read review triage comments").flat();
  const triage = currentTriageReceipt(comments, pr.headRefOid);
  const control = currentReviewControl(comments, pr.headRefOid, { required: true });
  if (!control.frozen) throw new Error("review control has not frozen the exact head");
  assertReviewActionAllowed(control, { headSha: pr.headRefOid, action: "merge" });
  if (JSON.stringify([...control.followUpIssues].sort((a, b) => a - b))
    !== JSON.stringify([...triage.followUpIssues].sort((a, b) => a - b))) {
    throw new Error("review control and triage receipt disagree on follow-up issues");
  }
  for (const followUp of triage.followUpIssues) {
    const item = ghJson(run, ["issue", "view", String(followUp), "--repo", options.repo, "--json", "state,body,labels"], root, `read follow-up issue #${followUp}`);
    const validation = validateFollowUpIssue(item, { originPr: options.pr });
    if (!validation.ok) throw new Error(`follow-up issue #${followUp} ${validation.failures.join("; ")}`);
  }

  run(process.execPath, [path.join(root, "scripts", "dev-loops.mjs"), "gates"], { cwd: root, label: "validate repository dev-loop policy" });
  const current = ghJson(run, ["pr", "view", String(options.pr), "--repo", options.repo, "--json", "baseRefName,baseRefOid,headRefOid"], root, "re-read pull request head");
  if (current?.baseRefName !== eligibility.target.branch || current?.baseRefOid !== localBase || current?.headRefOid !== pr.headRefOid) {
    throw new Error("pull request head or milestone base changed during the merge audit");
  }
  return {
    ok: true,
    repo: options.repo,
    pr: options.pr,
    issue: eligibility.issue,
    target: eligibility.target.branch,
    headSha: pr.headRefOid,
    baseSha: localBase,
    worktree: root,
    checks: CRITICAL_CHECKS.length,
    observedChecks: checks.length,
    followUps: triage.followUpIssues,
  };
}

export function runCli(argv = process.argv.slice(2), runtime = {}) {
  const options = parseMergeMilestoneArgs(argv);
  if (options.help) {
    (runtime.stdout ?? process.stdout).write("Usage: merge-milestone-pr.mjs --repo MediaNoxLabs/oxid --pr NUMBER [--execute]\n");
    return;
  }
  const result = auditMilestoneMerge(options, runtime);
  if (options.execute) {
    const run = runtime.run ?? defaultRun;
    run("gh", ["pr", "merge", String(options.pr), "--repo", options.repo, "--squash", "--match-head-commit", result.headSha], {
      cwd: runtime.cwd ?? process.cwd(), label: "merge audited milestone pull request",
    });
  }
  const closeout = options.execute ? {
    required: true,
    runFrom: "outside the merged PR worktree after metrics are recorded",
    command: [
      process.execPath,
      path.join("scripts", "worktree-lifecycle.mjs"),
      "closeout-pr",
      "--pr", String(options.pr),
      "--path", result.worktree,
      "--expect-head", result.headSha,
      "--execute",
    ],
  } : null;
  (runtime.stdout ?? process.stdout).write(`${JSON.stringify({ ...result, merged: options.execute, closeout })}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[merge-milestone-pr] ${error.message}\n`);
    process.exitCode = 1;
  }
}
