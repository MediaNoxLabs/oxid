// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { Writable } from "node:stream";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { createDeliveryBranchRewriteSink } from "../../scripts/loop/pre-flight-gate.mjs";
import { auditPi } from "../../scripts/factory/audit-pi.mjs";
import { mergePolicy, policyMismatches } from "../../scripts/factory/pi-policy.mjs";
import { FACTORY_STATE_LABELS, syncFactoryLabels } from "../../scripts/github/sync-factory-labels.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("tracked Pi policy uses balanced Codex defaults and exact package pins", async () => {
  const settings = JSON.parse(await readFile(path.join(repoRoot, ".pi", "settings.json"), "utf8"));
  assert.equal(settings.defaultProvider, "openai-codex");
  assert.equal(settings.defaultModel, "gpt-5.6-terra");
  assert.equal(settings.defaultThinkingLevel, "medium");
  assert.equal(settings.retry.maxRetries, 1);
  assert.equal(settings.retry.provider.timeoutMs, 600000);
  assert.equal(settings.retry.provider.maxRetries, 0);
  assert.deepEqual(settings.packages, [
    "npm:dev-loops@0.9.0",
    "npm:pi-subagents@0.42.1",
    "npm:@input-output-hk/agent-review-pi@0.5.0",
  ]);
  assert.equal(settings.subagents.defaultModel, "openai-codex/gpt-5.6-terra");
  assert.equal(settings.subagents.defaultThinking, "medium");
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

test("preflight rewrites split generic main guidance to integration", () => {
  let output = "";
  const destination = new Writable({
    write(chunk, _encoding, callback) {
      output += chunk.toString();
      callback();
    },
  });
  const rewritten = createDeliveryBranchRewriteSink(destination);
  rewritten.sink.write("create from origin/ma");
  rewritten.sink.write("in, then continue\n");
  rewritten.flush();
  assert.equal(output, "create from origin/integration, then continue\n");
  assert.doesNotMatch(output, /origin\/main/u);
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
    "worker-topology",
    "bounded-closeout",
    "factory-github-authority",
    "factory-label-taxonomy",
    "integration-recovery-guidance",
  ]) {
    assert.equal(byId.get(id)?.status, "pass", `${id}: ${byId.get(id)?.summary}`);
  }
});

test("factory topology permits isolated multi-host workers without sharing mutation lanes", async () => {
  const topology = await readFile(path.join(repoRoot, "docs", "factory", "worker-topology.md"), "utf8");
  const runbook = await readFile(path.join(repoRoot, "docs", "factory", "runbook.md"), "utf8");
  const productiveLoop = await readFile(path.join(repoRoot, "docs", "factory", "productive-loop.md"), "utf8");
  const devloops = await readFile(path.join(repoRoot, ".devloops"), "utf8");

  assert.match(topology, /every mutating parent session owns one issue and one isolated\nworktree/u);
  assert.match(topology, /Two active managed delivery worktrees/u);
  assert.match(topology, /cloud worker is possible/u);
  assert.match(topology, /--provider openai --model <model>/u);
  assert.match(topology, /Cloud workers are not autonomous claimants until the atomic lease work/u);
  assert.match(runbook, /one remote candidate active per parent session/u);
  assert.match(productiveLoop, /per Git common checkout\n  on a host/u);
  assert.match(devloops, /One delivery candidate per conductor/u);
});
