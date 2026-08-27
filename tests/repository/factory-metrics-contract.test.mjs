// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  aggregateMetricRecords,
  auditMetricsDirectory,
  metricTemplate,
  validateMetricRecord,
  writeMetricRecord,
} from "../../scripts/factory/metrics.mjs";

const HEAD = "a".repeat(40);

function record(overrides = {}) {
  const candidate = metricTemplate({ issue: 167, pr: 170, headSha: HEAD, now: "2026-08-28T00:00:00.000Z" });
  candidate.completedAt = "2026-08-28T00:30:00.000Z";
  candidate.recordedAt = "2026-08-28T00:31:00.000Z";
  candidate.phases = { developmentMs: 600_000, reviewMs: 300_000, validationMs: 300_000, ciMs: 300_000, totalElapsedMs: 1_800_000 };
  candidate.validations = [{ name: "repository-contract", durationMs: 300_000, outcome: "passed" }];
  candidate.review = { sessions: 2, turns: 8, toolCalls: 12, externalReviewRequired: false };
  candidate.tokens = { input: 100, output: 20, cacheRead: 10, cacheWrite: 0 };
  candidate.ci = { wallTimeMs: 300_000, requiredChecks: 8, failedChecks: 0, canceledRuns: 0 };
  return { ...candidate, ...overrides };
}

test("v1 rejects unknown, ambiguous, negative, revision, chronology, and secret-bearing data", () => {
  assert.equal(validateMetricRecord(record()).ok, true);
  assert.equal(validateMetricRecord(record({ tokens: null })).ok, true);
  for (const mutate of [
    (value) => { value.extra = "unknown"; },
    (value) => { value.tokens.input = -1; },
    (value) => { value.tokens.input = Number.POSITIVE_INFINITY; },
    (value) => { value.headSha = "abc"; },
    (value) => { value.completedAt = "2026-08-27T23:59:00.000Z"; },
    (value) => { value.routing.areas = ["did:example"]; },
    (value) => { value.routing.targets = []; },
    (value) => { delete value.review.turns; },
  ]) {
    const invalid = record();
    mutate(invalid);
    assert.equal(validateMetricRecord(invalid).ok, false);
  }
  const secret = record();
  secret.routing.areas = ["ghp_abcdefghijklmnopqrstuvwxyz"];
  assert.ok(validateMetricRecord(secret).errors.some((problem) => problem.code === "secret"));
});

test("records are written atomically with private mode and exact-head binding", async () => {
  const outputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-write-"));
  await assert.rejects(
    writeMetricRecord(record(), { outputDir, currentHead: "b".repeat(40) }),
    /does not match current checkout/,
  );
  const destination = await writeMetricRecord(record(), { outputDir, currentHead: HEAD });
  assert.equal((await stat(destination)).mode & 0o777, 0o600);
  assert.deepEqual(JSON.parse(await readFile(destination, "utf8")), record());
  await assert.rejects(writeMetricRecord(record(), { outputDir, currentHead: HEAD }), /already exists/);
  const entriesBefore = await readdir(outputDir);
  await writeMetricRecord(record(), { outputDir, currentHead: HEAD, replace: true });
  assert.deepEqual(await readdir(outputDir), entriesBefore);
});

test("aggregate reports medians, p90, and tuning SLO violations without raw records", () => {
  const slow = record({
    issue: 168,
    pr: 169,
    completedAt: "2026-08-28T01:30:00.000Z",
    recordedAt: "2026-08-28T01:31:00.000Z",
    phases: { developmentMs: 2_000_000, reviewMs: 1_000_000, validationMs: 600_000, ciMs: 360_001, totalElapsedMs: 5_400_000 },
    review: { sessions: 5, turns: 20, toolCalls: 30, externalReviewRequired: true },
    tokens: { input: 200, output: 40, cacheRead: 20, cacheWrite: 0 },
    attempts: { pushesAfterFirstCi: 1, canceled: 1, failed: 1 },
    ci: { wallTimeMs: 360_001, requiredChecks: 8, failedChecks: 1, canceledRuns: 1 },
    worktree: { peakTargetBytes: 11 * 1024 ** 3 },
  });
  const aggregate = aggregateMetricRecords([record(), slow]);
  assert.deepEqual(aggregate.records, { discovered: 2, valid: 2, invalid: 0 });
  assert.deepEqual(aggregate.distributions.totalElapsedMs, { median: 3_600_000, p90: 5_400_000 });
  assert.deepEqual(aggregate.distributions.totalTokens, { median: 195, p90: 260 });
  assert.equal(aggregate.totals.tokens, 390);
  assert.deepEqual(aggregate.coverage.tokens, { available: 2, unavailable: 0 });
  for (const key of ["routineOver60Minutes", "ciOverTargetBudget", "reviewSessionsOver4", "pushesAfterFirstCi", "failedOrCanceledAttempts", "targetOver10GiB"]) {
    assert.deepEqual(aggregate.sloViolations[key], [`issue-168/pr-169/${HEAD}`]);
  }
  assert.equal(JSON.stringify(aggregate).includes("validations"), false);
});

test("aggregate excludes unavailable token telemetry instead of treating it as zero", () => {
  const unavailable = record({ issue: 170, pr: null, tokens: null });
  const aggregate = aggregateMetricRecords([unavailable, record()]);
  assert.deepEqual(aggregate.coverage.tokens, { available: 1, unavailable: 1 });
  assert.deepEqual(aggregate.distributions.totalTokens, { median: 130, p90: 130 });
  assert.equal(aggregate.totals.tokens, 130);
});

test("read-only audit reports malformed records and required-field gaps", async () => {
  const inputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-audit-"));
  await writeFile(path.join(inputDir, "valid.json"), `${JSON.stringify(record())}\n`, { mode: 0o600 });
  const missing = record();
  delete missing.tokens.output;
  await writeFile(path.join(inputDir, "missing.json"), `${JSON.stringify(missing)}\n`, { mode: 0o600 });
  await writeFile(path.join(inputDir, "malformed.json"), "{not-json\n", { mode: 0o600 });
  const before = await stat(path.join(inputDir, "valid.json"));
  const result = await auditMetricsDirectory(inputDir);
  const after = await stat(path.join(inputDir, "valid.json"));
  assert.equal(result.ok, false);
  assert.deepEqual(result.records, { discovered: 3, valid: 1, invalid: 2 });
  assert.equal(result.missingFields["$.tokens.output"], 1);
  assert.equal(before.mtimeMs, after.mtimeMs);
});

test("committed schema advertises the same closed v1 contract", async () => {
  const schema = JSON.parse(await readFile(new URL("../../docs/factory/work-item-metrics-v1.schema.json", import.meta.url), "utf8"));
  assert.equal(schema.properties.schemaVersion.const, 1);
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(schema.properties.tokens.oneOf[0], { type: "null" });
  assert.deepEqual(new Set(schema.required), new Set([
    "schemaVersion", "repository", "issue", "pr", "headSha", "startedAt", "completedAt", "recordedAt",
    "phases", "validations", "review", "tokens", "attempts", "ci", "worktree", "routing",
  ]));
});

test("factory guidance defines the periodic secret-safe supervisor boundary", async () => {
  const [metrics, charter, loop, runbook] = await Promise.all([
    readFile(new URL("../../docs/factory/metrics.md", import.meta.url), "utf8"),
    readFile(new URL("../../docs/factory/charter.md", import.meta.url), "utf8"),
    readFile(new URL("../../docs/factory/productive-loop.md", import.meta.url), "utf8"),
    readFile(new URL("../../docs/factory/runbook.md", import.meta.url), "utf8"),
  ]);
  for (const source of [metrics, charter, loop, runbook]) assert.match(source, /weekly/i);
  assert.match(metrics, /median\/p90/);
  assert.match(metrics, /90 days/);
  assert.match(metrics, /no model call/i);
  assert.match(loop, /prompts, transcripts, credentials, identifiers/);
  assert.match(runbook, /owner-private/);
});
