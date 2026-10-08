// SPDX-License-Identifier: Apache-2.0

export const OPTIONAL_SARIF_PROJECTIONS = Object.freeze([
  "Checkov",
  "Opengrep OSS",
  "Trivy",
  "gitleaks",
  "zizmor",
]);

export const CRITICAL_CHECKS = Object.freeze([
  "Validate PR title",
  "Validate PR body",
  "Verify commit sign-offs",
  "Repository gate (fmt, architecture, lint, tests, coverage)",
  "Locked Nix package and Compact artifacts",
  "Audit, Licenses, Sources, and Documentation",
  "scan",
]);

const OPTIONAL_SARIF_PROJECTION_SET = new Set(OPTIONAL_SARIF_PROJECTIONS);
const PENDING_STATES = new Set(["IN_PROGRESS", "PENDING", "QUEUED", "WAITING"]);
const PASS_CONCLUSIONS = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);

function checkName(check) {
  return check?.name ?? check?.context ?? "";
}

function hasNoWorkflow(check) {
  if (Object.hasOwn(check ?? {}, "workflow")) return check.workflow === "";
  if (Object.hasOwn(check ?? {}, "workflowName")) return check.workflowName === "";
  return false;
}

function isStrictSuccess(check) {
  if (check?.bucket === "pass") return true;
  const conclusion = String(check?.conclusion ?? "").toUpperCase();
  const status = String(check?.status ?? "").toUpperCase();
  const state = String(check?.state ?? "").toUpperCase();
  if (conclusion) return status === "COMPLETED" && conclusion === "SUCCESS";
  return state === "SUCCESS";
}

function isSuccessfulOrSkipped(check) {
  if (check?.bucket === "skipping") return true;
  if (isStrictSuccess(check)) return true;
  const conclusion = String(check?.conclusion ?? "").toUpperCase();
  const status = String(check?.status ?? "").toUpperCase();
  const state = String(check?.state ?? "").toUpperCase();
  if (conclusion) return status === "COMPLETED" && PASS_CONCLUSIONS.has(conclusion);
  return state === "SKIPPED";
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
  const authoritativeScanGreen = scans.length > 0 && scans.every(isStrictSuccess);
  const criticalChecksGreen = CRITICAL_CHECKS.every((name) => {
    const matches = checks.filter((check) => checkName(check) === name);
    return matches.length > 0 && matches.every(isStrictSuccess);
  });
  const ignored = [];
  const retained = [];
  for (const check of checks) {
    if (authoritativeScanGreen && criticalChecksGreen
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
    criticalChecksGreen,
    ignored,
    retained,
    blockers: retained.filter((check) => !isSuccessfulOrSkipped(check)),
  };
}

/** Preserve every PR fact while removing only projections proven optional. */
export function normalizePrFactsOptionalSarif(prData) {
  if (!prData || typeof prData !== "object") return prData;
  const statusCheckRollup = Array.isArray(prData.statusCheckRollup) ? prData.statusCheckRollup : [];
  const policy = classifyOptionalSarifChecks(statusCheckRollup);
  return policy.ignored.length === 0 ? prData : { ...prData, statusCheckRollup: policy.retained };
}
