#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { link, open, lstat, mkdir, readFile, readdir, realpath, rename, unlink } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

export const METRICS_SCHEMA_VERSION = 1;
export const METRICS_REPOSITORY = "MediaNoxLabs/oxid";
const MAX_SAFE_COUNT = 1_000_000_000_000_000;
const SECRET_VALUE = /(?:github_pat_|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|\bBearer\s+\S+|\b(?:password|secret|token)\s*[=:]\s*\S+|openid-credential-offer|\bdid:)/i;
const SAFE_NAME = /^[a-z0-9][a-z0-9._:-]{0,63}$/;
const TOP_LEVEL_KEYS = [
  "schemaVersion", "repository", "issue", "pr", "headSha", "startedAt", "completedAt", "recordedAt",
  "phases", "validations", "review", "tokens", "attempts", "ci", "worktree", "routing",
];

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
  for (const key of actual) if (!keys.includes(key)) error(errors, `${pathName}.${key}`, "unknown", "is not allowed");
  return value;
}

function nonNegativeInteger(value, pathName, errors) {
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_SAFE_COUNT) {
    error(errors, pathName, "range", "must be a finite non-negative safe integer");
    return false;
  }
  return true;
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

function inspectSecretValues(value, pathName, errors) {
  if (typeof value === "string" && SECRET_VALUE.test(value)) {
    error(errors, pathName, "secret", "must not contain credentials, identifiers, prompts, or secret-bearing values");
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => inspectSecretValues(entry, `${pathName}[${index}]`, errors));
    return;
  }
  if (value && typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) inspectSecretValues(entry, `${pathName}.${key}`, errors);
  }
}

function safeStringList(value, pathName, errors, { nonEmpty = false } = {}) {
  if (!Array.isArray(value)) {
    error(errors, pathName, "type", "must be an array");
    return;
  }
  if (nonEmpty && value.length === 0) error(errors, pathName, "empty", "must contain at least one value");
  if (new Set(value).size !== value.length) error(errors, pathName, "duplicate", "must not contain duplicates");
  value.forEach((entry, index) => {
    if (typeof entry !== "string" || !SAFE_NAME.test(entry)) {
      error(errors, `${pathName}[${index}]`, "format", "must be a bounded lowercase metric identifier");
    }
  });
}

export function validateMetricRecord(candidate) {
  const errors = [];
  const record = objectAt(candidate, "$", TOP_LEVEL_KEYS, errors);
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

  const phases = objectAt(record.phases, "$.phases", ["developmentMs", "reviewMs", "validationMs", "ciMs", "totalElapsedMs"], errors);
  if (phases) for (const key of ["developmentMs", "reviewMs", "validationMs", "ciMs", "totalElapsedMs"]) nonNegativeInteger(phases[key], `$.phases.${key}`, errors);

  if (!Array.isArray(record.validations)) {
    error(errors, "$.validations", "type", "must be an array");
  } else {
    record.validations.forEach((entry, index) => {
      const validation = objectAt(entry, `$.validations[${index}]`, ["name", "durationMs", "outcome"], errors);
      if (!validation) return;
      if (typeof validation.name !== "string" || !SAFE_NAME.test(validation.name)) {
        error(errors, `$.validations[${index}].name`, "format", "must be a bounded lowercase metric identifier, not a raw command");
      }
      nonNegativeInteger(validation.durationMs, `$.validations[${index}].durationMs`, errors);
      if (!["passed", "failed", "canceled"].includes(validation.outcome)) {
        error(errors, `$.validations[${index}].outcome`, "enum", "must be passed, failed, or canceled");
      }
    });
  }

  const review = objectAt(record.review, "$.review", ["sessions", "turns", "toolCalls", "externalReviewRequired"], errors);
  if (review) {
    for (const key of ["sessions", "turns", "toolCalls"]) nonNegativeInteger(review[key], `$.review.${key}`, errors);
    if (typeof review.externalReviewRequired !== "boolean") error(errors, "$.review.externalReviewRequired", "type", "must be boolean");
  }

  if (record.tokens !== null) {
    const tokens = objectAt(record.tokens, "$.tokens", ["input", "output", "cacheRead", "cacheWrite"], errors);
    if (tokens) for (const key of ["input", "output", "cacheRead", "cacheWrite"]) nonNegativeInteger(tokens[key], `$.tokens.${key}`, errors);
  }

  const attempts = objectAt(record.attempts, "$.attempts", ["pushesAfterFirstCi", "canceled", "failed"], errors);
  if (attempts) for (const key of ["pushesAfterFirstCi", "canceled", "failed"]) nonNegativeInteger(attempts[key], `$.attempts.${key}`, errors);

  const ci = objectAt(record.ci, "$.ci", ["wallTimeMs", "requiredChecks", "failedChecks", "canceledRuns"], errors);
  if (ci) {
    for (const key of ["wallTimeMs", "requiredChecks", "failedChecks", "canceledRuns"]) nonNegativeInteger(ci[key], `$.ci.${key}`, errors);
    if (Number.isSafeInteger(ci.failedChecks) && Number.isSafeInteger(ci.requiredChecks) && ci.failedChecks > ci.requiredChecks) {
      error(errors, "$.ci.failedChecks", "consistency", "must not exceed requiredChecks");
    }
  }

  const worktree = objectAt(record.worktree, "$.worktree", ["peakTargetBytes"], errors);
  if (worktree) nonNegativeInteger(worktree.peakTargetBytes, "$.worktree.peakTargetBytes", errors);

  const routing = objectAt(record.routing, "$.routing", ["profile", "areas", "targets"], errors);
  if (routing) {
    if (!["feature", "integration", "release"].includes(routing.profile)) error(errors, "$.routing.profile", "enum", "must be feature, integration, or release");
    safeStringList(routing.areas, "$.routing.areas", errors);
    safeStringList(routing.targets, "$.routing.targets", errors, { nonEmpty: true });
  }

  if (startedAt !== null && completedAt !== null && completedAt < startedAt) error(errors, "$.completedAt", "chronology", "must not precede startedAt");
  if (completedAt !== null && recordedAt !== null && recordedAt < completedAt) error(errors, "$.recordedAt", "chronology", "must not precede completedAt");
  if (startedAt !== null && completedAt !== null && phases && Number.isSafeInteger(phases.totalElapsedMs) && phases.totalElapsedMs !== completedAt - startedAt) {
    error(errors, "$.phases.totalElapsedMs", "consistency", "must equal completedAt minus startedAt");
  }
  if (phases && ci && Number.isSafeInteger(phases.ciMs) && Number.isSafeInteger(ci.wallTimeMs) && phases.ciMs !== ci.wallTimeMs) {
    error(errors, "$.phases.ciMs", "consistency", "must equal ci.wallTimeMs");
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

function ciBudgetMs(record) {
  const targets = new Set(record.routing.targets);
  if (["coverage-linux", "quality", "nix-package", "compact-artifacts", "ui-release-linux"].some((target) => targets.has(target))) return 25 * 60_000;
  if (targets.has("ui-linux")) return 15 * 60_000;
  if (targets.has("unit-linux") || targets.has("headless-linux")) return 10 * 60_000;
  return 5 * 60_000;
}

function metricId(record) {
  return `issue-${record.issue}/pr-${record.pr ?? "none"}/${record.headSha}`;
}

export function aggregateMetricRecords(records, invalidRecords = []) {
  const values = (selector) => records.map(selector);
  const availableValues = (selector) => values(selector).filter((value) => value !== null);
  const violationIds = (predicate) => records.filter(predicate).map(metricId);
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
    },
    distributions: {
      totalTokens: distribution(availableValues(totalTokens)),
      totalElapsedMs: distribution(values((record) => record.phases.totalElapsedMs)),
      developmentMs: distribution(values((record) => record.phases.developmentMs)),
      reviewMs: distribution(values((record) => record.phases.reviewMs)),
      validationMs: distribution(values((record) => record.phases.validationMs)),
      ciWallTimeMs: distribution(values((record) => record.ci.wallTimeMs)),
      reviewSessions: distribution(values((record) => record.review.sessions)),
      toolCalls: distribution(values((record) => record.review.toolCalls)),
      pushesAfterFirstCi: distribution(values((record) => record.attempts.pushesAfterFirstCi)),
      peakTargetBytes: distribution(values((record) => record.worktree.peakTargetBytes)),
    },
    totals: {
      tokens: availableValues(totalTokens).reduce((sum, value) => sum + value, 0),
      canceledAttempts: values((record) => record.attempts.canceled).reduce((sum, value) => sum + value, 0),
      failedAttempts: values((record) => record.attempts.failed).reduce((sum, value) => sum + value, 0),
      canceledCiRuns: values((record) => record.ci.canceledRuns).reduce((sum, value) => sum + value, 0),
    },
    sloViolations: {
      routineOver60Minutes: violationIds((record) => record.routing.profile === "feature" && record.phases.totalElapsedMs > 60 * 60_000),
      ciOverTargetBudget: violationIds((record) => record.ci.wallTimeMs > ciBudgetMs(record)),
      reviewSessionsOver4: violationIds((record) => record.review.sessions > 4),
      pushesAfterFirstCi: violationIds((record) => record.attempts.pushesAfterFirstCi > 0),
      failedOrCanceledAttempts: violationIds((record) => record.attempts.failed > 0 || record.attempts.canceled > 0 || record.ci.canceledRuns > 0),
      targetOver10GiB: violationIds((record) => record.worktree.peakTargetBytes > 10 * 1024 ** 3),
    },
  };
}

async function defaultMetricsDirectory(cwd) {
  const common = execFileSync("git", ["rev-parse", "--git-common-dir"], { cwd, encoding: "utf8" }).trim();
  return path.join(path.resolve(cwd, common), "oxid-factory", "metrics-v1");
}

async function regularJsonFiles(inputDir) {
  let entries;
  try {
    entries = await readdir(inputDir, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
  return entries.filter((entry) => entry.name.endsWith(".json")).sort((left, right) => left.name.localeCompare(right.name));
}

export async function auditMetricsDirectory(inputDir) {
  const records = [];
  const invalidRecords = [];
  for (const entry of await regularJsonFiles(inputDir)) {
    const file = path.join(inputDir, entry.name);
    const publicFile = SAFE_NAME.test(entry.name.replace(/\.json$/, "")) ? entry.name : "unsafe-filename.json";
    if (!entry.isFile() || entry.isSymbolicLink()) {
      invalidRecords.push({ file: publicFile, errors: [{ path: "$", code: "type", message: "record must be a regular file" }] });
      continue;
    }
    try {
      const parsed = JSON.parse(await readFile(file, "utf8"));
      const result = validateMetricRecord(parsed);
      if (result.ok) records.push(result.record);
      else invalidRecords.push({ file: publicFile, errors: result.errors });
    } catch (error) {
      invalidRecords.push({ file: publicFile, errors: [{ path: "$", code: "json", message: "record is not valid JSON" }] });
    }
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
  const resolvedDir = await realpath(outputDir);
  const filename = `issue-${record.issue}-pr-${record.pr ?? "none"}-${record.headSha}.json`;
  const destination = path.join(resolvedDir, filename);
  if (!replace) {
    try {
      await lstat(destination);
      throw new Error(`metric record already exists: ${filename}; pass --replace only to correct the same issue/PR/head record`);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
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
  } catch (error) {
    await handle?.close().catch(() => {});
    await unlink(temporary).catch(() => {});
    throw error;
  }
  return destination;
}

export function metricTemplate({ issue, pr = null, headSha, now = new Date().toISOString() }) {
  return {
    schemaVersion: METRICS_SCHEMA_VERSION,
    repository: METRICS_REPOSITORY,
    issue,
    pr,
    headSha,
    startedAt: now,
    completedAt: now,
    recordedAt: now,
    phases: { developmentMs: 0, reviewMs: 0, validationMs: 0, ciMs: 0, totalElapsedMs: 0 },
    validations: [],
    review: { sessions: 0, turns: 0, toolCalls: 0, externalReviewRequired: false },
    tokens: null,
    attempts: { pushesAfterFirstCi: 0, canceled: 0, failed: 0 },
    ci: { wallTimeMs: 0, requiredChecks: 0, failedChecks: 0, canceledRuns: 0 },
    worktree: { peakTargetBytes: 0 },
    routing: { profile: "feature", areas: [], targets: ["basic"] },
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

export async function runCli(argv = process.argv.slice(2), { cwd = process.cwd(), stdout = process.stdout } = {}) {
  const { command, values } = cliOptions(argv);
  if (values.help || !command) {
    stdout.write(`${USAGE}\n`);
    return 0;
  }
  if (command === "template") {
    const issue = Number(values.issue);
    const pr = values.pr === undefined ? null : Number(values.pr);
    if (!Number.isInteger(issue) || issue < 1 || (pr !== null && (!Number.isInteger(pr) || pr < 1))) throw new Error("template requires positive --issue and optional --pr values");
    if (!/^[0-9a-f]{40}$/.test(values.head ?? "")) throw new Error("template requires an exact lowercase --head SHA");
    stdout.write(`${JSON.stringify(metricTemplate({ issue, pr, headSha: values.head }), null, 2)}\n`);
    return 0;
  }
  if (command === "write") {
    if (!values.record) throw new Error("write requires --record FILE");
    const info = await lstat(values.record);
    if (!info.isFile() || info.isSymbolicLink()) throw new Error("--record must be a regular file");
    const record = JSON.parse(await readFile(values.record, "utf8"));
    const currentHead = execFileSync("git", ["rev-parse", "HEAD"], { cwd, encoding: "utf8" }).trim();
    const worktreeState = execFileSync("git", ["status", "--porcelain=v1"], { cwd, encoding: "utf8" }).trim();
    if (worktreeState) throw new Error("write requires a clean checkout so the record is unambiguously bound to HEAD");
    const outputDir = values["output-dir"] ? path.resolve(values["output-dir"]) : await defaultMetricsDirectory(cwd);
    const destination = await writeMetricRecord(record, { outputDir, currentHead, replace: values.replace === true });
    stdout.write(`${JSON.stringify({ ok: true, file: path.basename(destination) })}\n`);
    return 0;
  }
  if (command === "audit") {
    const inputDir = values["input-dir"] ? path.resolve(values["input-dir"]) : await defaultMetricsDirectory(cwd);
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
