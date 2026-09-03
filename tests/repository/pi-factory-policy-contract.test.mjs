// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { createDeliveryBranchRewriteSink } from "../../scripts/loop/pre-flight-gate.mjs";
import { auditPi, auditWorktreeAdmission } from "../../scripts/factory/audit-pi.mjs";
import { applyUserPolicy, mergePolicy, policyMismatches } from "../../scripts/factory/pi-policy.mjs";
import { FACTORY_STATE_LABELS, syncFactoryLabels } from "../../scripts/github/sync-factory-labels.mjs";
import { applyDeliveryProfile, extractDeliveryProfileArgs } from "../../scripts/dev-loops.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("tracked Pi policy uses balanced Codex defaults and exact package pins", async () => {
  const settings = JSON.parse(await readFile(path.join(repoRoot, ".pi", "settings.json"), "utf8"));
  assert.match(settings.defaultProvider, /^[a-z0-9-]+$/u);
  assert.match(settings.defaultModel, /^[a-z0-9.-]+$/u);
  assert.match(settings.defaultThinkingLevel, /^(?:off|minimal|low|medium|high|xhigh|max)$/u);
  assert.equal(settings.retry.maxRetries, 1);
  assert.equal(settings.retry.provider.timeoutMs, 600000);
  assert.equal(settings.retry.provider.maxRetries, 0);
  assert.deepEqual(settings.packages, [
    "npm:dev-loops@0.9.0",
    "npm:pi-subagents@0.42.1",
    "npm:@input-output-hk/agent-review-pi@0.5.0",
  ]);
  assert.equal(settings.subagents.defaultModel, `${settings.defaultProvider}/${settings.defaultModel}`);
  assert.equal(settings.subagents.defaultThinking, settings.defaultThinkingLevel);
  const smoke = await readFile(path.join(repoRoot, "scripts", "check-pi-devshell.sh"), "utf8");
  assert.match(smoke, /pi --list-models/u);
});

test("delivery profiles keep prototype evidence local and promotion explicit", async () => {
  const profiles = JSON.parse(await readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8"));
  assert.equal(profiles.defaultProfile, "production-ready");
  assert.deepEqual(Object.keys(profiles.profiles).sort(), ["production-ready", "prototype"]);

  const prototype = profiles.profiles.prototype;
  assert.equal(prototype.remoteMutation, false);
  assert.equal(prototype.mergeEligible, false);
  assert.equal(prototype.evidenceClass, "provisional");
  assert.equal(prototype.maximumReviewers, 1);
  assert.deepEqual(prototype.targets.required, ["basic"]);
  assert.deepEqual(prototype.targets.optionalHostedOnDemand, ["unit-linux", "headless-linux"]);
  assert.equal(prototype.targets.maximumFocusedQualifications, 1);

  assert.deepEqual(profiles.promotion, {
    explicit: true,
    refreshBase: "origin/develop",
    auditPrototypeGaps: true,
    invalidateProvisionalEvidence: true,
    recomputeTargets: true,
  });

  const [rootAgent, devLoopAgent, developerAgent, reviewAgent, productiveLoop] = await Promise.all([
    readFile(path.join(repoRoot, "AGENT.md"), "utf8"),
    readFile(path.join(repoRoot, ".pi", "agents", "dev-loop.agent.md"), "utf8"),
    readFile(path.join(repoRoot, ".pi", "agents", "developer.agent.md"), "utf8"),
    readFile(path.join(repoRoot, ".pi", "agents", "review.agent.md"), "utf8"),
    readFile(path.join(repoRoot, "docs", "factory", "productive-loop.md"), "utf8"),
  ]);
  for (const source of [rootAgent, devLoopAgent, productiveLoop]) {
    assert.match(source, /\/dev-loop prototype issue <n>/u);
    assert.match(source, /\/dev-loop production-ready issue <n>/u);
  }
  assert.match(developerAgent, /deliveryProfile: prototype/u);
  assert.match(reviewAgent, /provisional/u);
  assert.match(productiveLoop, /Do not turn a prototype into a PR by merely pushing its head/u);
});

test("the handoff wrapper makes prototype local and production-ready the default", async () => {
  const contract = JSON.parse(await readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8"));
  const base = {
    target: { kind: "issue", issue: 228, repo: "owner/repo" },
    deliveryProfile: undefined,
    executionMode: "durable_auto",
    currentGate: "draft",
    nextAction: "create a draft PR",
    requiredReads: ["AGENT.md"],
    stopRules: ["merge"],
    maxCopilotRounds: 5,
    requireDraftFirst: true,
    gateConfig: { requireCi: true },
    acceptance: { criteria: [], evidence: [], maxFinalizationTurns: 6 },
    control: { needsAttentionAfterMs: 300000, activeNoticeAfterMs: 300000 },
  };

  assert.deepEqual(extractDeliveryProfileArgs([
    "--input", "state.json", "--delivery-profile=prototype", "--gate-state", "{}",
  ]), {
    args: ["--input", "state.json", "--gate-state", "{}"],
    requested: "prototype",
  });
  assert.throws(
    () => extractDeliveryProfileArgs(["--delivery-profile", "prototype", "--delivery-profile=production-ready"]),
    /only once/u,
  );

  const prototype = applyDeliveryProfile(base, contract, "prototype");
  assert.equal(prototype.deliveryProfile, "prototype");
  assert.equal(prototype.executionMode, "bounded_handoff");
  assert.equal(prototype.requireDraftFirst, false);
  assert.equal(prototype.maxCopilotRounds, 0);
  assert.equal(Object.hasOwn(prototype, "gateConfig"), false);
  assert.deepEqual(prototype.stopRules, ["remote-mutation", "hosted-ci", "merge-readiness", "merge"]);
  assert.equal(prototype.control.needsAttentionAfterMs, 180000);
  assert.equal(prototype.control.activeNoticeAfterMs, 600000);
  assert.equal(prototype.acceptance.criteria.length, contract.profiles.prototype.closeoutFields.length);

  const production = applyDeliveryProfile(base, contract, "production-ready");
  assert.equal(production.deliveryProfile, "production-ready");
  assert.equal(production.nextAction, base.nextAction);
  assert.deepEqual(production.stopRules, base.stopRules);
  assert.equal(production.requiredReads.includes(".pi/delivery-profiles.json"), true);
  assert.throws(
    () => applyDeliveryProfile({ ...base, target: { kind: "pr", pr: 1, repo: "owner/repo" } }, contract, "prototype"),
    /issue-backed target/u,
  );
});

test("unavailable lifecycle helper uses conservative fresh-checkout capacity", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-admission-unavailable-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  assert.equal(spawnSync("git", ["init", "--quiet", root]).status, 0);
  const result = await auditWorktreeAdmission({ repoRoot: root });
  assert.equal(result.admissionReady, true);
  assert.equal(result.capacityEvidenceAvailable, true);
  assert.deepEqual(result.checks.map(({ id, status }) => ({ id, status })), [
    { id: "worktree-admission", status: "pass" },
    { id: "worktree-target-storage", status: "pass" },
  ]);
  assert.match(result.checks[0].summary, /conservative fallback/u);
});

test("config-only audit rejects admission enforcement instead of reporting false red", () => {
  const result = spawnSync(process.execPath, [
    "scripts/factory/audit-pi.mjs", "--config-only", "--enforce-admission",
  ], { cwd: repoRoot, encoding: "utf8" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /--config-only cannot be combined with --enforce-admission/u);
});

test("user subagent policy merge is bounded and preserves unrelated settings", () => {
  const policy = {
    maxSubagentDepth: 2,
    parallel: { maxTasks: 2, concurrency: 2 },
    usageBudget: { tokens: { soft: 120000, hard: 200000 } },
  };
  const merged = mergePolicy({ unrelated: true, parallel: { legacy: "preserved", concurrency: 9 } }, policy);
  assert.deepEqual(merged, {
    unrelated: true,
    parallel: { legacy: "preserved", maxTasks: 2, concurrency: 2 },
    maxSubagentDepth: 2,
    usageBudget: { tokens: { soft: 120000, hard: 200000 } },
  });
  assert.deepEqual(policyMismatches(merged, policy), []);
  assert.deepEqual(policyMismatches({}, policy).map((item) => item.field), [
    "maxSubagentDepth",
    "parallel.maxTasks",
    "parallel.concurrency",
    "usageBudget.tokens.soft",
    "usageBudget.tokens.hard",
  ]);
});

test("preflight rewrites split and late generic main guidance to integration", async () => {
  let output = "";
  const destination = new Writable({
    write(chunk, _encoding, callback) {
      output += chunk.toString();
      callback();
    },
  });
  const rewritten = createDeliveryBranchRewriteSink(destination);
  rewritten.sink.write("  (creates+provisions tmp/worktrees/dev-loops/<kind>-<n> from origin/ma");
  rewritten.sink.write("in)\n");
  await new Promise((resolve, reject) => rewritten.sink.write(
    "legitimate origin/main and origin/main-release diagnostic\n",
    (error) => error ? reject(error) : resolve(),
  ));
  await rewritten.flush();
  assert.equal(output, [
    "  (creates+provisions tmp/worktrees/dev-loops/<kind>-<n> from origin/develop)",
    "legitimate origin/main and origin/main-release diagnostic",
    "",
  ].join("\n"));
  await new Promise((resolve, reject) => rewritten.sink.write(
    "late origin/main output\n",
    (error) => error ? reject(error) : resolve(),
  ));
  assert.match(output, /late origin\/main output/u);
});

test("preflight rewrite sink honors destination backpressure", async () => {
  let output = "";
  let release;
  const destination = new Writable({
    highWaterMark: 1,
    write(chunk, _encoding, callback) {
      output += chunk.toString();
      release = callback;
    },
  });
  const rewritten = createDeliveryBranchRewriteSink(destination);
  let completed = false;
  const write = new Promise((resolve, reject) => rewritten.sink.write(
    `prefix ${"x".repeat(64)} (creates+provisions tmp/worktrees/dev-loops/<kind>-<n> from origin/main)\n`,
    (error) => {
      completed = true;
      if (error) reject(error);
      else resolve();
    },
  ));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(completed, false);
  release();
  await write;
  const flush = rewritten.flush();
  await new Promise((resolve) => setImmediate(resolve));
  release();
  await flush;
  assert.match(output, /origin\/develop/u);
  assert.doesNotMatch(output, /origin\/main/u);
});

test("user policy apply is explicit, atomic, private, and preserves existing keys", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-pi-policy-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const agentDir = path.join(root, "agent");
  const configPath = path.join(agentDir, "extensions", "subagent", "config.json");
  const env = { PI_CODING_AGENT_DIR: agentDir };

  await assert.rejects(applyUserPolicy({ env }), /without --execute/u);
  const created = await applyUserPolicy({ env, execute: true });
  assert.equal(created.changed, true);
  assert.equal(created.backupPath, null);
  assert.equal((await stat(configPath)).mode & 0o777, 0o600);

  const aligned = await applyUserPolicy({ env, execute: true });
  assert.equal(aligned.changed, false);
  assert.equal(aligned.backupPath, null);

  const existing = { unrelated: { preserved: true }, parallel: { concurrency: 99 } };
  await writeFile(configPath, `${JSON.stringify(existing)}\n`);
  await chmod(configPath, 0o644);
  const updated = await applyUserPolicy({ env, execute: true });
  assert.equal(updated.changed, true);
  assert.equal((await stat(configPath)).mode & 0o777, 0o600);
  assert.equal((await stat(updated.backupPath)).mode & 0o777, 0o600);
  assert.deepEqual(JSON.parse(await readFile(updated.backupPath, "utf8")), existing);
  assert.equal(JSON.parse(await readFile(configPath, "utf8")).unrelated.preserved, true);

  await writeFile(configPath, `${JSON.stringify(existing)}\n`);
  const repeated = await applyUserPolicy({ env, execute: true });
  assert.equal(repeated.backupPath, updated.backupPath);
  assert.deepEqual(JSON.parse(await readFile(repeated.backupPath, "utf8")), existing);
});

test("user policy apply leaves invalid JSON untouched", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-pi-policy-invalid-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const agentDir = path.join(root, "agent");
  const configPath = path.join(agentDir, "extensions", "subagent", "config.json");
  await mkdir(path.dirname(configPath), { recursive: true });
  await writeFile(configPath, "{invalid\n");
  await assert.rejects(applyUserPolicy({ env: { PI_CODING_AGENT_DIR: agentDir }, execute: true }), /Cannot read/u);
  assert.equal(await readFile(configPath, "utf8"), "{invalid\n");
});

test("factory claim surface fails closed and exposes no raw GitHub mutations", async () => {
  const source = await readFile(path.join(repoRoot, ".pi", "extensions", "factory.ts"), "utf8");
  assert.match(source, /Claiming #\$\{issue\} is disabled/u);
  assert.doesNotMatch(source, /["'](?:issue|pr)["']\s*,\s*["'](?:edit|comment|close|reopen|delete|merge|create)["']/u);
  assert.doesNotMatch(source, /factory\/\$\{issue\}/u);
});

test("factory state labels are complete, unique, and dry-run by default", () => {
  const expected = [
    "factory:ready",
    "factory:claimed",
    "factory:in-progress",
    "factory:gate-draft",
    "factory:gate-preapproval",
    "factory:merge-ready",
    "factory:blocked",
  ];
  assert.deepEqual(FACTORY_STATE_LABELS.map((label) => label.name), expected);
  assert.equal(new Set(expected).size, expected.length);
  const output = [];
  let mutations = 0;
  syncFactoryLabels({
    stdout: { write(value) { output.push(value); } },
    run() { mutations += 1; },
  });
  assert.equal(mutations, 0);
  assert.deepEqual(output, expected.map((name) => `would sync ${name}\n`));
});

test("read-only Pi audit recognizes tracked configuration controls", async () => {
  const result = await auditPi({
    repoRoot,
    includeOperational: false,
    piVersion: "0.84.0",
    userPolicyResult: { ok: true, configPath: "/private/policy.json", mismatches: [] },
  });
  assert.equal(result.operationalChecked, false);
  assert.equal(result.admissionReady, null);
  const byId = new Map(result.checks.map((entry) => [entry.id, entry]));
  for (const id of [
    "project-pi-policy",
    "package-pins",
    "pi-runtime",
    "tracked-agent-budgets",
    "user-subagent-policy",
    "dev-loop-bounds",
    "delivery-profiles",
  ]) {
    assert.equal(byId.get(id)?.status, "pass", `${id}: ${byId.get(id)?.summary}`);
  }
});

test("factory topology permits isolated multi-host workers without sharing mutation lanes", async () => {
  const topology = await readFile(path.join(repoRoot, "docs", "factory", "worker-topology.md"), "utf8");
  const runbook = await readFile(path.join(repoRoot, "docs", "factory", "runbook.md"), "utf8");
  const productiveLoop = await readFile(path.join(repoRoot, "docs", "factory", "productive-loop.md"), "utf8");
  const devloops = await readFile(path.join(repoRoot, ".devloops"), "utf8");
  const normalizeProse = (value) => value.replace(/\s+/gu, " ");

  assert.match(normalizeProse(topology), /every mutating parent session owns one issue and one isolated worktree/u);
  assert.match(topology, /Two active managed delivery worktrees/u);
  assert.match(topology, /cloud worker is possible/u);
  assert.match(topology, /--provider openai --model <model>/u);
  assert.match(topology, /Cloud workers are not autonomous claimants until the atomic lease work/u);
  assert.match(normalizeProse(runbook), /one remote candidate active per parent session/u);
  assert.match(normalizeProse(productiveLoop), /per Git common checkout on a host/u);
  assert.match(devloops, /One delivery candidate per conductor/u);
});
