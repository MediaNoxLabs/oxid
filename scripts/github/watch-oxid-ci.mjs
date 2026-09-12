// SPDX-License-Identifier: Apache-2.0

import { setTimeout as delay } from "node:timers/promises";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { GITHUB_REST_HEADERS, runGhCommand } from "./rest-client.mjs";

function isNoneTerminal(result) {
  return result?.ciStatus === "none" && result.status === "success" && result.settled === true;
}

function isSupersessionPending(result) {
  return result?.status === "pending" && result.settled === false && result.workflowAttemptSelection !== undefined;
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

function actionRunId(detailsUrl) {
  const match = typeof detailsUrl === "string" && detailsUrl.match(/\/actions\/runs\/(\d+)(?:\/|$)/);
  return match ? Number(match[1]) : null;
}

function isActiveWorkflowRun(run) {
  return run?.status === "queued" || run?.status === "in_progress";
}

function isSuccessfulWorkflowRun(run) {
  return run?.status === "completed" && run?.conclusion === "success";
}

const FAILURE_CONCLUSIONS = new Set([
  "failure", "cancelled", "timed_out", "action_required", "startup_failure", "stale",
]);

function isFailedCheckRun(check) {
  return check?.status === "completed" && FAILURE_CONCLUSIONS.has(check?.conclusion);
}

function newestWorkflowRun(runs) {
  return [...runs].sort((left, right) => (
    Number(right.run_number ?? 0) - Number(left.run_number ?? 0)
    || Number(right.id ?? 0) - Number(left.id ?? 0)
  ))[0];
}

function isNewerWorkflowRun(candidate, prior) {
  const candidateNumber = Number(candidate?.run_number);
  const priorNumber = Number(prior?.run_number);
  if (Number.isInteger(candidateNumber) && Number.isInteger(priorNumber)) return candidateNumber > priorNumber;
  return Number(candidate?.id) > Number(prior?.id);
}

function loadWorkflowAttemptData({ repo, headSha }) {
  const request = (path) => JSON.parse(runGhCommand("gh", [
    "api", path, ...GITHUB_REST_HEADERS,
  ], { failureLabel: "GitHub Actions workflow-attempt request" }));
  const checkRuns = request(`repos/${repo}/commits/${headSha}/check-runs?per_page=100`).check_runs;
  const workflowRuns = request(`repos/${repo}/actions/runs?head_sha=${headSha}&per_page=100`).workflow_runs;
  if (!Array.isArray(checkRuns) || !Array.isArray(workflowRuns)) {
    throw new Error("GitHub Actions workflow-attempt response was malformed");
  }
  return { checkRuns, workflowRuns };
}

/**
 * Ignore an Actions failure only when every failed check is tied to an older
 * same-head run and that workflow's newest bounded attempt has replaced it.
 */
export function reconcileSupersededWorkflowFailure(result, options, { loadWorkflowAttempts = loadWorkflowAttemptData } = {}) {
  if (result?.status !== "failure" || result?.settled !== true || !Array.isArray(result.failedChecks) || result.failedChecks.length === 0) {
    return result;
  }
  try {
    const { checkRuns, workflowRuns } = loadWorkflowAttempts({ repo: options.repo, headSha: result.headSha });
    const failedCheckNames = result.failedChecks.map(({ name }) => name).filter((name) => typeof name === "string");
    if (failedCheckNames.length !== result.failedChecks.length) return result;
    const failedNames = new Set(failedCheckNames);
    const matchingFailures = checkRuns.filter((check) => failedNames.has(check?.name) && isFailedCheckRun(check));
    if ([...failedNames].some((name) => !matchingFailures.some((check) => check.name === name))) return result;
    if (matchingFailures.some((check) => check?.app?.slug !== "github-actions")) return result;
    const failedActionRunIds = [...new Set(matchingFailures
      .map((check) => actionRunId(check.details_url))
      .filter((id) => id !== null))];
    if (failedActionRunIds.length === 0 || matchingFailures.some((check) => actionRunId(check.details_url) === null)) return result;

    const runsById = new Map(workflowRuns.map((run) => [run?.id, run]));
    const failedRuns = failedActionRunIds.map((id) => runsById.get(id));
    if (failedRuns.some((run) => !run || !Number.isInteger(run.workflow_id))) return result;

    const replacements = failedRuns.map((failedRun) => newestWorkflowRun(workflowRuns.filter((candidate) => (
      candidate?.workflow_id === failedRun.workflow_id && isNewerWorkflowRun(candidate, failedRun)
    ))));
    if (replacements.some((run) => !run)) return result;
    const diagnostic = {
      examinedRuns: workflowRuns.length,
      supersededFailedRunIds: failedRuns.map(({ id }) => id),
      selectedReplacementRunIds: replacements.map(({ id }) => id),
    };
    if (replacements.some(isActiveWorkflowRun)) {
      return { ...result, status: "pending", settled: false, ciStatus: "pending", workflowAttemptSelection: diagnostic };
    }
    if (replacements.every(isSuccessfulWorkflowRun)) {
      const supersededIds = new Set(failedRuns.map(({ id }) => id));
      const currentChecks = checkRuns.filter((check) => !supersededIds.has(actionRunId(check.details_url)));
      if (currentChecks.some((check) => check?.status === "queued" || check?.status === "in_progress")) {
        return { ...result, status: "pending", settled: false, ciStatus: "pending", workflowAttemptSelection: diagnostic };
      }
      if (currentChecks.some(isFailedCheckRun)) return result;
      return { ...result, status: "success", settled: true, ciStatus: "success", failedChecks: [], workflowAttemptSelection: diagnostic };
    }
    return result;
  } catch {
    // The original settled failure is the fail-closed result for API/parse errors.
    return result;
  }
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
    now = performance.now.bind(performance),
    loadWorkflowAttempts,
    ...watchDependencies
  },
) {
  const reconcile = (result) => reconcileSupersededWorkflowFailure(result, options, { loadWorkflowAttempts });
  const startedAtMs = now();
  const initial = await watchCiStatus(options, watchDependencies);
  const reconciledInitial = reconcile(initial);
  if (!isNoneTerminal(initial) && !isSupersessionPending(reconciledInitial)) return reconciledInitial;

  // A zero-budget read cannot establish that Oxid's mandatory workflows had
  // time to register. Report non-green without adding a wait beyond the
  // caller's explicit single-observation budget.
  if (options.timeoutMs === 0) {
    return isNoneTerminal(initial) ? { ...initial, status: "pending", settled: false } : reconciledInitial;
  }

  const baselineSha = initial.headSha;
  let attempts = initial.attempts;
  let latest = reconciledInitial;

  // The upstream watcher consumed its own generic no-check grace before it
  // returned `none`. Re-observe only the registration gap, one bounded poll at
  // a time. A real pending/success/failure result resumes upstream delegation.
  while (true) {
    const remaining = remainingTimeoutMs(startedAtMs, options.timeoutMs, now);
    if (remaining === 0) return timedOutNoChecks(latest, attempts);
    await delayImpl(Math.min(options.pollIntervalMs, remaining));
    const observed = await watchCiStatus({ ...options, timeoutMs: 0 }, watchDependencies);
    attempts += observed.attempts;
    const observedWithAttempts = { ...observed, attempts };
    latest = reconcile(observedWithAttempts);
    if (observed.headSha !== baselineSha) return changedResult(observedWithAttempts);
    if (isNoneTerminal(observed)) continue;
    if (isSupersessionPending(latest)) continue;

    const remainingAfterObservation = remainingTimeoutMs(startedAtMs, options.timeoutMs, now);
    if (remainingAfterObservation === 0) return latest;
    const resumed = await watchCiStatus({
      ...options,
      timeoutMs: remainingAfterObservation,
    }, watchDependencies);
    if (resumed.headSha !== baselineSha) return changedResult(resumed);
    const reconciledResumed = reconcile(resumed);
    if (isSupersessionPending(reconciledResumed)) {
      latest = reconciledResumed;
      continue;
    }
    return reconciledResumed;
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
