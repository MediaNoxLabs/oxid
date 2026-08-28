#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { link, open, lstat, mkdir, readdir, realpath, rename, unlink } from "node:fs/promises";
import { constants as fsConstants, realpathSync } from "node:fs";
import { createHash, randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { HOSTED_TARGETS, HostedTarget } from "../ci/target-plan.mjs";

export const METRICS_SCHEMA_VERSION = 1;
export const METRICS_REPOSITORY = "MediaNoxLabs/oxid";
const MAX_SAFE_COUNT = 1_000_000_000_000_000;
const MAX_METRIC_ENTRIES = 64;
const MAX_RECORD_BYTES = 256 * 1024;
const MAX_RECORD_FILES = 10_000;
const MAX_SECRET_SCAN_DEPTH = 8;
const MAX_FUTURE_SKEW_MS = 5 * 60_000;
const RETENTION_MS = 90 * 24 * 60 * 60_000;
const SECRET_VALUE = /(?:github_pat_|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|\bBearer\s+\S+|openid-credential-offer:\/\/|credential_offer(?:_uri)?=|\bdid:[a-z0-9]+:[A-Za-z0-9._:%-]+)/i;
const SAFE_NAME = /^[a-z0-9][a-z0-9._:-]{0,63}$/;
export const METRIC_KEYS = Object.freeze({
  top: Object.freeze([
    "schemaVersion", "repository", "issue", "pr", "headSha", "startedAt", "completedAt", "recordedAt",
    "phases", "validations", "review", "tokens", "attempts", "ci", "worktree", "routing",
  ]),
  phase: Object.freeze(["developmentMs", "reviewMs", "validationMs", "ciMs", "totalElapsedMs"]),
  validation: Object.freeze(["name", "durationMs", "outcome"]),
  review: Object.freeze(["sessions", "turns", "toolCalls", "externalReviewRequired"]),
  tokens: Object.freeze(["input", "output", "cacheRead", "cacheWrite"]),
  attempts: Object.freeze(["pushesAfterFirstCi", "canceled", "failed"]),
  ci: Object.freeze(["wallTimeMs", "requiredChecks", "failedChecks", "canceledRuns", "checks"]),
  ciCheck: Object.freeze(["name", "queueMs", "durationMs", "outcome"]),
  worktree: Object.freeze(["peakTargetBytes", "peakWorktreeBytes"]),
  routing: Object.freeze(["profile", "areas", "targets"]),
});

export const CI_TARGET_BUDGET_MS = Object.freeze({
  [HostedTarget.BASIC]: 5 * 60_000,
  [HostedTarget.UNIT_LINUX]: 10 * 60_000,
  [HostedTarget.HEADLESS_LINUX]: 10 * 60_000,
  [HostedTarget.UI_LINUX]: 20 * 60_000,
  [HostedTarget.UI_RELEASE_LINUX]: 25 * 60_000,
  [HostedTarget.COVERAGE_LINUX]: 25 * 60_000,
  [HostedTarget.QUALITY]: 20 * 60_000,
  [HostedTarget.NIX_PACKAGE]: 45 * 60_000,
  [HostedTarget.COMPACT_ARTIFACTS]: 30 * 60_000,
});

if (Object.keys(CI_TARGET_BUDGET_MS).length !== HOSTED_TARGETS.length
  || HOSTED_TARGETS.some((target) => !Number.isSafeInteger(CI_TARGET_BUDGET_MS[target]))) {
  throw new Error("every hosted CI target must have one explicit supervisor budget");
}

function error(errors, pathName, code, message) {
  errors.push({ path: pathName, code, message });
}

function objectAt(value, pathName, keys, errors) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    error(errors, pathName, "type", "must be an object");
    return null;
  }
  const actual = Object.keys(value);
  for (const key of keys) if (!actual.includes(key)) error(errors, `${pathName}.${key}`, "missing", "is required");
  for (const key of actual) if (!keys.includes(key)) error(errors, `${pathName}.<unknown>`, "unknown", "an unrecognized property is not allowed");
  return value;
}

function nonNegativeInteger(value, pathName, errors) {
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_SAFE_COUNT) {
    error(errors, pathName, "range", "must be a finite non-negative safe integer");
    return false;
  }
  return true;
}

function nullableNonNegativeInteger(value, pathName, errors) {
  return value === null || nonNegativeInteger(value, pathName, errors);
}

function positiveInteger(value, pathName, errors) {
  if (!Number.isSafeInteger(value) || value < 1 || value > MAX_SAFE_COUNT) {
    error(errors, pathName, "range", "must be a positive safe integer");
    return false;
  }
  return true;
}

function canonicalTimestamp(value, pathName, errors) {
  if (typeof value !== "string") {
    error(errors, pathName, "type", "must be an ISO-8601 UTC timestamp");
    return null;
  }
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== value) {
    error(errors, pathName, "format", "must be a canonical ISO-8601 UTC timestamp");
    return null;
  }
  return milliseconds;
}

function inspectSecretValues(value, pathName, errors, depth = 0) {
  if (depth > MAX_SECRET_SCAN_DEPTH) {
    error(errors, pathName, "depth", `must not exceed ${MAX_SECRET_SCAN_DEPTH} nested levels`);
    return;
  }
  if (typeof value === "string" && SECRET_VALUE.test(value)) {
    error(errors, pathName, "secret", "must not contain credentials, identifiers, prompts, or secret-bearing values");
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => inspectSecretValues(entry, `${pathName}[${index}]`, errors, depth + 1));
    return;
  }
  if (value && typeof value === "object") {
    for (const entry of Object.values(value)) inspectSecretValues(entry, `${pathName}.<value>`, errors, depth + 1);
  }
}

function safeStringList(value, pathName, errors, { nonEmpty = false } = {}) {
  if (!Array.isArray(value)) {
    error(errors, pathName, "type", "must be an array");
    return;
  }
  if (nonEmpty && value.length === 0) error(errors, pathName, "empty", "must contain at least one value");
  if (value.length > MAX_METRIC_ENTRIES) error(errors, pathName, "size", `must contain at most ${MAX_METRIC_ENTRIES} values`);
  if (new Set(value).size !== value.length) error(errors, pathName, "duplicate", "must not contain duplicates");
  value.forEach((entry, index) => {
    if (typeof entry !== "string" || !SAFE_NAME.test(entry)) {
      error(errors, `${pathName}[${index}]`, "format", "must be a bounded lowercase metric identifier");
    }
  });
}

export function validateMetricRecord(candidate, { nowMs = Date.now() } = {}) {
  const errors = [];
  const record = objectAt(candidate, "$", METRIC_KEYS.top, errors);
  if (!record) return { ok: false, errors };

  if (record.schemaVersion !== METRICS_SCHEMA_VERSION) error(errors, "$.schemaVersion", "version", `must equal ${METRICS_SCHEMA_VERSION}`);
  if (record.repository !== METRICS_REPOSITORY) error(errors, "$.repository", "repository", `must equal ${METRICS_REPOSITORY}`);
  positiveInteger(record.issue, "$.issue", errors);
  if (record.pr !== null) positiveInteger(record.pr, "$.pr", errors);
  if (typeof record.headSha !== "string" || !/^[0-9a-f]{40}$/.test(record.headSha)) {
    error(errors, "$.headSha", "format", "must be an exact lowercase 40-character Git SHA");
  }
  const startedAt = canonicalTimestamp(record.startedAt, "$.startedAt", errors);
  const completedAt = canonicalTimestamp(record.completedAt, "$.completedAt", errors);
  const recordedAt = canonicalTimestamp(record.recordedAt, "$.recordedAt", errors);

  const phases = objectAt(record.phases, "$.phases", METRIC_KEYS.phase, errors);
  if (phases) for (const key of METRIC_KEYS.phase) nonNegativeInteger(phases[key], `$.phases.${key}`, errors);

  if (!Array.isArray(record.validations)) {
    error(errors, "$.validations", "type", "must be an array");
  } else {
    if (record.validations.length > MAX_METRIC_ENTRIES) error(errors, "$.validations", "size", `must contain at most ${MAX_METRIC_ENTRIES} values`);
    const names = [];
    record.validations.forEach((entry, index) => {
      const validation = objectAt(entry, `$.validations[${index}]`, METRIC_KEYS.validation, errors);
      if (!validation) return;
      if (typeof validation.name !== "string" || !SAFE_NAME.test(validation.name)) {
        error(errors, `$.validations[${index}].name`, "format", "must be a bounded lowercase metric identifier, not a raw command");
      } else names.push(validation.name);
      nonNegativeInteger(validation.durationMs, `$.validations[${index}].durationMs`, errors);
      if (!["passed", "failed", "canceled"].includes(validation.outcome)) {
        error(errors, `$.validations[${index}].outcome`, "enum", "must be passed, failed, or canceled");
      }
    });
    if (new Set(names).size !== names.length) error(errors, "$.validations", "duplicate", "must not contain duplicate validation names");
  }

  const review = objectAt(record.review, "$.review", METRIC_KEYS.review, errors);
  if (review) {
    for (const key of ["sessions", "turns", "toolCalls"]) nullableNonNegativeInteger(review[key], `$.review.${key}`, errors);
    if (typeof review.externalReviewRequired !== "boolean") error(errors, "$.review.externalReviewRequired", "type", "must be boolean");
  }

  if (record.tokens !== null) {
    const tokens = objectAt(record.tokens, "$.tokens", METRIC_KEYS.tokens, errors);
    if (tokens) for (const key of METRIC_KEYS.tokens) nonNegativeInteger(tokens[key], `$.tokens.${key}`, errors);
  }

  const attempts = objectAt(record.attempts, "$.attempts", METRIC_KEYS.attempts, errors);
  if (attempts) for (const key of METRIC_KEYS.attempts) nonNegativeInteger(attempts[key], `$.attempts.${key}`, errors);

  const ci = objectAt(record.ci, "$.ci", METRIC_KEYS.ci, errors);
  if (ci) {
    for (const key of ["wallTimeMs", "requiredChecks", "failedChecks", "canceledRuns"]) nonNegativeInteger(ci[key], `$.ci.${key}`, errors);
    if (Number.isSafeInteger(ci.failedChecks) && Number.isSafeInteger(ci.requiredChecks) && ci.failedChecks > ci.requiredChecks) {
      error(errors, "$.ci.failedChecks", "consistency", "must not exceed requiredChecks");
    }
    if (!Array.isArray(ci.checks)) {
      error(errors, "$.ci.checks", "type", "must be an array");
    } else {
      if (ci.checks.length > MAX_METRIC_ENTRIES) error(errors, "$.ci.checks", "size", `must contain at most ${MAX_METRIC_ENTRIES} values`);
      const names = [];
      ci.checks.forEach((entry, index) => {
        const check = objectAt(entry, `$.ci.checks[${index}]`, METRIC_KEYS.ciCheck, errors);
        if (!check) return;
        if (typeof check.name !== "string" || !SAFE_NAME.test(check.name)) {
          error(errors, `$.ci.checks[${index}].name`, "format", "must be a bounded lowercase metric identifier");
        } else names.push(check.name);
        nonNegativeInteger(check.queueMs, `$.ci.checks[${index}].queueMs`, errors);
        nonNegativeInteger(check.durationMs, `$.ci.checks[${index}].durationMs`, errors);
        if (!["passed", "failed", "canceled"].includes(check.outcome)) {
          error(errors, `$.ci.checks[${index}].outcome`, "enum", "must be passed, failed, or canceled");
        }
      });
      if (new Set(names).size !== names.length) error(errors, "$.ci.checks", "duplicate", "must not contain duplicate check names");
      if (Number.isSafeInteger(ci.requiredChecks) && ci.requiredChecks !== ci.checks.length) {
        error(errors, "$.ci.requiredChecks", "consistency", "must equal the number of recorded required checks");
      }
      const failed = ci.checks.filter((check) => check?.outcome === "failed").length;
      if (Number.isSafeInteger(ci.failedChecks) && ci.failedChecks !== failed) {
        error(errors, "$.ci.failedChecks", "consistency", "must equal the number of failed required checks");
      }
    }
  }

  const worktree = objectAt(record.worktree, "$.worktree", METRIC_KEYS.worktree, errors);
  if (worktree) {
    nonNegativeInteger(worktree.peakTargetBytes, "$.worktree.peakTargetBytes", errors);
    nonNegativeInteger(worktree.peakWorktreeBytes, "$.worktree.peakWorktreeBytes", errors);
    if (Number.isSafeInteger(worktree.peakTargetBytes) && Number.isSafeInteger(worktree.peakWorktreeBytes)
      && worktree.peakTargetBytes > worktree.peakWorktreeBytes) {
      error(errors, "$.worktree.peakTargetBytes", "consistency", "must not exceed peakWorktreeBytes");
    }
  }

  const routing = objectAt(record.routing, "$.routing", METRIC_KEYS.routing, errors);
  if (routing) {
    if (!["feature", "integration", "release"].includes(routing.profile)) error(errors, "$.routing.profile", "enum", "must be feature, integration, or release");
    safeStringList(routing.areas, "$.routing.areas", errors);
    safeStringList(routing.targets, "$.routing.targets", errors, { nonEmpty: true });
    if (Array.isArray(routing.targets)) {
      routing.targets.forEach((target, index) => {
        if (typeof target === "string" && SAFE_NAME.test(target) && !HOSTED_TARGETS.includes(target)) {
          error(errors, `$.routing.targets[${index}]`, "enum", "must be a known hosted CI target");
        }
      });
    }
  }

  if (startedAt !== null && completedAt !== null && completedAt < startedAt) error(errors, "$.completedAt", "chronology", "must not precede startedAt");
  if (completedAt !== null && recordedAt !== null && recordedAt < completedAt) error(errors, "$.recordedAt", "chronology", "must not precede completedAt");
  if (recordedAt !== null && recordedAt > nowMs + MAX_FUTURE_SKEW_MS) error(errors, "$.recordedAt", "chronology", "must not be materially in the future");
  if (startedAt !== null && completedAt !== null && phases && Number.isSafeInteger(phases.totalElapsedMs) && phases.totalElapsedMs !== completedAt - startedAt) {
    error(errors, "$.phases.totalElapsedMs", "consistency", "must equal completedAt minus startedAt");
  }
  if (phases && ci && Number.isSafeInteger(phases.ciMs) && Number.isSafeInteger(ci.wallTimeMs) && phases.ciMs !== ci.wallTimeMs) {
    error(errors, "$.phases.ciMs", "consistency", "must equal ci.wallTimeMs");
  }
  if (phases && Number.isSafeInteger(phases.totalElapsedMs)) {
    for (const key of METRIC_KEYS.phase.filter((name) => name !== "totalElapsedMs")) {
      if (Number.isSafeInteger(phases[key]) && phases[key] > phases.totalElapsedMs) {
        error(errors, `$.phases.${key}`, "consistency", "must not exceed totalElapsedMs");
      }
    }
  }
  inspectSecretValues(record, "$", errors);
  return { ok: errors.length === 0, errors, ...(errors.length === 0 ? { record } : {}) };
}

function totalTokens(record) {
  if (record.tokens === null) return null;
  return Object.values(record.tokens).reduce((sum, value) => sum + value, 0);
}

function median(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function percentile90(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(sorted.length * 0.9) - 1)];
}

function distribution(values) {
  return { median: median(values), p90: percentile90(values) };
}

function safeSum(values) {
  let sum = 0;
  for (const value of values) {
    sum += value;
    if (!Number.isSafeInteger(sum)) return null;
  }
  return sum;
}

function targetCheckOverBudget(record) {
  const selected = new Set(record.routing.targets);
  return record.ci.checks.some((check) => selected.has(check.name)
    && CI_TARGET_BUDGET_MS[check.name] !== undefined
    && check.durationMs > CI_TARGET_BUDGET_MS[check.name]);
}

function metricId(record) {
  return `issue-${record.issue}/pr-${record.pr ?? "none"}/${record.headSha}`;
}

function recordIdentity(record) {
  return `issue-${record.issue}/${record.headSha}`;
}

function aggregateCiChecks(records) {
  const checks = new Map();
  for (const record of records) {
    for (const check of record.ci.checks) {
      const current = checks.get(check.name) ?? { queueMs: [], durationMs: [], outcomes: { passed: 0, failed: 0, canceled: 0 } };
      current.queueMs.push(check.queueMs);
      current.durationMs.push(check.durationMs);
      current.outcomes[check.outcome] += 1;
      checks.set(check.name, current);
    }
  }
  return Object.fromEntries([...checks.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([name, values]) => [name, {
    records: values.durationMs.length,
    queueMs: distribution(values.queueMs),
    durationMs: distribution(values.durationMs),
    outcomes: values.outcomes,
  }]));
}

function aggregateValidations(records) {
  const validations = new Map();
  for (const record of records) {
    for (const validation of record.validations) {
      const current = validations.get(validation.name) ?? { durationMs: [], outcomes: { passed: 0, failed: 0, canceled: 0 } };
      current.durationMs.push(validation.durationMs);
      current.outcomes[validation.outcome] += 1;
      validations.set(validation.name, current);
    }
  }
  return Object.fromEntries([...validations.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([name, values]) => [name, {
    records: values.durationMs.length,
    durationMs: distribution(values.durationMs),
    outcomes: values.outcomes,
  }]));
}

export function aggregateMetricRecords(records, invalidRecords = [], { nowMs = Date.now() } = {}) {
  for (const [index, record] of records.entries()) {
    const validation = validateMetricRecord(record, { nowMs });
    if (!validation.ok) throw new Error(`aggregate record ${index} is invalid: ${validation.errors[0]?.path ?? "$"}`);
  }
  const values = (selector) => records.map(selector);
  const availableValues = (selector) => values(selector).filter((value) => value !== null);
  const violationIds = (predicate) => records.filter(predicate).map(metricId);
  const tokenValues = availableValues(totalTokens);
  const totalTokenSum = tokenValues.length === 0 ? null : safeSum(tokenValues);
  const canceledAttemptSum = safeSum(values((record) => record.attempts.canceled));
  const failedAttemptSum = safeSum(values((record) => record.attempts.failed));
  const canceledCiSum = safeSum(values((record) => record.ci.canceledRuns));
  const missingFields = {};
  for (const invalid of invalidRecords) {
    for (const problem of invalid.errors ?? []) {
      if (problem.code === "missing") missingFields[problem.path] = (missingFields[problem.path] ?? 0) + 1;
    }
  }
  return {
    schemaVersion: METRICS_SCHEMA_VERSION,
    records: { discovered: records.length + invalidRecords.length, valid: records.length, invalid: invalidRecords.length },
    missingFields,
    coverage: {
      tokens: {
        available: records.filter((record) => record.tokens !== null).length,
        unavailable: records.filter((record) => record.tokens === null).length,
      },
      review: Object.fromEntries(["sessions", "turns", "toolCalls"].map((key) => [key, {
        available: records.filter((record) => record.review[key] !== null).length,
        unavailable: records.filter((record) => record.review[key] === null).length,
      }])),
    },
    distributions: {
      totalTokens: distribution(availableValues(totalTokens)),
      inputTokens: distribution(records.filter((record) => record.tokens !== null).map((record) => record.tokens.input)),
      outputTokens: distribution(records.filter((record) => record.tokens !== null).map((record) => record.tokens.output)),
      cacheReadTokens: distribution(records.filter((record) => record.tokens !== null).map((record) => record.tokens.cacheRead)),
      cacheWriteTokens: distribution(records.filter((record) => record.tokens !== null).map((record) => record.tokens.cacheWrite)),
      totalElapsedMs: distribution(values((record) => record.phases.totalElapsedMs)),
      developmentMs: distribution(values((record) => record.phases.developmentMs)),
      reviewMs: distribution(values((record) => record.phases.reviewMs)),
      validationMs: distribution(values((record) => record.phases.validationMs)),
      ciWallTimeMs: distribution(values((record) => record.ci.wallTimeMs)),
      reviewSessions: distribution(availableValues((record) => record.review.sessions)),
      reviewTurns: distribution(availableValues((record) => record.review.turns)),
      toolCalls: distribution(availableValues((record) => record.review.toolCalls)),
      pushesAfterFirstCi: distribution(values((record) => record.attempts.pushesAfterFirstCi)),
      peakTargetBytes: distribution(values((record) => record.worktree.peakTargetBytes)),
      peakWorktreeBytes: distribution(values((record) => record.worktree.peakWorktreeBytes)),
    },
    validations: aggregateValidations(records),
    ciChecks: aggregateCiChecks(records),
    totals: {
      tokens: totalTokenSum,
      canceledAttempts: canceledAttemptSum,
      failedAttempts: failedAttemptSum,
      canceledCiRuns: canceledCiSum,
      externalReviewsRequired: records.filter((record) => record.review.externalReviewRequired).length,
    },
    overflowedTotals: [
      ...(tokenValues.length > 0 && totalTokenSum === null ? ["tokens"] : []),
      ...(canceledAttemptSum === null ? ["canceledAttempts"] : []),
      ...(failedAttemptSum === null ? ["failedAttempts"] : []),
      ...(canceledCiSum === null ? ["canceledCiRuns"] : []),
    ],
    sloViolations: {
      routineOver60Minutes: violationIds((record) => record.routing.profile === "feature" && record.phases.totalElapsedMs > 60 * 60_000),
      ciTargetOverBudget: violationIds(targetCheckOverBudget),
      reviewSessionsOver4: violationIds((record) => Number.isSafeInteger(record.review.sessions) && record.review.sessions > 4),
      pushesAfterFirstCi: violationIds((record) => record.attempts.pushesAfterFirstCi > 0),
      failedOrCanceledAttempts: violationIds((record) => record.attempts.failed > 0 || record.attempts.canceled > 0 || record.ci.canceledRuns > 0),
      targetOver10GiB: violationIds((record) => record.worktree.peakTargetBytes > 10 * 1024 ** 3),
      retentionOver90Days: violationIds((record) => nowMs - Date.parse(record.recordedAt) > RETENTION_MS),
    },
  };
}

async function defaultMetricsDirectory(cwd) {
  const common = execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, encoding: "utf8" }).trim();
  return path.join(common, "oxid-factory", "metrics-v1");
}

async function regularJsonFiles(inputDir) {
  let entries;
  try {
    entries = await readdir(inputDir, { withFileTypes: true });
  } catch (cause) {
    if (cause?.code === "ENOENT") return [];
    throw cause;
  }
  const jsonEntries = entries.filter((entry) => entry.name.endsWith(".json")).sort((left, right) => left.name.localeCompare(right.name));
  if (jsonEntries.length > MAX_RECORD_FILES) throw new Error(`metrics store exceeds the ${MAX_RECORD_FILES}-record audit bound`);
  return jsonEntries;
}

function privateFileLabel(filename) {
  if (SAFE_NAME.test(filename.replace(/\.json$/, ""))) return filename;
  const digest = createHash("sha256").update(filename).digest("hex").slice(0, 12);
  return `unsafe-${digest}.json`;
}

async function readBoundedRegularFile(file) {
  let handle;
  try {
    handle = await open(file, fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW | fsConstants.O_NONBLOCK);
    const info = await handle.stat();
    if (!info.isFile()) throw Object.assign(new Error("record must be a regular file"), { metricCode: "type" });
    const buffer = Buffer.alloc(MAX_RECORD_BYTES + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const { bytesRead } = await handle.read(buffer, offset, buffer.length - offset, offset);
      if (bytesRead === 0) break;
      offset += bytesRead;
    }
    if (offset > MAX_RECORD_BYTES) throw Object.assign(new Error(`record exceeds ${MAX_RECORD_BYTES} bytes`), { metricCode: "size" });
    return buffer.subarray(0, offset).toString("utf8");
  } catch (cause) {
    if (cause?.code === "ELOOP") throw Object.assign(new Error("record must be a regular file"), { metricCode: "type", cause });
    throw cause;
  } finally {
    await handle?.close().catch(() => {});
  }
}

export async function auditMetricsDirectory(inputDir) {
  const candidates = [];
  const records = [];
  const invalidRecords = [];
  for (const entry of await regularJsonFiles(inputDir)) {
    const file = path.join(inputDir, entry.name);
    const publicFile = privateFileLabel(entry.name);
    try {
      const parsed = JSON.parse(await readBoundedRegularFile(file));
      const result = validateMetricRecord(parsed);
      if (result.ok) candidates.push({ file: publicFile, record: result.record });
      else invalidRecords.push({ file: publicFile, errors: result.errors });
    } catch (cause) {
      const code = cause?.metricCode ?? (cause instanceof SyntaxError ? "json" : "io");
      const message = cause?.metricCode ? cause.message : (code === "json" ? "record is not valid JSON" : "record could not be read");
      invalidRecords.push({ file: publicFile, errors: [{ path: "$", code, message }] });
    }
  }
  const counts = new Map();
  for (const candidate of candidates) counts.set(recordIdentity(candidate.record), (counts.get(recordIdentity(candidate.record)) ?? 0) + 1);
  for (const candidate of candidates) {
    if (counts.get(recordIdentity(candidate.record)) === 1) records.push(candidate.record);
    else invalidRecords.push({
      file: candidate.file,
      errors: [{ path: "$", code: "duplicate", message: "issue/head identity must appear exactly once; PR metadata does not create a second work item" }],
    });
  }
  return { ok: invalidRecords.length === 0, ...aggregateMetricRecords(records, invalidRecords), invalidRecords };
}

export async function writeMetricRecord(record, { outputDir, currentHead, replace = false } = {}) {
  const result = validateMetricRecord(record);
  if (!result.ok) throw new Error(`metric record rejected: ${result.errors.map((problem) => `${problem.path} ${problem.message}`).join("; ")}`);
  if (record.headSha !== currentHead) throw new Error(`metric record head ${record.headSha} does not match current checkout ${currentHead}`);
  await mkdir(outputDir, { recursive: true, mode: 0o700 });
  const directoryInfo = await lstat(outputDir);
  if (!directoryInfo.isDirectory() || directoryInfo.isSymbolicLink()) throw new Error("metrics output must be a real directory");
  if ((directoryInfo.mode & 0o077) !== 0) throw new Error("metrics output directory must not grant group or other access");
  if (typeof process.getuid === "function" && directoryInfo.uid !== process.getuid()) throw new Error("metrics output directory must be owned by the invoking user");
  const resolvedDir = await realpath(outputDir);
  const filename = `issue-${record.issue}-${record.headSha}.json`;
  const destination = path.join(resolvedDir, filename);
  if (!replace) {
    try {
      await lstat(destination);
      throw new Error(`metric record already exists: ${filename}; pass --replace only to correct the same issue/PR/head record`);
    } catch (cause) {
      if (cause?.code !== "ENOENT") throw cause;
    }
  }
  const temporary = path.join(resolvedDir, `.${filename}.${process.pid}.${randomUUID()}.tmp`);
  let handle;
  try {
    handle = await open(temporary, "wx", 0o600);
    await handle.writeFile(`${JSON.stringify(record, null, 2)}\n`, "utf8");
    await handle.sync();
    await handle.close();
    handle = null;
    if (replace) await rename(temporary, destination);
    else {
      await link(temporary, destination);
      await unlink(temporary);
    }
  } catch (cause) {
    await handle?.close().catch(() => {});
    await unlink(temporary).catch(() => {});
    throw cause;
  }
  return destination;
}

export function metricTemplate({ issue, pr = null, headSha, now = new Date().toISOString(), draft = true }) {
  const measured = draft ? null : 0;
  return {
    schemaVersion: METRICS_SCHEMA_VERSION,
    repository: METRICS_REPOSITORY,
    issue,
    pr,
    headSha,
    startedAt: draft ? null : now,
    completedAt: draft ? null : now,
    recordedAt: draft ? null : now,
    phases: { developmentMs: measured, reviewMs: measured, validationMs: measured, ciMs: measured, totalElapsedMs: measured },
    validations: [],
    review: { sessions: null, turns: null, toolCalls: null, externalReviewRequired: draft ? null : false },
    tokens: null,
    attempts: { pushesAfterFirstCi: measured, canceled: measured, failed: measured },
    ci: { wallTimeMs: measured, requiredChecks: measured, failedChecks: measured, canceledRuns: measured, checks: [] },
    worktree: { peakTargetBytes: measured, peakWorktreeBytes: measured },
    routing: { profile: "feature", areas: [], targets: draft ? [] : ["basic"] },
  };
}

const USAGE = `Usage:
  node scripts/factory/metrics.mjs template --issue N [--pr N] --head SHA
  node scripts/factory/metrics.mjs write --record FILE [--output-dir DIR] [--replace]
  node scripts/factory/metrics.mjs audit [--input-dir DIR] [--json]

The default private store is <git-common-dir>/oxid-factory/metrics-v1. Audit is
read-only and never invokes a model, retries work, mutates GitHub, or cleans disk.`;

function cliOptions(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      issue: { type: "string" }, pr: { type: "string" }, head: { type: "string" }, record: { type: "string" },
      "output-dir": { type: "string" }, "input-dir": { type: "string" }, replace: { type: "boolean" },
      json: { type: "boolean" }, help: { type: "boolean", short: "h" },
    },
    allowPositionals: true,
    strict: true,
  });
  if (positionals.length > 1) throw new Error(`unexpected positional argument: ${positionals[1]}`);
  return { command: positionals[0], values };
}

function rejectUnusedOptions(values, allowed) {
  for (const key of Object.keys(values)) {
    if (!allowed.includes(key)) throw new Error(`--${key} is not valid for this command`);
  }
}

function pathIsWithin(parent, candidate) {
  const relative = path.relative(parent, candidate);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

function repositoryPrivatePaths(cwd) {
  const root = realpathSync(execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd, encoding: "utf8" }).trim());
  const common = execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, encoding: "utf8" }).trim();
  return { root, common: realpathSync(common) };
}

function canonicalProspectivePath(candidate) {
  let cursor = path.dirname(candidate);
  const suffix = [path.basename(candidate)];
  while (true) {
    try {
      return path.resolve(realpathSync(cursor), ...suffix.reverse());
    } catch (cause) {
      if (cause?.code !== "ENOENT") throw cause;
      const parent = path.dirname(cursor);
      if (parent === cursor) throw cause;
      suffix.push(path.basename(cursor));
      cursor = parent;
    }
  }
}

function requireOutsideWorktree(candidate, label, { root, common }) {
  if (pathIsWithin(root, candidate) && !pathIsWithin(common, candidate)) {
    throw new Error(`${label} must remain outside the worktree (the common Git metrics store is allowed)`);
  }
}

export async function runCli(argv = process.argv.slice(2), { cwd = process.cwd(), stdout = process.stdout } = {}) {
  const { command, values } = cliOptions(argv);
  if (values.help || !command) {
    stdout.write(`${USAGE}\n`);
    return 0;
  }
  if (command === "template") {
    rejectUnusedOptions(values, ["issue", "pr", "head"]);
    const issue = Number(values.issue);
    const pr = values.pr === undefined ? null : Number(values.pr);
    if (!Number.isInteger(issue) || issue < 1 || (pr !== null && (!Number.isInteger(pr) || pr < 1))) throw new Error("template requires positive --issue and optional --pr values");
    if (!/^[0-9a-f]{40}$/.test(values.head ?? "")) throw new Error("template requires an exact lowercase --head SHA");
    stdout.write(`${JSON.stringify(metricTemplate({ issue, pr, headSha: values.head }), null, 2)}\n`);
    return 0;
  }
  if (command === "write") {
    rejectUnusedOptions(values, ["record", "output-dir", "replace"]);
    if (!values.record) throw new Error("write requires --record FILE");
    const recordPath = path.resolve(cwd, values.record);
    const privatePaths = repositoryPrivatePaths(cwd);
    requireOutsideWorktree(canonicalProspectivePath(recordPath), "--record", privatePaths);
    const record = JSON.parse(await readBoundedRegularFile(recordPath));
    const currentHead = execFileSync("git", ["rev-parse", "HEAD"], { cwd, encoding: "utf8" }).trim();
    const outputDir = values["output-dir"] ? path.resolve(cwd, values["output-dir"]) : await defaultMetricsDirectory(cwd);
    requireOutsideWorktree(canonicalProspectivePath(outputDir), "--output-dir", privatePaths);
    const worktreeState = execFileSync("git", ["status", "--porcelain=v1"], { cwd, encoding: "utf8" }).trim();
    if (worktreeState) throw new Error("write requires a clean checkout so the record is unambiguously bound to HEAD");
    const destination = await writeMetricRecord(record, { outputDir, currentHead, replace: values.replace === true });
    stdout.write(`${JSON.stringify({ ok: true, file: path.basename(destination) })}\n`);
    return 0;
  }
  if (command === "audit") {
    rejectUnusedOptions(values, ["input-dir", "json"]);
    const inputDir = values["input-dir"] ? path.resolve(cwd, values["input-dir"]) : await defaultMetricsDirectory(cwd);
    const result = await auditMetricsDirectory(inputDir);
    if (values.json) stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    else stdout.write(`records=${result.records.valid}/${result.records.discovered} invalid=${result.records.invalid} elapsed-p50-ms=${result.distributions.totalElapsedMs.median ?? "n/a"} elapsed-p90-ms=${result.distributions.totalElapsedMs.p90 ?? "n/a"}\n`);
    return result.ok ? 0 : 1;
  }
  throw new Error(`unknown command: ${command}`);
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  runCli().then((code) => { process.exitCode = code; }).catch((error) => {
    process.stderr.write(`[factory-metrics] ${error.message}\n`);
    process.exitCode = 1;
  });
}
