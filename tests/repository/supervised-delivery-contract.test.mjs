// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { EventEmitter } from "node:events";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { runManagedChild } from "../../scripts/lib/managed-child-process.mjs";
import {
  assertLocalGateDevShellCapabilities,
  digestGateCommand,
  runLocalGate,
  verifyLocalGate,
} from "../../scripts/loop/local-gate.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function targetPlan(rustChanged) {
  return () => ({
    paths: ["scripts/loop/local-gate.mjs"],
    plan: {
      areas: ["harness"],
      deliveryProfile: "production-ready",
      diffAvailable: true,
      profile: "feature",
      rustChanged,
      targets: ["basic"],
    },
  });
}

async function gateFixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-local-gate-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  git(root, ["init", "--quiet"]);
  await writeFile(path.join(root, "tracked.txt"), "fixture\n");
  git(root, ["add", "tracked.txt"]);
  execFileSync("git", [
    "-c", "commit.gpgsign=false",
    "-c", "user.name=Oxid Test",
    "-c", "user.email=oxid@example.invalid",
    "commit", "--quiet", "-m", "fixture",
  ], { cwd: root });
  const head = git(root, ["rev-parse", "HEAD"]);
  git(root, ["update-ref", "refs/remotes/origin/develop", head]);
  return { root, head };
}

test("local gate reports the bootstrap boundary before starting validation without timeout", async (t) => {
  const { root } = await gateFixture(t);
  assert.throws(
    () => assertLocalGateDevShellCapabilities({ environmentPath: path.join(root, "no-devshell-tools") }),
    /pinned devshell capability: timeout; rerun through \.\/bootstrap\.sh -- node scripts\/loop\/local-gate\.mjs/u,
  );

  let childStarted = false;
  await assert.rejects(
    runLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      assertCapabilities: () => assertLocalGateDevShellCapabilities({ environmentPath: path.join(root, "no-devshell-tools") }),
      runChild: async () => { childStarted = true; return 0; },
      resolvePlan: targetPlan(false),
    }),
    /pinned devshell capability: timeout/u,
  );
  assert.equal(childStarted, false);
  assert.doesNotThrow(() => assertLocalGateDevShellCapabilities({ hasCapability: () => true }));
});

test("the production-ready Pi contract has one non-delegating implementation child and a terminal metrics checkpoint", async () => {
  const [agent, policy, profiles] = await Promise.all([
    readFile(path.join(repoRoot, ".pi", "agents", "dev-loop.agent.md"), "utf8"),
    readFile(path.join(repoRoot, ".pi", "subagent-policy.json"), "utf8").then(JSON.parse),
    readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8").then(JSON.parse),
  ]);
  const tools = agent.match(/^tools:\s*(.+)$/mu)?.[1].split(",").map((value) => value.trim());
  assert.deepEqual(tools, ["read", "grep", "find", "ls", "bash", "edit", "write"]);
  assert.match(agent, /^maxSubagentDepth: 1$/mu);
  assert.equal(policy.maxSubagentSpawnsPerSession, 1);
  assert.equal(policy.maxSubagentSpawnsPerRun, 1);

  const supervision = profiles.profiles["production-ready"].supervision;
  assert.deepEqual(supervision, {
    implementationChildrenPerInvocation: 1,
    childMayDelegate: false,
    supervisorOwnedPhases: ["review", "hosted-ci", "retry", "metrics", "merge"],
    localGate: {
      gateId: "production-ready",
      receiptCommand: "node scripts/loop/local-gate.mjs",
      reusePolicy: "exact-head",
    },
    resumePolicy: "reuse-only",
    terminalCheckpoint: ["headSha", "validationReceipt", "workerMetrics", "remainingRisks"],
  });
  assert.match(agent, /never silently creates another phase child/u);
  assert.match(agent, /exactly one post-commit\s+canonical, change-relevant L0 receipt/u);
  assert.match(agent, /non-Rust\nplan runs `\.\/run\.sh repository --strict`; a Rust plan runs `\.\/run\.sh basic/u);
});

test("the production-ready sequence runs one canonical non-Rust receipt and reuses it", async (t) => {
  const { root, head } = await gateFixture(t);
  const command = ["./run.sh", "repository", "--strict"];
  let fullGateStarts = 0;
  const runChild = async (file, args) => {
    assert.deepEqual([file, ...args], command);
    fullGateStarts += 1;
    return 0;
  };
  let clock = Date.parse("2026-09-12T12:00:00.000Z");
  const now = () => {
    const current = clock;
    clock += 1_000;
    return current;
  };

  const implementation = await runLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    runChild,
    now,
    resolvePlan: targetPlan(false),
  });
  assert.equal(implementation.action, "ran");
  assert.equal(implementation.receipt.headSha, head);
  assert.equal(implementation.receipt.commandDigest, digestGateCommand(command));

  const reviewer = await verifyLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    resolvePlan: targetPlan(false),
  });
  const preApproval = await runLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    runChild,
    now,
    resolvePlan: targetPlan(false),
  });
  assert.equal(reviewer.action, "verified");
  assert.equal(preApproval.action, "reused");
  assert.equal(fullGateStarts, 1, "implementation + reviewer + pre-approval must start the full gate at most once");

  const receiptPath = path.join(
    git(root, ["rev-parse", "--path-format=absolute", "--git-common-dir"]),
    "oxid-factory", "local-gates-v1", `${head}-production-ready.json`,
  );
  assert.equal((await stat(receiptPath)).mode & 0o777, 0o600);
  await assert.rejects(
    runLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      command: ["just", "check"],
      runChild,
      resolvePlan: targetPlan(false),
    }),
    /immutable repository-owned command/u,
  );
  assert.equal(fullGateStarts, 1, "a mismatched unchanged-head gate must stop instead of rerunning");
  await assert.rejects(
    verifyLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      command: ["true"],
      resolvePlan: targetPlan(false),
    }),
    /immutable repository-owned command/u,
  );
});

test("production-ready runs the Rust-safe basic target and fails closed for an invalid plan", async (t) => {
  const { root } = await gateFixture(t);
  let command;
  const result = await runLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    runChild: async (file, args) => { command = [file, ...args]; return 0; },
    resolvePlan: targetPlan(true),
  });
  assert.equal(result.action, "ran");
  assert.deepEqual(command, ["./run.sh", "basic", "--strict"]);

  await assert.rejects(
    runLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      runChild: async () => 0,
      resolvePlan: () => ({
        paths: ["scripts/loop/local-gate.mjs"],
        plan: { diffAvailable: true, rustChanged: "true" },
      }),
    }),
    /available, well-formed target plan/u,
  );
});

test("resume-first refuses an in-flight unchanged-head gate instead of launching a replacement", async (t) => {
  const { root, head } = await gateFixture(t);
  const common = git(root, ["rev-parse", "--path-format=absolute", "--git-common-dir"]);
  const lock = path.join(common, "oxid-factory", "local-gates-v1", `${head}-production-ready.lock`);
  await import("node:fs/promises").then(({ mkdir }) => mkdir(lock, { recursive: true, mode: 0o700 }));
  let starts = 0;
  await assert.rejects(
    runLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      runChild: async () => { starts += 1; return 0; },
      resolvePlan: targetPlan(false),
    }),
    /reconcile it instead of launching a replacement/u,
  );
  assert.equal(starts, 0);
});

test("bounded drain escalation kills the exact owned descendant process group", { skip: process.platform === "win32" }, async () => {
  const processRef = new EventEmitter();
  const child = new EventEmitter();
  child.pid = 452;
  const signals = [];
  let escalate;
  const completion = runManagedChild("node", ["fixture"], {
    processRef,
    platform: "darwin",
    spawnImpl: () => child,
    kill: (pid, signal) => {
      signals.push([pid, signal]);
      if (signal === "SIGKILL") child.emit("close", null, "SIGKILL");
    },
    setTimeoutImpl: (callback, delay) => {
      assert.equal(delay, 5);
      escalate = callback;
      return { unref() {} };
    },
    clearTimeoutImpl: () => {},
    graceMs: 5,
  });
  processRef.emit("SIGTERM");
  assert.equal(typeof escalate, "function");
  escalate();
  assert.equal(await completion, 143);
  assert.deepEqual(signals, [[-452, "SIGTERM"], [-452, "SIGKILL"]]);
  assert.equal(processRef.listenerCount("SIGTERM"), 0);
  assert.equal(processRef.listenerCount("exit"), 0);
});
