#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

export function inferSubagentAvailability(env = process.env) {
  const explicit = env.DEVLOOPS_SUBAGENT_AVAILABLE?.trim();
  if (explicit === "0" || explicit === "1") return explicit;
  if (env.PI_SUBAGENT_CHILD !== "1") return "0";
  const depth = Number(env.PI_SUBAGENT_DEPTH);
  const maximum = Number(env.PI_SUBAGENT_MAX_DEPTH);
  return Number.isInteger(depth) && Number.isInteger(maximum) && depth < maximum ? "1" : "0";
}

export async function runPreFlightGate(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  env = process.env,
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const script = path.join(resolved.packageRoot, "scripts", "loop", "pre-flight-gate.mjs");
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
