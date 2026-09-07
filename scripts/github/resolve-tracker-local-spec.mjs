#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { realpath } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

export async function runResolveTrackerLocalSpec(
  argv = process.argv.slice(2),
  { cwd = process.cwd(), stdout = process.stdout, stderr = process.stderr, env = process.env, ghCommand = "gh" } = {},
) {
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const helper = await realpath(path.join(resolved.packageRoot, "scripts", "github", "resolve-tracker-local-spec.mjs"));
  if (!isContained(resolved.packageRoot, helper)) {
    throw new Error(`tracker spec helper resolves outside the exact dev-loops package root: ${helper}`);
  }
  const module = await import(pathToFileURL(helper).href);
  await module.runCli(argv, { stdout, stderr, env, ghCommand });
  return process.exitCode ?? 0;
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  runResolveTrackerLocalSpec().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[resolve-tracker-local-spec] ${error.message}\n`);
    process.exitCode = 1;
  });
}
