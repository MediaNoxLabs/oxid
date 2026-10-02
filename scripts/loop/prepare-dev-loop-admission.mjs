#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, realpathSync, renameSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resolveCanonicalGithubRepository } from "../dev-loops.mjs";
import { parseBootstrapDevLoopInvocation } from "./bootstrap-dev-loop.mjs";

const HEAD = /^[0-9a-f]{40}$/u;
const MAX_ADMISSION_AGE_MS = 5 * 60 * 1000;

function git(cwd, ...args) {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" }).trim();
}

function paths(cwd, issue) {
  const directory = path.join(cwd, "target", "tmp", "dev-loop");
  return {
    directory,
    receipt: path.join(directory, `issue-${issue}-admission.json`),
    startup: path.join(directory, `issue-${issue}-startup.json`),
  };
}

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function assertCheckout(receipt, issue, cwd) {
  if (receipt?.schema !== "oxid-dev-loop-admission-v1" || receipt.issue !== issue) {
    throw new Error(`admission receipt does not bind issue #${issue}`);
  }
  const root = git(cwd, "rev-parse", "--show-toplevel");
  const branch = git(cwd, "branch", "--show-current");
  const headSha = git(cwd, "rev-parse", "HEAD");
  const deliveryBase = git(cwd, "config", "--local", "--get", `branch.${branch}.oxidDeliveryBase`);
  const repository = resolveCanonicalGithubRepository(root);
  if (realpathSync(root) !== realpathSync(cwd) || !repository || repository.toLowerCase() !== receipt.repository?.toLowerCase()
    || branch !== receipt.branch || deliveryBase !== receipt.deliveryBase || !HEAD.test(headSha)) {
    throw new Error(`admission receipt disagrees with the current checkout for issue #${issue}`);
  }
  return { root, headSha };
}

/** Complete authoritative startup before Pi's implementation child is launched. */
export function prepareAdmission(issue, { cwd = process.cwd(), run = execFileSync } = {}) {
  const file = paths(cwd, issue);
  const receipt = readJson(file.receipt);
  const { root, headSha } = assertCheckout(receipt, issue, cwd);
  const stdout = run(process.execPath, [path.join(root, "scripts", "dev-loops.mjs"),
    "loop", "startup", "--issue", String(issue), "--json"], {
    cwd: root, encoding: "utf8", timeout: 120_000,
  });
  const startup = JSON.parse(stdout);
  if (startup?.ok !== true || startup?.canonicalStateSummary?.target?.issue !== issue) {
    throw new Error(`startup did not authorize issue #${issue}`);
  }
  mkdirSync(file.directory, { recursive: true });
  const startupBytes = Buffer.from(`${JSON.stringify(startup)}\n`);
  const prepared = {
    ...receipt, headSha, preparedAt: new Date().toISOString(),
    prePiStartupCalls: 1, implementationChildCalls: 0,
    startupSha256: digest(startupBytes),
  };
  const temporary = `${file.startup}.${process.pid}.tmp`;
  writeFileSync(temporary, startupBytes);
  renameSync(temporary, file.startup);
  writeFileSync(`${file.receipt}.${process.pid}.tmp`, `${JSON.stringify(prepared)}\n`);
  renameSync(`${file.receipt}.${process.pid}.tmp`, file.receipt);
  return { issue, repository: receipt.repository, startupPath: file.startup };
}

/** Verify the pre-Pi snapshot before the child builds its handoff envelope. */
export function verifyAdmission(issue, { cwd = process.cwd(), now = Date.now() } = {}) {
  const file = paths(cwd, issue);
  const receipt = readJson(file.receipt);
  const { headSha } = assertCheckout(receipt, issue, cwd);
  const age = now - Date.parse(receipt.preparedAt);
  if (receipt.headSha !== headSha || !Number.isFinite(age) || age < 0 || age > MAX_ADMISSION_AGE_MS
    || receipt.prePiStartupCalls !== 1 || receipt.implementationChildCalls !== 0
    || digest(readFileSync(file.startup)) !== receipt.startupSha256) {
    throw new Error(`admission snapshot for issue #${issue} is stale or incomplete; restart through ./bootstrap.sh --pi`);
  }
  const startup = readJson(file.startup);
  if (startup?.ok !== true || startup?.canonicalStateSummary?.target?.issue !== issue) {
    throw new Error(`admission startup snapshot disagrees with issue #${issue}`);
  }
  return { issue, repository: receipt.repository, deliveryBase: receipt.deliveryBase,
    startupPath: file.startup, prePiCalls: receipt.calls, prePiStartupCalls: 1 };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [mode, ...args] = process.argv.slice(2);
    let issue;
    if (mode === "prepare" && args[0] === "--") {
      issue = parseBootstrapDevLoopInvocation(args.slice(1))?.issue;
      if (!issue) process.exit(0);
      process.stdout.write(`${JSON.stringify(prepareAdmission(issue))}\n`);
    } else if (mode === "verify" && args.length === 2 && args[0] === "--issue"
      && /^[1-9]\d*$/u.test(args[1])) {
      issue = Number(args[1]);
      process.stdout.write(`${JSON.stringify(verifyAdmission(issue))}\n`);
    } else {
      throw new Error("usage: prepare-dev-loop-admission.mjs prepare -- <pi args...> | verify --issue N");
    }
  } catch (error) {
    process.stderr.write(`[dev-loop-admission] ${error.message}\n`);
    process.exitCode = 1;
  }
}
