// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

import {
  assertIssueTarget,
  deliveryTargetFromIssueBody,
  extractDeliveryTargetOption,
  parseDeliveryTarget,
} from "../../scripts/lib/delivery-target.mjs";
import {
  CRITICAL_CHECKS,
  auditMilestoneMerge,
  parseMergeMilestoneArgs,
  validateCriticalChecks,
  validateMilestonePr,
} from "../../scripts/github/merge-milestone-pr.mjs";
import { buildTriageReceipt, currentTriageReceipt, parseTriageComment } from "../../scripts/github/review-triage.mjs";
import { normalizeDevLoopsArgs } from "../../scripts/dev-loops.mjs";

test("delivery targets are exact develop or semantic milestone branches", () => {
  assert.deepEqual(parseDeliveryTarget("develop"), {
    branch: "develop", remoteRef: "origin/develop", kind: "factory",
  });
  assert.deepEqual(parseDeliveryTarget("origin/milestone-0.4.0"), {
    branch: "milestone-0.4.0", remoteRef: "origin/milestone-0.4.0", kind: "milestone",
  });
  assert.deepEqual(parseDeliveryTarget("milestone-2.0.13"), {
    branch: "milestone-2.0.13", remoteRef: "origin/milestone-2.0.13", kind: "milestone",
  });
  for (const invalid of ["", "main", "integration", "milestone-latest", "milestone-1.2", "milestone-01.2.3", " origin/develop"] ) {
    assert.throws(() => parseDeliveryTarget(invalid), /delivery target/);
  }
});

test("issue body carries exactly one unambiguous delivery target", () => {
  assert.equal(deliveryTargetFromIssueBody("## Goal\nShip it\n\n## Delivery target\n\n`milestone-0.4.0`\n\n## Context\nBounded").branch, "milestone-0.4.0");
  assert.equal(deliveryTargetFromIssueBody("### Delivery target\n\ndevelop\n").branch, "develop");
  assert.throws(() => deliveryTargetFromIssueBody("## Goal\nMissing"), /exactly one/);
  assert.throws(() => deliveryTargetFromIssueBody("## Delivery target\ndevelop\n## Delivery target\nmilestone-0.4.0"), /exactly one/);
  assert.throws(() => deliveryTargetFromIssueBody("## Delivery target\ndevelop\nmilestone-0.4.0"), /exactly one branch name/);
});

test("CLI delivery target is singular, stripped, and issue-bound", () => {
  const selected = extractDeliveryTargetOption(["pr", "create", "--delivery-base=milestone-0.4.0", "--head", "feat/issue-1"], { required: true });
  assert.deepEqual(selected.args, ["pr", "create", "--head", "feat/issue-1"]);
  assert.equal(selected.target.remoteRef, "origin/milestone-0.4.0");
  assert.throws(() => extractDeliveryTargetOption(["pr", "create"], { required: true }), /required/);
  assert.throws(() => extractDeliveryTargetOption(["--delivery-base", "develop", "--delivery-base", "milestone-0.4.0"]), /only once/);
  assert.equal(assertIssueTarget("## Delivery target\n\nmilestone-0.4.0", "origin/milestone-0.4.0").kind, "milestone");
  assert.throws(() => assertIssueTarget("## Delivery target\n\ndevelop", "milestone-0.4.0"), /does not match/);
});

test("a stacked PR keeps its conventional parent while retaining its delivery target", () => {
  const args = normalizeDevLoopsArgs([
    "pr", "create", "--head", "feat/issue-280", "--base", "docs/issue-279", "--delivery-base", "develop",
  ]);
  assert.deepEqual(args, ["pr", "create", "--head", "feat/issue-280", "--base", "docs/issue-279"]);
  assert.throws(
    () => normalizeDevLoopsArgs(["pr", "create", "--base", "main", "--delivery-base", "develop"]),
    /conventional issue branch/u,
  );
});

function milestonePr(overrides = {}) {
  return {
    state: "OPEN",
    baseRefName: "milestone-0.4.0",
    baseRefOid: "a".repeat(40),
    headRefName: "feat/issue-42",
    headRefOid: "b".repeat(40),
    isDraft: false,
    isCrossRepository: false,
    mergeable: "MERGEABLE",
    mergeStateStatus: "CLEAN",
    title: "feat(wallet): stream one increment",
    body: "Closes #42",
    ...overrides,
  };
}

test("only issue-matched milestone PRs are eligible for automatic merge", () => {
  assert.equal(validateMilestonePr(milestonePr()).ok, true);
  for (const baseRefName of ["develop", "main", "integration", "milestone-latest"]) {
    assert.match(validateMilestonePr(milestonePr({ baseRefName })).failures.join("; "), /milestone-<x\.y\.z>/);
  }
  for (const overrides of [
    { state: "CLOSED" }, { isDraft: true }, { mergeStateStatus: "BEHIND" },
    { headRefOid: "short" }, { headRefName: "feat/issue-43" }, { body: "Refs #42" },
  ]) assert.equal(validateMilestonePr(milestonePr(overrides)).ok, false);
  assert.deepEqual(parseMergeMilestoneArgs(["--repo", "MediaNoxLabs/oxid", "--pr", "42", "--execute"]), {
    help: false, repo: "MediaNoxLabs/oxid", pr: 42, execute: true,
  });
});

test("critical checks are fixed, unique, and green", () => {
  const passing = CRITICAL_CHECKS.map((name) => ({ name, bucket: "pass", state: "SUCCESS" }));
  assert.equal(validateCriticalChecks(passing).ok, true);
  assert.equal(validateCriticalChecks(passing.filter((check) => check.name !== "scan")).ok, false);
  assert.equal(validateCriticalChecks(passing.map((check) => check.name === "scan" ? { ...check, bucket: "fail" } : check)).ok, false);
  assert.equal(validateCriticalChecks([...passing, passing[0]]).ok, false);
});

test("review triage is exact-head and cannot defer a blocking finding", () => {
  const headSha = "b".repeat(40);
  const body = buildTriageReceipt({ headSha, followUpIssues: [51, 52] });
  assert.deepEqual(parseTriageComment(body).followUpIssues, [51, 52]);
  assert.deepEqual(currentTriageReceipt([{ body }], headSha).followUpIssues, [51, 52]);
  assert.throws(() => currentTriageReceipt([{ body }], "c".repeat(40)), /exactly one/);
  assert.throws(() => currentTriageReceipt([{ body: buildTriageReceipt({ headSha, blockingFindingCount: 1 }) }], headSha), /blocking findings/);
  assert.throws(() => currentTriageReceipt([{ body }, { body }], headSha), /exactly one/);
});

function milestoneAuditRun({ reReadHead = "b".repeat(40), issueTarget = "milestone-0.4.0" } = {}) {
  const pr = milestonePr();
  const checks = CRITICAL_CHECKS.map((name) => ({ name, bucket: "pass", state: "SUCCESS", workflow: "fixture" }));
  return (command, args) => {
    if (command === "git" && args[0] === "rev-parse" && args[1] === "--show-toplevel") return "/repo\n";
    if (command === "git" && args[0] === "rev-parse") return `${pr.baseRefOid}\n`;
    if (command === "git") return "";
    if (command === process.execPath) return "";
    if (command !== "gh") throw new Error(`unexpected command ${command}`);
    if (args[0] === "pr" && args[1] === "view" && args.at(-1).includes("state,")) return JSON.stringify(pr);
    if (args[0] === "pr" && args[1] === "view") return JSON.stringify({
      baseRefName: pr.baseRefName, baseRefOid: pr.baseRefOid, headRefOid: reReadHead,
    });
    if (args[0] === "issue" && args[1] === "view") return JSON.stringify({
      state: "OPEN",
      body: `## Goal\nShip one bounded increment safely.\n\n## Delivery target\n\n${issueTarget}\n\n## Acceptance criteria\n\n1. It works.`,
    });
    if (args[0] === "pr" && args[1] === "checks") return JSON.stringify(checks);
    if (args[0] === "api") return JSON.stringify([[{
      body: buildTriageReceipt({ headSha: pr.headRefOid }),
    }]]);
    throw new Error(`unexpected gh args ${args.join(" ")}`);
  };
}

test("milestone audit binds issue target, base, checks, triage, and final head", () => {
  const options = { repo: "MediaNoxLabs/oxid", pr: 42, execute: false };
  assert.equal(auditMilestoneMerge(options, { cwd: "/repo", run: milestoneAuditRun() }).target, "milestone-0.4.0");
  assert.throws(() => auditMilestoneMerge(options, { cwd: "/repo", run: milestoneAuditRun({ issueTarget: "develop" }) }), /does not match/);
  assert.throws(() => auditMilestoneMerge(options, { cwd: "/repo", run: milestoneAuditRun({ reReadHead: "c".repeat(40) }) }), /changed during/);
});

test("milestone merge implementation pins squash execution to the audited head", async () => {
  const source = await readFile(new URL("../../scripts/github/merge-milestone-pr.mjs", import.meta.url), "utf8");
  assert.match(source, /--squash/);
  assert.match(source, /--match-head-commit/);
  assert.doesNotMatch(source, /--admin/);
  assert.match(source, /currentTriageReceipt/);
  assert.match(source, /assertIssueTarget/);
});
