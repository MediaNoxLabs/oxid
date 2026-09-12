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
  digestGateCommand,
  runLocalGate,
  verifyLocalGate,
} from "../../scripts/loop/local-gate.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
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
  assert.match(agent, /Later review\/checkpoint logic invokes `verify` with the same planned command, not the full command/u);
});

test("the issue 449 sequence runs one full local gate and reuses its exact-head receipt", async (t) => {
  const { root, head } = await gateFixture(t);
  const command = ["just", "check"];
  let fullGateStarts = 0;
  const runChild = async () => {
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
    command,
    runChild,
    now,
  });
  assert.equal(implementation.action, "ran");
  assert.equal(implementation.receipt.headSha, head);
  assert.equal(implementation.receipt.commandDigest, digestGateCommand(command));

  const reviewer = await verifyLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    command,
  });
  const preApproval = await runLocalGate({
    cwd: root,
    deliveryBase: "origin/develop",
    gateId: "production-ready",
    command,
    runChild,
    now,
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
      command: ["env", "OXID_COVERAGE_BASE=origin/develop", "just", "check"],
      runChild,
    }),
    /commandDigest does not match/u,
  );
  assert.equal(fullGateStarts, 1, "a mismatched unchanged-head gate must stop instead of rerunning");
  await assert.rejects(
    verifyLocalGate({
      cwd: root,
      deliveryBase: "origin/develop",
      gateId: "production-ready",
      command: ["true"],
    }),
    /commandDigest does not match/u,
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
      command: ["just", "check"],
      runChild: async () => { starts += 1; return 0; },
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
