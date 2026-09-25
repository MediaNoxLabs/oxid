// SPDX-License-Identifier: Apache-2.0

export const OPTIONAL_SARIF_PROJECTIONS = Object.freeze([
  "Checkov",
  "Opengrep OSS",
  "Trivy",
  "gitleaks",
  "zizmor",
]);

const OPTIONAL_SARIF_PROJECTION_SET = new Set(OPTIONAL_SARIF_PROJECTIONS);
const PENDING_STATES = new Set(["IN_PROGRESS", "PENDING", "QUEUED", "WAITING"]);
const PASS_CONCLUSIONS = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);

function checkName(check) {
  return check?.name ?? check?.context ?? "";
}

function hasNoWorkflow(check) {
  return check?.workflow === "" || check?.workflowName === "";
}

function isSuccessful(check) {
  if (check?.bucket === "pass" || check?.bucket === "skipping") return true;
  const conclusion = String(check?.conclusion ?? "").toUpperCase();
  const status = String(check?.status ?? "").toUpperCase();
  const state = String(check?.state ?? "").toUpperCase();
  if (conclusion) return status === "COMPLETED" && PASS_CONCLUSIONS.has(conclusion);
  return state === "SUCCESS" || state === "SKIPPED";
}

function isPending(check) {
  if (check?.bucket === "pending") return PENDING_STATES.has(String(check?.state ?? "PENDING").toUpperCase());
  if (String(check?.conclusion ?? "").trim()) return false;
  return PENDING_STATES.has(String(check?.status ?? check?.state ?? "").toUpperCase());
}

/** Classify current-head checks using the one fail-closed optional-SARIF policy. */
export function classifyOptionalSarifChecks(checks) {
  if (!Array.isArray(checks)) {
    return { authoritativeScanGreen: false, ignored: [], retained: [], blockers: [] };
  }
  const scans = checks.filter((check) => checkName(check) === "scan");
  const authoritativeScanGreen = scans.length === 1 && isSuccessful(scans[0]);
  const ignored = [];
  const retained = [];
  for (const check of checks) {
    if (authoritativeScanGreen
      && OPTIONAL_SARIF_PROJECTION_SET.has(checkName(check))
      && hasNoWorkflow(check)
      && isPending(check)) {
      ignored.push(check);
    } else {
      retained.push(check);
    }
  }
  return {
    authoritativeScanGreen,
    ignored,
    retained,
    blockers: retained.filter((check) => !isSuccessful(check)),
  };
}

/** Preserve every PR fact while removing only projections proven optional. */
export function normalizePrFactsOptionalSarif(prData) {
  if (!prData || typeof prData !== "object") return prData;
  const statusCheckRollup = Array.isArray(prData.statusCheckRollup) ? prData.statusCheckRollup : [];
  const policy = classifyOptionalSarifChecks(statusCheckRollup);
  return policy.ignored.length === 0 ? prData : { ...prData, statusCheckRollup: policy.retained };
}
