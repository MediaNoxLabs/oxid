#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "./lib/dev-loop-runtime.mjs";
import { enforceSingleBase, pinnedPublicRoute } from "./lib/pinned-dev-loops-args.mjs";

const INTEGRATION_BASE = "integration";

/** Force the base on dev-loops' public PR create route and deprecated alias. */
export function normalizeDevLoopsArgs(argv) {
  const args = [...argv];
  const route = pinnedPublicRoute(args);
  const isPrCreate = route.category === "pr" && (route.command === "create" || route.command === "create-draft");
  return enforceSingleBase(args, INTEGRATION_BASE, {
    addWhenMissing: isPrCreate,
    label: "repository dev-loops operations",
  });
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
    child.stdout.pipe(stdout, { end: false });
    child.stderr.pipe(stderr, { end: false });
    child.once("error", reject);
    child.once("close", (code, signal) => {
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
