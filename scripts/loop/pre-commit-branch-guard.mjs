#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { accessSync, constants as fsConstants, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { runManagedChild } from "../lib/managed-child-process.mjs";
import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

export async function runBranchGuard(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  env = process.env,
  stdout = process.stdout,
  stderr = process.stderr,
  resolvePackage = resolveDevLoopsPackageRoot,
  runChild = runManagedChild,
} = {}) {
  const resolved = await resolvePackage({ cwd });
  const packageRoot = realpathSync(resolved.packageRoot);
  const script = path.join(packageRoot, "scripts", "loop", "pre-commit-branch-guard.mjs");
  try {
    accessSync(script, fsConstants.R_OK);
  } catch (error) {
    throw new Error(`reviewed dev-loops branch guard is unavailable: ${error.message}`, { cause: error });
  }
  const resolvedScript = realpathSync(script);
  if (path.dirname(resolvedScript) !== path.join(packageRoot, "scripts", "loop")) {
    throw new Error("reviewed dev-loops branch guard escapes the exact pinned package root");
  }
  return runChild(process.execPath, [resolvedScript, ...argv], {
    cwd,
    env,
    stdout,
    stderr,
    label: "pre-commit branch guard",
  });
}

function isDirectRun(metaUrl) {
  return process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(metaUrl));
}

if (isDirectRun(import.meta.url)) {
  runBranchGuard().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[pre-commit-branch-guard] ${error.message}\n`);
    process.exitCode = 1;
  });
}
