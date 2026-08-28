#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import path from "node:path";

import { runDevLoopPreflight } from "../lib/dev-loop-preflight-core.mjs";
import { runManagedChild } from "../lib/managed-child-process.mjs";
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

export function assertNoPreflightBypass(env = process.env) {
  const value = env.DEVLOOPS_PREFLIGHT_BYPASS;
  if (value !== undefined && String(value).trim() !== "") {
    throw new Error("DEVLOOPS_PREFLIGHT_BYPASS is not permitted by the repository pre-flight wrapper");
  }
}

export async function runRepositoryPreflight(cwd, env = process.env) {
  assertNoPreflightBypass(env);
  const pi = {
    getAllTools: () => REPOSITORY_CONFIGURED_TOOLS,
    getActiveTools: () => DEV_LOOP_SELECTED_TOOLS,
  };
  let resolved;
  const result = await runDevLoopPreflight(pi, cwd, {
    env,
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
  const repositoryCheck = await runRepositoryPreflight(cwd, env);
  if (!repositoryCheck.ok) throw new Error(repositoryCheck.message);
  const script = path.join(repositoryCheck.resolved.packageRoot, "scripts", "loop", "pre-flight-gate.mjs");
  const childEnv = { ...env, DEVLOOPS_SUBAGENT_AVAILABLE: inferSubagentAvailability(env) };
  delete childEnv.DEVLOOPS_PREFLIGHT_BYPASS;
  return runManagedChild(process.execPath, [script, ...argv], {
    cwd,
    env: childEnv,
    stdout,
    stderr,
    label: "pre-flight gate",
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
