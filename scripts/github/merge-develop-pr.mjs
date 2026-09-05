#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

const DEVELOP_BASE = "develop";
const REPOSITORY = "MediaNoxLabs/oxid";
const BLOCKING_TITLE_MARKERS = /(?:\[?\bWIP\b\]?|\bDRAFT\b|DO NOT MERGE|🚧)/i;
const CLOSING_ISSUE = /\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#([1-9]\d*)\b/i;
const ELIGIBLE_MERGE_STATES = new Set(["CLEAN", "UNSTABLE"]);

function parseJson(source, label) {
  try {
    return JSON.parse(source);
  } catch (error) {
    throw new Error(`${label} returned invalid JSON: ${error.message}`, { cause: error });
  }
}

export function parseMergeDevelopArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      pr: { type: "string" },
      execute: { type: "boolean" },
      "authorized-by-owner": { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.repo !== REPOSITORY) throw new Error(`--repo must be ${REPOSITORY}`);
  const pr = Number(values.pr);
  if (!Number.isInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  const execute = values.execute === true;
  const authorizedByOwner = values["authorized-by-owner"] === true;
  if (execute) throw new Error("automated merges to develop are disabled; hand the PR to a human");
  return { help: false, repo: values.repo, pr, execute, authorizedByOwner };
}

export function closingIssueNumber(body) {
  const match = typeof body === "string" ? body.match(CLOSING_ISSUE) : null;
  return match ? Number(match[1]) : null;
}

export function validatePrForDevelopMerge(pr) {
  const failures = [];
  if (pr?.state !== "OPEN") failures.push("pull request is not open");
  if (pr?.baseRefName !== DEVELOP_BASE) {
    failures.push(`base must be ${DEVELOP_BASE}; main is human-only`);
  }
  if (pr?.isDraft !== false) failures.push("pull request is still a draft");
  if (pr?.isCrossRepository === true) failures.push("cross-repository heads are not eligible for automated merge");
  if (BLOCKING_TITLE_MARKERS.test(pr?.title ?? "")) failures.push("title contains a merge-blocking marker");
  if (pr?.mergeable !== "MERGEABLE") failures.push(`mergeable is ${pr?.mergeable ?? "unknown"}`);
  // GitHub reports UNSTABLE when an advisory check fails. Required checks are
  // queried and enforced independently below, so advisory signal stays visible
  // without acquiring merge authority.
  if (!ELIGIBLE_MERGE_STATES.has(pr?.mergeStateStatus)) {
    failures.push(`mergeStateStatus is ${pr?.mergeStateStatus ?? "unknown"}, expected CLEAN or UNSTABLE`);
  }
  if (typeof pr?.headRefOid !== "string" || !/^[0-9a-f]{40}$/.test(pr.headRefOid)) {
    failures.push("head SHA is missing or malformed");
  }
  if (typeof pr?.baseRefOid !== "string" || !/^[0-9a-f]{40}$/.test(pr.baseRefOid)) {
    failures.push("base SHA is missing or malformed");
  }
  const issue = closingIssueNumber(pr?.body);
  if (issue === null) failures.push("PR body must close an issue with a problem statement");
  return { ok: failures.length === 0, failures, issue };
}

export function validateRequiredChecks(checks) {
  const failures = [];
  if (!Array.isArray(checks) || checks.length === 0) failures.push("no required checks were returned");
  for (const check of Array.isArray(checks) ? checks : []) {
    if (check?.bucket !== "pass") failures.push(`${check?.name ?? "unnamed check"}: ${check?.state ?? check?.bucket ?? "unknown"}`);
  }
  if (!(Array.isArray(checks) && checks.some((check) => check?.name === "Verify commit sign-offs" && check?.bucket === "pass"))) {
    failures.push("required GPG/DCO check is absent or not passing");
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

export function auditDevelopMerge(options, { cwd = process.cwd(), run = defaultRun } = {}) {
  const root = run("git", ["rev-parse", "--show-toplevel"], { cwd, label: "resolve repository root" }).trim();
  const prFields = "state,baseRefName,baseRefOid,headRefName,headRefOid,isDraft,isCrossRepository,mergeable,mergeStateStatus,title,body";
  const pr = ghJson(run, ["pr", "view", String(options.pr), "--repo", options.repo, "--json", prFields], root, "read pull request facts");
  const eligibility = validatePrForDevelopMerge(pr);
  if (!eligibility.ok) throw new Error(`automated merge denied: ${eligibility.failures.join("; ")}`);

  const issue = ghJson(run, ["issue", "view", String(eligibility.issue), "--repo", options.repo, "--json", "state,body"], root, "read backing issue");
  if (issue?.state !== "OPEN" || typeof issue?.body !== "string" || issue.body.trim().length < 40) {
    throw new Error(`backing issue #${eligibility.issue} must be open and contain a problem statement`);
  }

  run("git", ["fetch", "--no-tags", "origin", DEVELOP_BASE, pr.headRefOid], { cwd: root, label: "refresh develop and PR head" });
  const localBase = run("git", ["rev-parse", `refs/remotes/origin/${DEVELOP_BASE}`], { cwd: root, label: "resolve fetched develop" }).trim();
  if (localBase !== pr.baseRefOid) throw new Error(`base changed during audit: GitHub ${pr.baseRefOid}, fetched ${localBase}`);
  run("git", ["merge-base", "--is-ancestor", localBase, pr.headRefOid], { cwd: root, label: "verify current-head freshness" });
  run("git", ["merge-tree", "--write-tree", localBase, pr.headRefOid], { cwd: root, label: "verify conflict-free merge tree" });

  const checks = ghJson(run, ["pr", "checks", String(options.pr), "--repo", options.repo, "--required", "--json", "bucket,name,state,workflow"], root, "read required checks");
  const checkResult = validateRequiredChecks(checks);
  if (!checkResult.ok) throw new Error(`required checks are not green: ${checkResult.failures.join("; ")}`);

  const devLoops = path.join(root, "scripts", "dev-loops.mjs");
  run(process.execPath, [devLoops, "gates"], { cwd: root, label: "validate repository dev-loop policy" });
  run(process.execPath, [devLoops, "gate", "detect-evidence", "--repo", options.repo, "--pr", String(options.pr), "--silent"], {
    cwd: root,
    label: "verify current-head gate evidence and resolved conversations",
  });

  const current = ghJson(run, ["pr", "view", String(options.pr), "--repo", options.repo, "--json", "baseRefName,baseRefOid,headRefOid"], root, "re-read pull request head");
  if (current?.baseRefName !== DEVELOP_BASE || current?.baseRefOid !== localBase || current?.headRefOid !== pr.headRefOid) {
    throw new Error("pull request head or develop base changed during the merge audit");
  }

  return { ok: true, repo: options.repo, pr: options.pr, issue: eligibility.issue, headSha: pr.headRefOid, baseSha: localBase, checks: checks.length };
}

export function runCli(argv = process.argv.slice(2), runtime = {}) {
  const options = parseMergeDevelopArgs(argv);
  if (options.help) {
    (runtime.stdout ?? process.stdout).write(
      "Usage: merge-develop-pr.mjs --repo OWNER/REPO --pr NUMBER [--authorized-by-owner --execute]\n",
    );
    return;
  }
  const result = auditDevelopMerge(options, runtime);
  (runtime.stdout ?? process.stdout).write(`${JSON.stringify({ ...result, merged: false })}\n`);
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[merge-develop-pr] ${error.message}\n`);
    process.exitCode = 1;
  }
}
