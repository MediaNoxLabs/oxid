#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

/**
 * Fast, synthetic black-box probe for the repository-pinned Taskflow runtime.
 * It deliberately uses only short-lived child Node processes and a temporary
 * directory: it does not enable the Pi extension or touch factory resources.
 */
import { createRequire } from "node:module";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const EXPECTED_VERSION = "0.2.10";

function usage() {
  process.stderr.write("Usage: node scripts/factory/taskflow-conformance.mjs [--json] [--step-ms <positive integer>]\n");
}

function parseArgs(argv) {
  let json = false;
  let stepMs = 80;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--json") json = true;
    else if (argv[index] === "--step-ms") {
      stepMs = Number(argv[++index]);
      if (!Number.isInteger(stepMs) || stepMs < 10 || stepMs > 600_000) throw new Error("--step-ms must be an integer from 10 to 600000");
    } else throw new Error(`unknown argument: ${argv[index]}`);
  }
  return { json, stepMs };
}

async function loadCore() {
  const resolved = await resolveDevLoopsPackageRoot({ cwd: root, includeAllPinnedPackages: true });
  const adapter = resolved.packageRoots.find(({ name }) => name === "pi-taskflow");
  if (!adapter || adapter.version !== EXPECTED_VERSION) throw new Error(`expected pi-taskflow@${EXPECTED_VERSION}`);
  const require = createRequire(path.join(adapter.packageRoot, "package.json"));
  const entry = require.resolve("taskflow-core");
  const manifest = JSON.parse(await readFile(path.join(path.dirname(entry), "..", "package.json"), "utf8"));
  if (manifest.name !== "taskflow-core" || manifest.version !== EXPECTED_VERSION) throw new Error(`expected taskflow-core@${EXPECTED_VERSION}`);
  return { core: await import(pathToFileURL(entry).href), versions: { piTaskflow: adapter.version, taskflowCore: manifest.version } };
}

function stateFor(def, cwd, args = {}) {
  const now = Date.now();
  return { runId: `conformance-${now}`, flowName: def.name, def, args, status: "running", phases: {}, createdAt: now, updatedAt: now, cwd };
}

function script(id, run, timeout, extra = {}) {
  return { id, type: "script", run, timeout, idempotent: true, cache: { scope: "off" }, ...extra };
}

async function runProbe(core, cwd, def, args, events) {
  return core.executeTaskflow(stateFor(def, cwd, args), {
    cwd,
    agents: [],
    usageAccounting: "unavailable",
    onProgress(state) { events.push({ status: state.status, phases: Object.values(state.phases).map(({ id, status }) => `${id}:${status}`) }); },
  });
}

async function main() {
  const { json, stepMs } = parseArgs(process.argv.slice(2));
  const { core, versions } = await loadCore();
  const cwd = await mkdtemp(path.join(os.tmpdir(), "oxid-taskflow-conformance-"));
  try {
    const events = [];
    const progressFlow = {
      name: "synthetic-progress", version: 1, scriptCwd: "invocation", incremental: false, concurrency: 1,
      phases: [script("progress", [process.execPath, "-e", `setTimeout(() => process.stdout.write('done'), ${stepMs})`], stepMs * 8, { final: true })],
    };
    const progress = await runProbe(core, cwd, progressFlow, {}, events);

    const timeoutFlow = {
      name: "synthetic-timeout", version: 1, scriptCwd: "invocation", incremental: false, concurrency: 1,
      phases: [script("slow", [process.execPath, "-e", `setTimeout(() => process.stdout.write('late'), ${stepMs * 8})`], stepMs, { final: true })],
    };
    const timedOut = await runProbe(core, cwd, timeoutFlow, {}, []);

    const resumeFlow = {
      name: "synthetic-input", version: 1, scriptCwd: "invocation", incremental: true, concurrency: 1,
      args: { value: { type: "string" } },
      phases: [script("echo", [process.execPath, "-e", "process.stdout.write(process.argv[1])", "{args.value}"], stepMs * 8, { final: true, cache: { scope: "project" } })],
    };
    const first = await runProbe(core, cwd, resumeFlow, { value: "one" }, []);
    const second = await runProbe(core, cwd, resumeFlow, { value: "two" }, []);
    const immutableResume = first.finalOutput === "one" && second.finalOutput === "two" && first.state !== second.state;

    const matrix = [
      { property: "bounded-progress-visibility", status: progress.ok && events.length > 0 ? "supported" : "unverified", evidence: `progress callbacks=${events.length}; terminal=${progress.state.status}` },
      { property: "slow-versus-stalled-classification", status: timedOut.state.phases.slow?.timedOut ? "supported" : "unverified", evidence: timedOut.state.phases.slow?.error ?? "no timeout classification" },
      { property: "process-tree-cancellation-escalation", status: "unverified", evidence: "public runtime result exposes timeout, but does not expose child-tree reap or SIGKILL escalation evidence" },
      { property: "terminal-cleanup", status: "unverified", evidence: "public runtime result has no process-registry cleanup observation" },
      { property: "immutable-resume", status: immutableResume ? "supported" : "unsupported", evidence: `first=${first.finalOutput}; second=${second.finalOutput}; distinctState=${first.state !== second.state}` },
      { property: "changed-input-invalidation", status: second.finalOutput === "two" ? "supported" : "unsupported", evidence: `changed cached-flow argument produced ${second.finalOutput}` },
    ];
    const report = { schemaVersion: 1, mode: "synthetic-headless-black-box", versions, timings: { stepMs, realTimeOverFiveMinutes: "on-demand only" }, matrix };
    if (json) process.stdout.write(`${JSON.stringify(report)}\n`);
    else for (const row of matrix) process.stdout.write(`${row.status.toUpperCase()} ${row.property}: ${row.evidence}\n`);
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
}

main().catch((error) => { process.stderr.write(`[taskflow-conformance] ${error.message}\n`); process.exitCode = 1; });
