#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "./lib/dev-loop-runtime.mjs";

const INTEGRATION_BASE = "integration";

function readOption(args, name) {
  const values = [];
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === name) {
      if (index + 1 >= args.length || args[index + 1].startsWith("--")) throw new Error(`${name} requires a value`);
      values.push(args[index + 1]);
      index += 1;
    } else if (args[index].startsWith(`${name}=`)) {
      values.push(args[index].slice(name.length + 1));
    }
  }
  return values;
}

/** Force the only writable delivery base for repository PR creation. */
export function normalizeDevLoopsArgs(argv) {
  const args = [...argv];
  const prIndex = args.findIndex((argument) => argument === "pr");
  const isPrCreate = prIndex >= 0 && (args[prIndex + 1] === "create" || args[prIndex + 1] === "create-draft");
  if (!isPrCreate) return args;
  const bases = readOption(args, "--base");
  if (bases.length > 1) throw new Error("repository pull requests accept exactly one base");
  if (bases.some((base) => base !== INTEGRATION_BASE)) {
    throw new Error("repository pull requests must target integration");
  }
  if (bases.length === 0) args.push("--base", INTEGRATION_BASE);
  return args;
}

export async function runDevLoops(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const args = normalizeDevLoopsArgs(argv);
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const cli = path.join(resolved.packageRoot, "cli", "index.mjs");
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cli, ...args], { cwd, stdio: ["inherit", "pipe", "pipe"] });
    child.stdout.pipe(stdout);
    child.stderr.pipe(stderr);
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (signal) reject(new Error(`dev-loops terminated by ${signal}`));
      else resolve(code ?? 1);
    });
  });
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  runDevLoops().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[dev-loops] ${error.message}\n`);
    process.exitCode = 1;
  });
}
