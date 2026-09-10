#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Repository-owned injection seam; all ready-for-review policy remains upstream.
import { pathToFileURL } from "node:url";
import path from "node:path";
import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { evaluateOxidPrSizeBudget } from "../loop/oxid-size-budget.mjs";

export async function main(argv = process.argv.slice(2), runtime = {}) {
  const repoRoot = runtime.repoRoot ?? process.cwd();
  const packageRoot = (await resolveDevLoopsPackageRoot({ cwd: repoRoot })).packageRoot;
  const { main: upstreamMain } = await import(pathToFileURL(path.join(packageRoot, "scripts/github/ready-for-review.mjs")).href);
  return upstreamMain(argv, { ...runtime, evaluatePrSizeBudget: runtime.evaluatePrSizeBudget ?? evaluateOxidPrSizeBudget });
}

if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1]) {
  main().then((code) => { process.exitCode = code; }).catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
