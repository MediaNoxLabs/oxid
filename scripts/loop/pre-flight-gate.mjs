#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { runDevLoopPreflight } from "../lib/dev-loop-preflight-core.mjs";
import {
  DEV_LOOP_SELECTED_TOOLS,
  REPOSITORY_CONFIGURED_TOOLS,
  resolveDevLoopsPackageRoot,
} from "../lib/dev-loop-runtime.mjs";

export function inferSubagentAvailability(env = process.env) {
  const hasPiMarkers = ["PI_SUBAGENT_CHILD", "PI_SUBAGENT_DEPTH", "PI_SUBAGENT_MAX_DEPTH"]
    .some((name) => env[name] !== undefined);
  if (hasPiMarkers) {
    const depth = Number(env.PI_SUBAGENT_DEPTH);
    const maximum = Number(env.PI_SUBAGENT_MAX_DEPTH);
    return env.PI_SUBAGENT_CHILD === "1"
      && Number.isInteger(depth) && depth >= 0
      && Number.isInteger(maximum) && maximum > 0
      && depth < maximum ? "1" : "0";
  }
  const explicit = env.DEVLOOPS_SUBAGENT_AVAILABLE?.trim();
  return explicit === "1" ? "1" : "0";
}

export async function runRepositoryPreflight(cwd) {
  const pi = {
    getAllTools: () => REPOSITORY_CONFIGURED_TOOLS,
    getActiveTools: () => DEV_LOOP_SELECTED_TOOLS,
  };
  let resolved;
  const result = await runDevLoopPreflight(pi, cwd, {
    activeAgent: "dev-loop",
    activeTools: DEV_LOOP_SELECTED_TOOLS,
    resolve: async (options) => {
      resolved = await resolveDevLoopsPackageRoot(options);
      return resolved;
    },
  });
  return { ...result, resolved };
}

export async function runPreFlightGate(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  env = process.env,
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  if (env.DEVLOOPS_PREFLIGHT_BYPASS?.trim() === "1") {
    throw new Error("DEVLOOPS_PREFLIGHT_BYPASS is not permitted by the repository pre-flight wrapper");
  }
  const repositoryCheck = await runRepositoryPreflight(cwd);
  if (!repositoryCheck.ok) throw new Error(repositoryCheck.message);
  const script = path.join(repositoryCheck.resolved.packageRoot, "scripts", "loop", "pre-flight-gate.mjs");
  const childEnv = { ...env, DEVLOOPS_SUBAGENT_AVAILABLE: inferSubagentAvailability(env) };
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [script, ...argv], {
      cwd,
      env: childEnv,
      stdio: ["inherit", "pipe", "pipe"],
    });
    child.stdout.pipe(stdout, { end: false });
    child.stderr.pipe(stderr, { end: false });
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (signal) reject(new Error(`pre-flight gate terminated by ${signal}`));
      else resolve(code ?? 1);
    });
  });
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  runPreFlightGate().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[pre-flight-gate] ${error.message}\n`);
    process.exitCode = 1;
  });
}
