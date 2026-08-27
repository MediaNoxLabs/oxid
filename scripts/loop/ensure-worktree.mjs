#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn, execFileSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { enforceSingleBase } from "../lib/pinned-dev-loops-args.mjs";

const INTEGRATION_BASE = "origin/integration";

/** Force all managed worktrees to start from the integration remote ref. */
export function normalizeWorktreeArgs(argv) {
  return enforceSingleBase(argv, INTEGRATION_BASE, {
    addWhenMissing: true,
    label: "repository worktrees",
  });
}

function optionValue(args, name) {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
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
  if (!repoRootValue) return [...argv];
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

  const issue = optionValue(argv, "--issue");
  const pr = optionValue(argv, "--pr");
  const selector = issue ? `issue-${issue}` : pr ? `pr-${pr}` : null;
  if (!selector) throw new Error("linked-worktree reuse requires exactly one --issue or --pr selector");
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
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [script, ...args], { cwd, stdio: ["inherit", "pipe", "pipe"] });
    child.stdout.pipe(stdout, { end: false });
    child.stderr.pipe(stderr, { end: false });
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (signal) reject(new Error(`ensure-worktree terminated by ${signal}`));
      else resolve(code ?? 1);
    });
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
