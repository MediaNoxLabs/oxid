#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { runManagedChild } from "../lib/managed-child-process.mjs";
import { enforceSingleBase, readLongOptionValues } from "../lib/pinned-dev-loops-args.mjs";

const INTEGRATION_BASE = "origin/integration";

/** Force all managed worktrees to start from the integration remote ref. */
export function normalizeWorktreeArgs(argv) {
  return enforceSingleBase(argv, INTEGRATION_BASE, {
    addWhenMissing: true,
    label: "repository worktrees",
  });
}

function optionValue(args, name) {
  const values = readLongOptionValues(args, name);
  if (values.length > 1) throw new Error(`${name} accepts exactly one value`);
  return values[0];
}

function selectorValue(args) {
  const issue = optionValue(args, "--issue");
  const pr = optionValue(args, "--pr");
  if ((issue === undefined) === (pr === undefined)) {
    throw new Error("worktree context requires exactly one --issue or --pr selector");
  }
  const value = issue ?? pr;
  if (!/^[1-9]\d*$/.test(value)) throw new Error(`${issue === undefined ? "--pr" : "--issue"} must be a positive integer`);
  return issue === undefined ? `pr-${value}` : `issue-${value}`;
}

function replaceOption(args, name, value) {
  const rewritten = [...args];
  const index = rewritten.indexOf(name);
  if (index >= 0) rewritten[index + 1] = value;
  else {
    const prefix = `${name}=`;
    const equalsIndex = rewritten.findIndex((arg) => arg.startsWith(prefix));
    if (equalsIndex >= 0) rewritten[equalsIndex] = `${name}=${value}`;
  }
  return rewritten;
}

export function normalizeLinkedWorktreeContext(argv, { gitCommand = "git" } = {}) {
  const repoRootValue = optionValue(argv, "--repo-root");
  if (repoRootValue === undefined) return [...argv];
  const selector = selectorValue(argv);
  const requestedRoot = realpathSync(path.resolve(repoRootValue));
  const topLevel = realpathSync(execFileSync(gitCommand, ["-C", requestedRoot, "rev-parse", "--show-toplevel"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim());
  const porcelain = execFileSync(gitCommand, ["-C", requestedRoot, "worktree", "list", "--porcelain"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  const mainLine = porcelain.split(/\r?\n/).find((line) => line.startsWith("worktree "));
  if (!mainLine) throw new Error("git worktree list did not identify the main checkout");
  const mainRoot = realpathSync(mainLine.slice("worktree ".length));
  if (topLevel === mainRoot) return replaceOption(argv, "--repo-root", mainRoot);

  const canonicalTarget = path.join(mainRoot, "tmp", "worktrees", "dev-loops", selector);
  if (topLevel !== canonicalTarget) {
    throw new Error(
      `refusing nested worktree creation from ${topLevel}; re-run with --repo-root ${mainRoot} for canonical target ${canonicalTarget}`,
    );
  }
  return replaceOption(argv, "--repo-root", mainRoot);
}

export async function runEnsureWorktree(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const args = normalizeLinkedWorktreeContext(normalizeWorktreeArgs(argv));
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const script = path.join(resolved.packageRoot, "scripts", "loop", "ensure-worktree.mjs");
  return runManagedChild(process.execPath, [script, ...args], {
    cwd,
    stdout,
    stderr,
    label: "ensure-worktree",
  });
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  runEnsureWorktree().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[ensure-worktree] ${error.message}\n`);
    process.exitCode = 1;
  });
}
