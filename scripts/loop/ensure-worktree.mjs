#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

const INTEGRATION_BASE = "origin/integration";

function baseValues(args) {
  const values = [];
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--base") {
      if (index + 1 >= args.length || args[index + 1].startsWith("--")) throw new Error("--base requires a value");
      values.push(args[index + 1]);
      index += 1;
    } else if (args[index].startsWith("--base=")) {
      values.push(args[index].slice("--base=".length));
    }
  }
  return values;
}

/** Force all managed worktrees to start from the integration remote ref. */
export function normalizeWorktreeArgs(argv) {
  const args = [...argv];
  const bases = baseValues(args);
  if (bases.length > 1) throw new Error("repository worktrees accept exactly one base");
  if (bases.some((base) => base !== INTEGRATION_BASE)) {
    throw new Error("repository worktrees must use origin/integration");
  }
  if (bases.length === 0) args.push("--base", INTEGRATION_BASE);
  return args;
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
