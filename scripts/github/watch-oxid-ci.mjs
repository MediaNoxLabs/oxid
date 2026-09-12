// SPDX-License-Identifier: Apache-2.0

import { setTimeout as delay } from "node:timers/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

function isNoneTerminal(result) {
  return result?.ciStatus === "none" && result.status === "success" && result.settled === true;
}

function changedResult(result) {
  return { ...result, status: "changed", settled: false };
}

function timedOutNoChecks(result, attempts) {
  return { ...result, status: "timeout", settled: false, attempts };
}

function remainingTimeoutMs(startedAtMs, timeoutMs, now) {
  return Math.max(0, timeoutMs - Math.max(0, now() - startedAtMs));
}

/**
 * Oxid's mandatory contribution and PR-metadata workflows mean a PR is never
 * genuinely checkless. Keep the upstream watcher authoritative for every real
 * check state; only prevent its generic `none` terminal result from becoming
 * green while those workflows may still be registering on the same head.
 */
export async function watchOxidPrCiStatus(
  options,
  {
    watchCiStatus,
    delayImpl = delay,
    now = Date.now,
    ...watchDependencies
  },
) {
  const startedAtMs = now();
  const initial = await watchCiStatus(options, watchDependencies);
  if (!isNoneTerminal(initial)) return initial;

  // A zero-budget read cannot establish that Oxid's mandatory workflows had
  // time to register. Report non-green without adding a wait beyond the
  // caller's explicit single-observation budget.
  if (options.timeoutMs === 0) return { ...initial, status: "pending", settled: false };

  const baselineSha = initial.headSha;
  let attempts = initial.attempts;
  let latest = initial;

  // The upstream watcher consumed its own generic no-check grace before it
  // returned `none`. Re-observe only the registration gap, one bounded poll at
  // a time. A real pending/success/failure result resumes upstream delegation.
  while (true) {
    const remaining = remainingTimeoutMs(startedAtMs, options.timeoutMs, now);
    if (remaining === 0) return timedOutNoChecks(latest, attempts);
    await delayImpl(Math.min(options.pollIntervalMs, remaining));
    const observed = await watchCiStatus({ ...options, timeoutMs: 0 }, watchDependencies);
    attempts += observed.attempts;
    latest = { ...observed, attempts };
    if (observed.headSha !== baselineSha) return changedResult(latest);
    if (isNoneTerminal(observed)) continue;

    const remainingAfterObservation = remainingTimeoutMs(startedAtMs, options.timeoutMs, now);
    if (remainingAfterObservation === 0) return observed;
    const resumed = await watchCiStatus({
      ...options,
      timeoutMs: remainingAfterObservation,
    }, watchDependencies);
    return resumed.headSha !== baselineSha ? changedResult(resumed) : resumed;
  }
}

async function loadPinnedWatcher(packageRoot) {
  const fromPackage = (relativePath) => import(pathToFileURL(path.join(packageRoot, relativePath)).href);
  const [watcher, output, helpers] = await Promise.all([
    fromPackage(path.join("scripts", "github", "probe-ci-status.mjs")),
    fromPackage(path.join("scripts", "lib", "jq-output.mjs")),
    fromPackage(path.join("scripts", "_core-helpers.mjs")),
  ]);
  return { watcher, output, helpers };
}

/** Run the narrow Oxid policy adapter while retaining the pinned CLI parser and emitter. */
export async function runOxidPrCiWatch(
  argv,
  {
    cwd = process.cwd(),
    stdout = process.stdout,
    stderr = process.stderr,
    loadPinnedWatcher: loadPinnedWatcherImpl = loadPinnedWatcher,
  } = {},
) {
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const { watcher, output, helpers } = await loadPinnedWatcherImpl(resolved.packageRoot);
  let options;
  try {
    options = watcher.parseCiWatchCliArgs(argv);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    await watcher.runCli(argv, { stdout, stderr });
    return 0;
  }
  if (options.pr === undefined || options.commit !== undefined) {
    throw new Error("Oxid PR CI adapter requires exactly one --pr and no --commit");
  }
  try {
    const result = await watchOxidPrCiStatus(options, { watchCiStatus: watcher.watchCiStatus });
    return output.emitResult(result, { jq: options.jq, silent: options.silent, stdout, stderr });
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
}
