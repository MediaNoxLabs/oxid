#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
import { spawnSync } from "node:child_process";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { normalizePrFactsOptionalSarif } from "../github/optional-sarif-policy.mjs";
import { runDevLoopsPackageScript } from "../lib/dev-loop-package-script.mjs";
import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

function runChild(command, args, env) {
  const result = spawnSync(command, args, { encoding: "utf8", env, maxBuffer: 16 * 1024 * 1024 });
  const normalized = {
    code: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? result.error?.message ?? "",
  };
  if (normalized.code === 0 && command === "gh" && args[0] === "pr" && args[1] === "view") {
    normalized.stdout = JSON.stringify(normalizePrFactsOptionalSarif(JSON.parse(normalized.stdout)));
  }
  return normalized;
}

export async function runOxidPrGateCoordination(argv, {
  cwd = process.cwd(), stdout = process.stdout, stderr = process.stderr,
} = {}) {
  const { packageRoot } = await resolveDevLoopsPackageRoot({ cwd });
  const load = (relative) => import(pathToFileURL(path.join(packageRoot, relative)).href);
  const [detector, output, helpers] = await Promise.all([
    load("scripts/loop/detect-pr-gate-coordination-state.mjs"),
    load("scripts/lib/jq-output.mjs"),
    load("scripts/_core-helpers.mjs"),
  ]);
  let options;
  try {
    options = detector.parseDetectPrGateCoordinationCliArgs(argv);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    return runDevLoopsPackageScript("scripts/loop/detect-pr-gate-coordination-state.mjs", argv, { cwd });
  }
  try {
    const result = await detector.detectPrGateCoordinationState(options, {
      cwd, repoRoot: cwd, env: process.env, runChild,
    });
    return output.emitResult(result, { jq: options.jq, silent: options.silent, stdout, stderr });
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
}

process.exitCode = await runOxidPrGateCoordination(process.argv.slice(2));
