// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, readdir, stat, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  CI_TARGET_BUDGET_MS,
  METRIC_KEYS,
  aggregateMetricRecords,
  auditMetricsDirectory,
  metricTemplate,
  runCli,
  validateMetricRecord,
  writeMetricRecord,
} from "../../scripts/factory/metrics.mjs";
import { HOSTED_TARGETS } from "../../scripts/ci/target-plan.mjs";

const HEAD = "a".repeat(40);

function record(overrides = {}) {
  const candidate = metricTemplate({ issue: 167, pr: 170, headSha: HEAD, now: "2026-08-28T00:00:00.000Z", draft: false });
  candidate.completedAt = "2026-08-28T00:30:00.000Z";
  candidate.recordedAt = "2026-08-28T00:31:00.000Z";
  candidate.phases = { developmentMs: 600_000, reviewMs: 300_000, validationMs: 300_000, ciMs: 300_000, totalElapsedMs: 1_800_000 };
  candidate.validations = [{ name: "repository-contract", durationMs: 300_000, outcome: "passed" }];
  candidate.review = { sessions: 2, turns: 8, toolCalls: 12, externalReviewRequired: false };
  candidate.tokens = { input: 100, output: 20, cacheRead: 10, cacheWrite: 0 };
  candidate.ci = {
    wallTimeMs: 300_000,
    requiredChecks: 2,
    failedChecks: 0,
    canceledRuns: 0,
    checks: [
      { name: "basic", queueMs: 1_000, durationMs: 90_000, outcome: "passed" },
      { name: "quality", queueMs: 2_000, durationMs: 298_000, outcome: "passed" },
    ],
  };
  candidate.worktree = { peakTargetBytes: 1024, peakWorktreeBytes: 2048 };
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
    (value) => { value.recordedAt = "9999-12-31T23:59:59.999Z"; },
    (value) => { value.phases.developmentMs = value.phases.totalElapsedMs + 1; },
    (value) => { value.routing.areas = ["did:example:private-identifier"]; },
    (value) => { value.routing.targets = []; },
    (value) => { value.routing.targets = ["future-unknown-target"]; },
    (value) => { value.validations = Array.from({ length: 65 }, (_, index) => ({ name: `validation-${index}`, durationMs: 1, outcome: "passed" })); },
    (value) => { value.validations.push({ ...value.validations[0] }); },
    (value) => { value.ci.requiredChecks = 3; },
    (value) => { value.ci.checks[1].name = "basic"; },
    (value) => { value.worktree.peakTargetBytes = value.worktree.peakWorktreeBytes + 1; },
    (value) => { delete value.review.turns; },
  ]) {
    const invalid = record();
    mutate(invalid);
    assert.equal(validateMetricRecord(invalid).ok, false);
  }
  const secret = record();
  secret.routing.areas = ["ghp_abcdefghijklmnopqrstuvwxyz"];
  assert.ok(validateMetricRecord(secret).errors.some((problem) => problem.code === "secret"));

  const boundedNames = record();
  boundedNames.routing.areas = [
    "secret:scan", "token:usage", "secret:scanning-of-dependencies", "token:usage-per-session-x", "did:resolver", "openid-credential-offer",
  ];
  assert.equal(validateMetricRecord(boundedNames).ok, true);
  boundedNames.routing.areas = ["did:example:private-identifier"];
  assert.ok(validateMetricRecord(boundedNames).errors.some((problem) => problem.code === "secret"));

  const deeplyNested = record();
  deeplyNested.extra = Array.from({ length: 10 }).reduce((nested) => [nested], "value");
  assert.ok(validateMetricRecord(deeplyNested).errors.some((problem) => problem.code === "depth"));

  const injectedClock = record({ recordedAt: "2099-01-01T00:31:00.000Z" });
  assert.equal(validateMetricRecord(injectedClock, { nowMs: Date.parse("2099-01-01T00:32:00.000Z") }).ok, true);
  assert.equal(aggregateMetricRecords([injectedClock], [], { nowMs: Date.parse("2099-01-01T00:32:00.000Z") }).records.valid, 1);
});

test("generated templates cannot turn unknown measurements into zero", () => {
  const draft = metricTemplate({ issue: 167, headSha: HEAD, now: "2026-08-28T00:00:00.000Z" });
  assert.equal(draft.startedAt, null);
  assert.equal(draft.phases.developmentMs, null);
  assert.equal(draft.completedAt, null);
  assert.equal(draft.routing.targets.length, 0);
  assert.equal(validateMetricRecord(draft).ok, false);
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
  const corrected = record();
  corrected.validations[0].durationMs = 299_999;
  await writeMetricRecord(corrected, { outputDir, currentHead: HEAD, replace: true });
  assert.deepEqual(await readdir(outputDir), entriesBefore);
  assert.deepEqual(JSON.parse(await readFile(destination, "utf8")), corrected);
});

test("writer rejects permissive and symlinked output directories", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-output-"));
  const permissive = path.join(root, "permissive");
  await mkdir(permissive, { mode: 0o700 });
  await chmod(permissive, 0o755);
  await assert.rejects(writeMetricRecord(record(), { outputDir: permissive, currentHead: HEAD }), /group or other access/);

  const real = path.join(root, "real");
  const linked = path.join(root, "linked");
  await mkdir(real, { mode: 0o700 });
  await symlink(real, linked, "dir");
  await assert.rejects(writeMetricRecord(record(), { outputDir: linked, currentHead: HEAD }), /real directory/);
});

test("aggregate reports medians, p90, and tuning SLO violations without raw records", () => {
  const slow = record({
    issue: 168,
    pr: 169,
    completedAt: "2026-08-28T01:30:00.000Z",
    recordedAt: "2026-08-28T01:31:00.000Z",
    phases: { developmentMs: 2_000_000, reviewMs: 1_000_000, validationMs: 600_000, ciMs: 1_200_001, totalElapsedMs: 5_400_000 },
    review: { sessions: 5, turns: 20, toolCalls: 30, externalReviewRequired: true },
    tokens: { input: 200, output: 40, cacheRead: 20, cacheWrite: 0 },
    attempts: { pushesAfterFirstCi: 1, canceled: 1, failed: 1 },
    ci: {
      wallTimeMs: 1_200_001,
      requiredChecks: 2,
      failedChecks: 1,
      canceledRuns: 1,
      checks: [
        { name: "basic", queueMs: 60_000, durationMs: 90_000, outcome: "passed" },
        { name: "quality", queueMs: 120_000, durationMs: 1_200_001, outcome: "failed" },
      ],
    },
    worktree: { peakTargetBytes: 11 * 1024 ** 3, peakWorktreeBytes: 12 * 1024 ** 3 },
    routing: { profile: "feature", areas: ["harness"], targets: ["basic", "quality"] },
  });
  const aggregate = aggregateMetricRecords([record(), slow], [], { nowMs: Date.parse("2026-08-28T02:00:00.000Z") });
  assert.deepEqual(aggregate.records, { discovered: 2, valid: 2, invalid: 0 });
  assert.deepEqual(aggregate.distributions.totalElapsedMs, { median: 3_600_000, p90: 5_400_000 });
  assert.deepEqual(aggregate.distributions.totalTokens, { median: 195, p90: 260 });
  assert.deepEqual(aggregate.distributions.cacheReadTokens, { median: 15, p90: 20 });
  assert.equal(aggregate.totals.tokens, 390);
  assert.equal(aggregate.totals.externalReviewsRequired, 1);
  assert.deepEqual(aggregate.coverage.tokens, { available: 2, unavailable: 0 });
  assert.deepEqual(aggregate.ciChecks.basic, {
    records: 2,
    queueMs: { median: 30_500, p90: 60_000 },
    durationMs: { median: 90_000, p90: 90_000 },
    outcomes: { passed: 2, failed: 0, canceled: 0 },
  });
  assert.deepEqual(aggregate.validations["repository-contract"], {
    records: 2,
    durationMs: { median: 300_000, p90: 300_000 },
    outcomes: { passed: 2, failed: 0, canceled: 0 },
  });
  for (const key of ["routineOver60Minutes", "ciTargetOverBudget", "reviewSessionsOver4", "pushesAfterFirstCi", "failedOrCanceledAttempts", "targetOver10GiB"]) {
    assert.deepEqual(aggregate.sloViolations[key], [`issue-168/pr-169/${HEAD}`]);
  }
  assert.equal(JSON.stringify(aggregate).includes("repository-contract"), true);
  for (const rawKey of ["startedAt", "completedAt", "recordedAt", "valid.json"]) {
    assert.equal(JSON.stringify(aggregate).includes(`\"${rawKey}\"`), false, rawKey);
  }
});

test("aggregate excludes unavailable token telemetry instead of treating it as zero", () => {
  const unavailable = record({ issue: 170, pr: null, tokens: null });
  const aggregate = aggregateMetricRecords([unavailable, record()]);
  assert.deepEqual(aggregate.coverage.tokens, { available: 1, unavailable: 1 });
  assert.deepEqual(aggregate.distributions.totalTokens, { median: 130, p90: 130 });
  assert.equal(aggregate.totals.tokens, 130);
});

test("per-target CI SLO still catches a slow basic lane beside a high-budget lane", () => {
  const mixed = record({
    issue: 171,
    pr: 180,
    phases: { developmentMs: 600_000, reviewMs: 300_000, validationMs: 300_000, ciMs: 360_001, totalElapsedMs: 1_800_000 },
    ci: {
      wallTimeMs: 360_001,
      requiredChecks: 2,
      failedChecks: 0,
      canceledRuns: 0,
      checks: [
        { name: "basic", queueMs: 1_000, durationMs: 360_001, outcome: "passed" },
        { name: "nix-package", queueMs: 2_000, durationMs: 60_000, outcome: "passed" },
      ],
    },
    routing: { profile: "feature", areas: ["build"], targets: ["basic", "nix-package"] },
  });
  const aggregate = aggregateMetricRecords([mixed], [], { nowMs: Date.parse("2026-08-28T02:00:00.000Z") });
  assert.deepEqual(aggregate.sloViolations.ciTargetOverBudget, [`issue-171/pr-180/${HEAD}`]);
});

test("aggregate reports records beyond retention without deleting them", () => {
  const expired = record({
    startedAt: "2026-01-01T00:00:00.000Z",
    completedAt: "2026-01-01T00:30:00.000Z",
    recordedAt: "2026-01-01T00:31:00.000Z",
  });
  const aggregate = aggregateMetricRecords([expired], [], { nowMs: Date.parse("2026-08-28T00:00:00.000Z") });
  assert.deepEqual(aggregate.sloViolations.retentionOver90Days, [`issue-167/pr-170/${HEAD}`]);
});

test("aggregate reports unavailable and overflowed token totals without losing precision", () => {
  const unavailable = aggregateMetricRecords([record({ tokens: null })]);
  assert.equal(unavailable.totals.tokens, null);
  assert.deepEqual(unavailable.overflowedTotals, []);

  const huge = record({ tokens: { input: 1_000_000_000_000_000, output: 1_000_000_000_000_000, cacheRead: 1_000_000_000_000_000, cacheWrite: 0 } });
  const overflow = aggregateMetricRecords([huge, huge, huge, huge]);
  assert.equal(overflow.totals.tokens, null);
  assert.deepEqual(overflow.overflowedTotals, ["tokens"]);
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

test("audit never echoes an unknown secret-bearing property name", async () => {
  const inputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-unknown-key-"));
  const unsafe = record();
  unsafe.ghp_abcdefghijklmnopqrstuvwxyz = "Bearer private-value";
  await writeFile(path.join(inputDir, "unsafe.json"), `${JSON.stringify(unsafe)}\n`, { mode: 0o600 });
  const result = await auditMetricsDirectory(inputDir);
  assert.equal(result.ok, false);
  assert.equal(result.invalidRecords[0].errors[0].path, "$.<unknown>");
  assert.equal(JSON.stringify(result).includes("ghp_abcdefghijklmnopqrstuvwxyz"), false);
});

test("audit excludes duplicate issue/head identities even when PR metadata differs", async () => {
  const inputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-duplicate-"));
  await writeFile(path.join(inputDir, "first.json"), `${JSON.stringify(record())}\n`, { mode: 0o600 });
  await writeFile(path.join(inputDir, "second.json"), `${JSON.stringify(record({ pr: null }))}\n`, { mode: 0o600 });
  const result = await auditMetricsDirectory(inputDir);
  assert.equal(result.ok, false);
  assert.deepEqual(result.records, { discovered: 2, valid: 0, invalid: 2 });
  assert.equal(result.invalidRecords.every((entry) => entry.errors[0].code === "duplicate"), true);
});

test("CLI rejects options that belong to another command", async () => {
  await assert.rejects(runCli(["audit", "--replace"], { stdout: { write() {} } }), /--replace is not valid for this command/);
});

test("CLI resolves record stores against its injected checkout and rejects dirty writes", async () => {
  const checkout = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-cwd-"));
  const inputDir = path.join(checkout, "relative-audit");
  await mkdir(inputDir, { mode: 0o700 });
  await writeFile(path.join(inputDir, "valid.json"), `${JSON.stringify(record())}\n`, { mode: 0o600 });
  let output = "";
  assert.equal(await runCli(["audit", "--input-dir", "relative-audit", "--json"], { cwd: checkout, stdout: { write(chunk) { output += chunk; } } }), 0);
  assert.equal(JSON.parse(output).records.valid, 1);

  execFileSync("git", ["init", "--quiet"], { cwd: checkout });
  await writeFile(path.join(checkout, "tracked.txt"), "initial\n");
  execFileSync("git", ["add", "tracked.txt"], { cwd: checkout });
  execFileSync("git", ["-c", "commit.gpgsign=false", "-c", "user.name=Oxid Test", "-c", "user.email=oxid@example.invalid", "commit", "--quiet", "-m", "fixture"], { cwd: checkout });
  const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: checkout, encoding: "utf8" }).trim();
  const recordDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-record-"));
  const recordPath = path.join(recordDir, "record.json");
  await writeFile(recordPath, `${JSON.stringify(record({ headSha: head }))}\n`, { mode: 0o600 });
  const linkedRecord = path.join(recordDir, "linked-record.json");
  await symlink(recordPath, linkedRecord);
  await assert.rejects(
    runCli(["write", "--record", linkedRecord, "--output-dir", recordDir], { cwd: checkout, stdout: { write() {} } }),
    /regular file/,
  );
  const insideRecord = path.join(checkout, "inside-record.json");
  await writeFile(insideRecord, `${JSON.stringify(record({ headSha: head }))}\n`, { mode: 0o600 });
  await assert.rejects(
    runCli(["write", "--record", insideRecord, "--output-dir", recordDir], { cwd: checkout, stdout: { write() {} } }),
    /outside the worktree/,
  );
  await assert.rejects(
    runCli(["write", "--record", recordPath, "--output-dir", "inside-output"], { cwd: checkout, stdout: { write() {} } }),
    /outside the worktree/,
  );
  await writeFile(path.join(checkout, "dirty.txt"), "dirty\n");
  await assert.rejects(
    runCli(["write", "--record", recordPath, "--output-dir", recordDir], { cwd: checkout, stdout: { write() {} } }),
    /clean checkout/,
  );
});

test("audit rejects oversized records before parsing them", async () => {
  const inputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-size-"));
  await writeFile(path.join(inputDir, "oversized.json"), `{"padding":"${"x".repeat(256 * 1024)}"}\n`, { mode: 0o600 });
  const result = await auditMetricsDirectory(inputDir);
  assert.equal(result.ok, false);
  assert.equal(result.invalidRecords[0].errors[0].code, "size");
});

test("audit rejects a named pipe without blocking", async (context) => {
  const inputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-fifo-"));
  try {
    execFileSync("mkfifo", [path.join(inputDir, "blocked.json")]);
  } catch (cause) {
    if (cause?.code === "ENOENT") return context.skip("mkfifo is unavailable on this host");
    throw cause;
  }
  const result = await auditMetricsDirectory(inputDir);
  assert.equal(result.ok, false);
  assert.equal(result.invalidRecords[0].errors[0].code, "type");
});

test("template, default write, and default audit work from a checkout subdirectory", async () => {
  const checkout = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-roundtrip-"));
  const nested = path.join(checkout, "nested");
  await mkdir(nested, { mode: 0o700 });
  await writeFile(path.join(nested, "tracked.txt"), "fixture\n");
  execFileSync("git", ["init", "--quiet"], { cwd: checkout });
  execFileSync("git", ["add", "nested/tracked.txt"], { cwd: checkout });
  execFileSync("git", ["-c", "commit.gpgsign=false", "-c", "user.name=Oxid Test", "-c", "user.email=oxid@example.invalid", "commit", "--quiet", "-m", "fixture"], { cwd: checkout });
  const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: nested, encoding: "utf8" }).trim();
  let templateOutput = "";
  assert.equal(await runCli(["template", "--issue", "167", "--pr", "180", "--head", head], { cwd: nested, stdout: { write(chunk) { templateOutput += chunk; } } }), 0);
  assert.equal(JSON.parse(templateOutput).startedAt, null);

  const privateInput = await mkdtemp(path.join(os.tmpdir(), "oxid-metrics-roundtrip-input-"));
  const recordPath = path.join(privateInput, "completed.json");
  await writeFile(recordPath, `${JSON.stringify(record({ issue: 167, pr: 180, headSha: head }))}\n`, { mode: 0o600 });
  let writeOutput = "";
  assert.equal(await runCli(["write", "--record", recordPath], { cwd: nested, stdout: { write(chunk) { writeOutput += chunk; } } }), 0);
  assert.equal(JSON.parse(writeOutput).file, `issue-167-${head}.json`);
  assert.equal(execFileSync("git", ["status", "--porcelain=v1"], { cwd: checkout, encoding: "utf8" }).trim(), "");

  let auditOutput = "";
  assert.equal(await runCli(["audit", "--json"], { cwd: nested, stdout: { write(chunk) { auditOutput += chunk; } } }), 0);
  assert.deepEqual(JSON.parse(auditOutput).records, { discovered: 1, valid: 1, invalid: 0 });
});

test("every hosted target has one explicit budget matching the authoritative matrix", async () => {
  const expected = {
    basic: 5 * 60_000,
    "unit-linux": 10 * 60_000,
    "headless-linux": 10 * 60_000,
    "ui-linux": 20 * 60_000,
    "ui-release-linux": 25 * 60_000,
    "coverage-linux": 25 * 60_000,
    quality: 20 * 60_000,
    "nix-package": 45 * 60_000,
    "compact-artifacts": 30 * 60_000,
  };
  assert.deepEqual(CI_TARGET_BUDGET_MS, expected);
  assert.deepEqual(new Set(Object.keys(CI_TARGET_BUDGET_MS)), new Set(HOSTED_TARGETS));
  const matrix = await readFile(new URL("../../docs/factory/ci-target-matrix.md", import.meta.url), "utf8");
  for (const [target, milliseconds] of Object.entries(expected)) {
    const row = matrix.split("\n").find((line) => line.startsWith(`| \`${target}\` |`));
    assert.ok(row, `missing target-matrix row for ${target}`);
    assert.match(row, new RegExp(`hard ${milliseconds / 60_000} min(?:;|,| \\|)`), `budget mismatch for ${target}`);
  }
});

test("committed schema advertises the same closed v1 contract", async () => {
  const schema = JSON.parse(await readFile(new URL("../../docs/factory/work-item-metrics-v1.schema.json", import.meta.url), "utf8"));
  assert.equal(schema.properties.schemaVersion.const, 1);
  assert.match(schema.$id, /^https:\/\/raw\.githubusercontent\.com\/MediaNoxLabs\/oxid\/integration\//);
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(schema.properties.tokens.oneOf[0], { type: "null" });
  assert.deepEqual(schema.properties.ci.required, ["wallTimeMs", "requiredChecks", "failedChecks", "canceledRuns", "checks"]);
  assert.equal(schema.properties.validations.maxItems, 64);
  assert.equal(schema.properties.ci.properties.checks.maxItems, 64);
  assert.deepEqual(new Set(schema.$defs.hostedTargets.items.enum), new Set(HOSTED_TARGETS));
  assert.match("2026-08-28T00:00:00.000Z", new RegExp(schema.$defs.timestamp.pattern));
  assert.deepEqual(schema.properties.worktree.required, ["peakTargetBytes", "peakWorktreeBytes"]);
  for (const [schemaKeys, validatorKeys] of [
    [schema.required, METRIC_KEYS.top],
    [schema.properties.phases.required, METRIC_KEYS.phase],
    [schema.properties.validations.items.required, METRIC_KEYS.validation],
    [schema.properties.review.required, METRIC_KEYS.review],
    [schema.properties.tokens.oneOf[1].required, METRIC_KEYS.tokens],
    [schema.properties.attempts.required, METRIC_KEYS.attempts],
    [schema.properties.ci.required, METRIC_KEYS.ci],
    [schema.properties.ci.properties.checks.items.required, METRIC_KEYS.ciCheck],
    [schema.properties.worktree.required, METRIC_KEYS.worktree],
    [schema.properties.routing.required, METRIC_KEYS.routing],
  ]) assert.deepEqual(new Set(schemaKeys), new Set(validatorKeys));
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
  assert.match(metrics, /CI target and dependency matrix/);
  assert.match(loop, /prompts, transcripts, credentials, identifiers/);
  assert.match(runbook, /owner-private/);
});
