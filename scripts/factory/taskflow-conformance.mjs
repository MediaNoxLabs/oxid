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
  process.stderr.write("Usage: node scripts/factory/taskflow-conformance.mjs [--json] [--long-process] [--step-ms <positive integer>] [--heartbeat-ms <positive integer>]\n");
}

function parseArgs(argv) {
  let json = false;
  let longProcess = false;
  let stepMs = 80;
  let heartbeatMs = 30_000;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--json") json = true;
    else if (argv[index] === "--long-process") longProcess = true;
    else if (argv[index] === "--step-ms") {
      stepMs = Number(argv[++index]);
      if (!Number.isInteger(stepMs) || stepMs < 10 || stepMs > 600_000) throw new Error("--step-ms must be an integer from 10 to 600000");
    } else if (argv[index] === "--heartbeat-ms") {
      heartbeatMs = Number(argv[++index]);
      if (!Number.isInteger(heartbeatMs) || heartbeatMs < 10 || heartbeatMs > 60_000) throw new Error("--heartbeat-ms must be an integer from 10 to 60000");
    } else throw new Error(`unknown argument: ${argv[index]}`);
  }
  if (stepMs > 300_000 && !longProcess) throw new Error("--step-ms above 300000 requires explicit --long-process mode");
  return { json, longProcess, stepMs, heartbeatMs };
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

function processExists(pid) {
  if (!Number.isInteger(pid) || pid < 2) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

async function waitForProcessExit(pids, timeoutMs = 2_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (pids.every((pid) => !processExists(pid))) return true;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return pids.every((pid) => !processExists(pid));
}

async function executeProbe(core, state, events, dependencies = {}) {
  const { onObservedProgress, ...runtimeDependencies } = dependencies;
  return core.executeTaskflow(state, {
    ...runtimeDependencies,
    cwd: state.cwd,
    agents: [],
    usageAccounting: "unavailable",
    onProgress(current) {
      events.push({ observedAt: Date.now(), status: current.status, phases: Object.values(current.phases).map(({ id, status }) => `${id}:${status}`) });
      onObservedProgress?.(current);
    },
  });
}

async function runProbe(core, cwd, def, args, events, dependencies = {}) {
  return executeProbe(core, stateFor(def, cwd, args), events, {
    ...dependencies,
    cwd,
  });
}

async function main() {
  const { json, longProcess, stepMs, heartbeatMs } = parseArgs(process.argv.slice(2));
  const { core, versions } = await loadCore();
  const cwd = await mkdtemp(path.join(os.tmpdir(), "oxid-taskflow-conformance-"));
  try {
    const controlStepMs = Math.min(stepMs, 100);
    const events = [];
    const progressStartedAt = Date.now();
    const progressFlow = {
      name: "synthetic-progress", version: 1, scriptCwd: "invocation", incremental: false, concurrency: 1,
      phases: [script("progress", [process.execPath, "-e", `setTimeout(() => process.stdout.write('done'), ${stepMs})`], Math.max(1_000, stepMs * 8), { final: true })],
    };
    const admission = core.validateTaskflow(progressFlow);
    let heartbeatCount = 0;
    let latestPhase = "not-started";
    const heartbeat = longProcess ? setInterval(() => {
      heartbeatCount += 1;
      const elapsedMs = Date.now() - progressStartedAt;
      process.stderr.write(`[taskflow-conformance] supervisor-heartbeat elapsedMs=${elapsedMs} phase=${latestPhase}\n`);
    }, heartbeatMs) : null;
    heartbeat?.unref();
    let progress;
    try {
      progress = await runProbe(core, cwd, progressFlow, {}, events, {
        onObservedProgress(current) {
          latestPhase = Object.values(current.phases).map(({ id, status }) => `${id}:${status}`).join(",") || current.status;
        },
      });
    } finally {
      if (heartbeat) clearInterval(heartbeat);
    }
    const progressEndedAt = Date.now();
    const callbackTimes = [progressStartedAt, ...events.map(({ observedAt }) => observedAt), progressEndedAt].sort((a, b) => a - b);
    const maxCallbackGapMs = callbackTimes.slice(1).reduce((maximum, value, index) => Math.max(maximum, value - callbackTimes[index]), 0);
    const boundedProgress = progress.ok && events.length > 0 && maxCallbackGapMs <= 60_000;

    const timeoutFlow = {
      name: "synthetic-timeout", version: 1, scriptCwd: "invocation", incremental: false, concurrency: 1,
      phases: [script("slow", [process.execPath, "-e", `setTimeout(() => process.stdout.write('late'), ${controlStepMs * 8})`], controlStepMs, { final: true })],
    };
    const timedOut = await runProbe(core, cwd, timeoutFlow, {}, []);
    const processReceipt = path.join(cwd, "process-tree.txt");
    const processTreeFlow = {
      name: "synthetic-process-tree-timeout", version: 1, scriptCwd: "invocation", incremental: false, concurrency: 1,
      phases: [script("tree", ["/bin/sh", "-c", `sleep 60 & child=$!; printf '%s\\n%s\\n' $$ $child > '${processReceipt}'; wait`], 100, { final: true })],
    };
    const processTreeResult = await runProbe(core, cwd, processTreeFlow, {}, []);
    const processPids = (await readFile(processReceipt, "utf8"))
      .trim()
      .split("\n")
      .map(Number);
    const processTreeReaped = await waitForProcessExit(processPids);
    const parentSnapshot = JSON.stringify(timedOut.state);
    const resumeState = core.forkRunForResume(timedOut.state, {
      cwd,
      overrides: { phaseId: "slow", timeout: Math.max(1_000, controlStepMs * 16) },
    });
    const resumed = await executeProbe(core, resumeState, []);
    const immutableResume = timedOut.state.status === "failed"
      && resumed.ok
      && resumed.state.parentRunId === timedOut.state.runId
      && resumed.state.runId !== timedOut.state.runId
      && JSON.stringify(timedOut.state) === parentSnapshot;

    const resumeFlow = {
      name: "synthetic-input", version: 1, scriptCwd: "invocation", incremental: true, concurrency: 1,
      args: { value: { type: "string" } },
      phases: [{ id: "echo", type: "agent", task: "{args.value}", timeout: controlStepMs * 8, final: true, cache: { scope: "cross-run" } }],
    };
    const cacheStore = new core.CacheStore(cwd);
    const runTask = async (_cwd, _agents, agent, task) => ({
      agent, task, exitCode: 0, output: task, stderr: "", usage: core.emptyUsage(), completionSource: "process-exit",
    });
    const first = await runProbe(core, cwd, resumeFlow, { value: "one" }, [], { cacheStore, runTask });
    const changed = await runProbe(core, cwd, resumeFlow, { value: "two" }, [], { cacheStore, runTask });
    const repeated = await runProbe(core, cwd, resumeFlow, { value: "two" }, [], { cacheStore, runTask });
    const changedInputInvalidation = first.finalOutput === "one"
      && changed.finalOutput === "two"
      && changed.reuse?.executed === 1
      && changed.reuse?.reusedCrossRun === 0
      && repeated.reuse?.reusedCrossRun === 1;

    const matrix = [
      { property: "bounded-progress-visibility", status: boundedProgress ? "supported" : "unverified", evidence: `progress callbacks=${events.length}; maxCallbackGapMs=${maxCallbackGapMs}; terminal=${progress.state.status}` },
      { property: "slow-versus-stalled-classification", status: "unverified", evidence: timedOut.state.phases.slow?.timedOut ? "wall timeout is observable, but no distinct idle/stalled reason is exposed for script phases" : "no timeout classification" },
      { property: "process-tree-cancellation-escalation", status: processTreeResult.state.phases.tree?.timedOut && processTreeReaped ? "supported" : "unsupported", evidence: `timedOut=${Boolean(processTreeResult.state.phases.tree?.timedOut)}; descendantsReaped=${processTreeReaped}; observedPids=${processPids.length}` },
      { property: "terminal-cleanup", status: processTreeReaped ? "supported" : "unsupported", evidence: `processGroupLeaderAndChildAbsent=${processTreeReaped}` },
      { property: "immutable-resume", status: immutableResume ? "supported" : "unsupported", evidence: `parent=${timedOut.state.status}; child=${resumed.state.status}; parentUnchanged=${JSON.stringify(timedOut.state) === parentSnapshot}` },
      { property: "changed-input-invalidation", status: changedInputInvalidation ? "supported" : "unsupported", evidence: `changedExecuted=${changed.reuse?.executed ?? 0}; changedReused=${changed.reuse?.reusedCrossRun ?? 0}; repeatedReused=${repeated.reuse?.reusedCrossRun ?? 0}` },
    ];
    if (longProcess) {
      matrix.unshift(
        { property: "saved-flow-long-script-admission", status: admission.ok ? "supported" : "unsupported", evidence: admission.ok ? "validated by the pinned public schema" : "pinned schema rejects a script timeout above 300000ms" },
        { property: "direct-executor-long-script", status: progress.ok && progressEndedAt - progressStartedAt > 300_000 ? "supported" : "unverified", evidence: `durationMs=${progressEndedAt - progressStartedAt}; terminal=${progress.state.status}` },
      );
    }
    const report = {
      schemaVersion: 1,
      mode: longProcess ? "long-process-headless-black-box" : "synthetic-headless-black-box",
      versions,
      timings: {
        stepMs,
        realTimeOverFiveMinutes: progressEndedAt - progressStartedAt > 300_000,
        progressDurationMs: progressEndedAt - progressStartedAt,
        progressCallbacks: events.length,
        maxCallbackGapMs,
        supervisorHeartbeats: heartbeatCount,
      },
      matrix,
    };
    if (json) process.stdout.write(`${JSON.stringify(report)}\n`);
    else for (const row of matrix) process.stdout.write(`${row.status.toUpperCase()} ${row.property}: ${row.evidence}\n`);
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
}

main().catch((error) => { process.stderr.write(`[taskflow-conformance] ${error.message}\n`); process.exitCode = 1; });
