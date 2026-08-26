#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
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

export async function runEnsureWorktree(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const args = normalizeWorktreeArgs(argv);
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
