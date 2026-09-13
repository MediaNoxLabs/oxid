#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { parseConventionalSubject, validateBranchName } from "../ci/contribution-policy.mjs";
import { deliveryTargetFromIssueBody } from "../lib/delivery-target.mjs";
import { ensureRecordedDeliveryBase, resolveRepositoryWorktreePath, runEnsureWorktree } from "./ensure-worktree.mjs";

const DEV_LOOP_COMMAND = /^\/dev-loop (prototype|production-ready) issue ([1-9]\d*)$/u;

function command(commandName, args) {
  return execFileSync(commandName, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

/** Return the sole exact public dev-loop command, or leave ordinary Pi input alone. */
export function parseBootstrapDevLoopInvocation(piArgs) {
  const prints = [];
  for (let index = 0; index < piArgs.length; index += 1) {
    const argument = piArgs[index];
    if (argument === "--print") {
      const value = piArgs[index + 1];
      if (value !== undefined) prints.push(value);
      index += 1;
    } else if (argument.startsWith("--print=")) {
      prints.push(argument.slice("--print=".length));
    }
  }
  const devLoops = prints.filter((value) => value.startsWith("/dev-loop"));
  if (devLoops.length === 0) return null;
  if (prints.length !== 1 || devLoops.length !== 1) {
    throw new Error("bootstrap accepts exactly one --print value for an initial /dev-loop invocation");
  }
  const match = devLoops[0].match(DEV_LOOP_COMMAND);
  if (!match) {
    throw new Error("bootstrap accepts only exact '/dev-loop prototype issue <n>' or '/dev-loop production-ready issue <n>' invocations");
  }
  return { profile: match[1], issue: Number(match[2]) };
}

function git(commandRunner, repository, args) {
  return commandRunner("git", ["-C", repository, ...args]).trim();
}

function mainWorktree(commandRunner, repository) {
  const current = path.resolve(git(commandRunner, repository, ["rev-parse", "--show-toplevel"]));
  const porcelain = git(commandRunner, repository, ["worktree", "list", "--porcelain"]);
  const worktrees = porcelain.split(/\r?\n/u)
    .filter((line) => line.startsWith("worktree "))
    .map((line) => path.resolve(line.slice("worktree ".length)));
  if (worktrees.length === 0) throw new Error("git worktree list did not identify the main checkout");
  return { current, main: worktrees[0], worktrees };
}

function issueIdentity(commandRunner, repository, issue) {
  let record;
  try {
    record = JSON.parse(commandRunner("gh", ["issue", "view", String(issue), "--json", "title,body", "--repo", "medianoxlabs/oxid"]));
  } catch (error) {
    throw new Error(`could not resolve issue #${issue} delivery metadata: ${error.message}`);
  }
  const subject = parseConventionalSubject(record?.title);
  if (!subject.ok || !subject.type) throw new Error(`issue #${issue} title is not a valid conventional subject: ${subject.errors.join("; ")}`);
  const target = deliveryTargetFromIssueBody(record?.body);
  const branch = `${subject.type}/issue-${issue}`;
  const branchResult = validateBranchName(branch, { expectedType: subject.type });
  if (!branchResult.ok) throw new Error(`issue #${issue} branch is invalid: ${branchResult.errors.join("; ")}`);
  return { branch, target };
}

function assertCanonicalBranch(commandRunner, canonical, branch) {
  const actual = git(commandRunner, canonical, ["branch", "--show-current"]);
  if (actual !== branch) throw new Error(`canonical worktree branch mismatch: expected ${branch}, found ${actual || "detached HEAD"}`);
}

/**
 * Resolve the cwd before Pi starts. Only the exact public initial command can
 * cross this boundary; malformed lookalikes stop before Pi is dispatched.
 */
export async function resolveBootstrapDevLoopCwd(piArgs, {
  repoRoot,
  run = command,
  ensureWorktree = runEnsureWorktree,
  recordDeliveryBase = ensureRecordedDeliveryBase,
} = {}) {
  if (!repoRoot) throw new Error("--repo-root is required");
  const invocation = parseBootstrapDevLoopInvocation(piArgs);
  if (!invocation) return path.resolve(repoRoot);

  const topology = mainWorktree(run, repoRoot);
  const { branch, target } = issueIdentity(run, topology.main, invocation.issue);
  const canonical = resolveRepositoryWorktreePath(topology.main, ["--issue", String(invocation.issue)]);
  if (topology.current === canonical) {
    if (!topology.worktrees.includes(canonical)) throw new Error(`canonical worktree is not registered: ${canonical}`);
    assertCanonicalBranch(run, canonical, branch);
    recordDeliveryBase(topology.main, branch, target.remoteRef);
    return canonical;
  }
  if (topology.current !== topology.main) {
    throw new Error(`refusing /dev-loop dispatch from non-canonical linked worktree ${topology.current}`);
  }

  const code = await ensureWorktree([
    "--silent", "--repo-root", topology.main, "--issue", String(invocation.issue),
    "--branch", branch, "--delivery-base", target.remoteRef,
  ], { cwd: topology.main });
  if (code !== 0) throw new Error(`could not ensure canonical issue worktree for #${invocation.issue}`);
  const refreshed = mainWorktree(run, topology.main);
  if (!refreshed.worktrees.includes(canonical)) throw new Error(`ensure-worktree did not register canonical worktree ${canonical}`);
  assertCanonicalBranch(run, canonical, branch);
  recordDeliveryBase(topology.main, branch, target.remoteRef);
  return canonical;
}

function parseCli(argv) {
  if (argv[0] !== "--repo-root" || !argv[1] || argv[2] !== "--") {
    throw new Error("usage: bootstrap-dev-loop.mjs --repo-root <checkout> -- <pi args...>");
  }
  return { repoRoot: argv[1], piArgs: argv.slice(3) };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { repoRoot, piArgs } = parseCli(process.argv.slice(2));
    process.stdout.write(`${await resolveBootstrapDevLoopCwd(piArgs, { repoRoot })}\n`);
  } catch (error) {
    process.stderr.write(`[bootstrap-dev-loop] ${error.message}\n`);
    process.exitCode = 1;
  }
}
