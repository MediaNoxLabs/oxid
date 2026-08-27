// SPDX-License-Identifier: Apache-2.0

function timestamp(value) {
  const parsed = Date.parse(value ?? "");
  return Number.isFinite(parsed) ? parsed : 0;
}

function attemptOrder(entry) {
  const id = Number(entry?.id);
  return [Number.isSafeInteger(id) && id > 0 ? id : 0, timestamp(entry?.started_at ?? entry?.created_at)];
}

function newer(left, right) {
  const a = attemptOrder(left);
  const b = attemptOrder(right);
  return a[0] !== b[0] ? a[0] > b[0] : a[1] > b[1];
}

export function selectCurrentCheckAttempts(checkRuns) {
  if (!Array.isArray(checkRuns)) throw new Error("check_runs must be an array");
  const selected = new Map();
  for (const run of checkRuns) {
    if (!run || typeof run !== "object" || typeof run.name !== "string" || run.name.trim() === "") {
      throw new Error("check run is missing a non-empty name");
    }
    const app = String(run.app?.id ?? run.app?.slug ?? "unknown");
    const key = `${app}:${run.name}`;
    const prior = selected.get(key);
    if (!prior || newer(run, prior)) selected.set(key, run);
  }
  return [...selected.values()];
}

export function selectCurrentCommitStatuses(statuses) {
  if (!Array.isArray(statuses)) throw new Error("statuses must be an array");
  const selected = new Map();
  for (const status of statuses) {
    if (!status || typeof status !== "object" || typeof status.context !== "string" || status.context.trim() === "") {
      throw new Error("commit status is missing a non-empty context");
    }
    const prior = selected.get(status.context);
    if (!prior || newer(status, prior)) selected.set(status.context, status);
  }
  return [...selected.values()];
}

export function summarizeCurrentCi({ checkRuns, statuses }) {
  const currentRuns = selectCurrentCheckAttempts(checkRuns);
  const currentStatuses = selectCurrentCommitStatuses(statuses);
  const pendingRuns = currentRuns.filter((run) => String(run.status).toLowerCase() !== "completed");
  const failedRuns = currentRuns.filter((run) => {
    if (String(run.status).toLowerCase() !== "completed") return false;
    return !new Set(["success", "neutral", "skipped"]).has(String(run.conclusion).toLowerCase());
  });
  const pendingStatuses = currentStatuses.filter((status) => String(status.state).toLowerCase() === "pending");
  const failedStatuses = currentStatuses.filter((status) => ["failure", "error"].includes(String(status.state).toLowerCase()));
  const failedChecks = [
    ...failedRuns.map((run) => ({ name: run.name, conclusion: run.conclusion })),
    ...failedStatuses.map((status) => ({ name: status.context, conclusion: status.state })),
  ];
  if (failedChecks.length > 0) return { ciStatus: "failure", failedChecks };
  if (pendingRuns.length > 0 || pendingStatuses.length > 0) return { ciStatus: "pending", failedChecks: [] };
  if (currentRuns.length === 0 && currentStatuses.length === 0) return { ciStatus: "none", failedChecks: [] };
  return { ciStatus: "success", failedChecks: [] };
}
