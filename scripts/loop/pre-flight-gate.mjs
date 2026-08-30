#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import { once } from "node:events";
import path from "node:path";
import { StringDecoder } from "node:string_decoder";
import { Writable } from "node:stream";

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

const PACKAGE_RECOVERY_GUIDANCE = "(creates+provisions tmp/worktrees/dev-loops/<kind>-<n> from origin/main)";
const REPOSITORY_RECOVERY_GUIDANCE = "(creates+provisions tmp/worktrees/dev-loops/<kind>-<n> from origin/integration)";

/**
 * The pinned generic package describes its own main-based repository. Oxid's
 * wrapper is authoritative for this consumer. Rewrite only the package's exact
 * worktree-recovery sentence; legitimate diagnostics about main must remain
 * byte-for-byte accurate. Keep this at the package-output boundary so no
 * installed package is patched in place.
 */
export function createDeliveryBranchRewriteSink(destination) {
  const decoder = new StringDecoder("utf8");
  let pending = "";
  let flushed = false;
  const keep = PACKAGE_RECOVERY_GUIDANCE.length - 1;
  const rewrite = (value) => value.replaceAll(PACKAGE_RECOVERY_GUIDANCE, REPOSITORY_RECOVERY_GUIDANCE);
  const writeDestination = (value, callback) => {
    if (value.length === 0 || destination.write(value)) callback();
    else destination.once("drain", callback);
  };
  const stablePrefix = () => {
    pending = rewrite(pending);
    if (pending.length <= keep) return "";
    const value = pending.slice(0, pending.length - keep);
    pending = pending.slice(pending.length - keep);
    return value;
  };
  const sink = new Writable({
    write(chunk, encoding, callback) {
      const decoded = decoder.write(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, encoding));
      if (flushed) {
        writeDestination(rewrite(decoded), callback);
        return;
      }
      pending += decoded;
      writeDestination(stablePrefix(), callback);
    },
  });
  return {
    sink,
    async flush() {
      if (flushed) return;
      flushed = true;
      pending += decoder.end();
      const finalOutput = rewrite(pending);
      pending = "";
      if (finalOutput.length > 0 && !destination.write(finalOutput)) {
        await once(destination, "drain");
      }
    },
  };
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
  const rewrittenStdout = createDeliveryBranchRewriteSink(stdout);
  const rewrittenStderr = createDeliveryBranchRewriteSink(stderr);
  try {
    return await runManagedChild(process.execPath, [script, ...argv], {
      cwd,
      env: childEnv,
      stdout: rewrittenStdout.sink,
      stderr: rewrittenStderr.sink,
      label: "pre-flight gate",
    });
  } finally {
    await Promise.all([rewrittenStdout.flush(), rewrittenStderr.flush()]);
  }
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
