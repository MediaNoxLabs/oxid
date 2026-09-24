// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("synthetic Taskflow conformance matrix is pinned, fast, and headless", () => {
  const result = spawnSync(process.execPath, ["scripts/factory/taskflow-conformance.mjs", "--json", "--step-ms", "30"], {
    encoding: "utf8",
    timeout: 10_000,
  });
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.deepEqual(report.versions, { piTaskflow: "0.2.10", taskflowCore: "0.2.10" });
  assert.equal(report.mode, "synthetic-headless-black-box");
  assert.equal(report.timings.realTimeOverFiveMinutes, false);
  assert.equal(report.timings.supervisorHeartbeats, 0);
  assert.ok(report.timings.progressCallbacks > 0);
  assert.ok(report.timings.maxCallbackGapMs < 1_000);
  assert.deepEqual(report.matrix.map(({ property, status }) => [property, status]), [
    ["bounded-progress-visibility", "supported"],
    ["slow-versus-stalled-classification", "unverified"],
    ["process-tree-cancellation-escalation", "supported"],
    ["terminal-cleanup", "supported"],
    ["immutable-resume", "supported"],
    ["changed-input-invalidation", "supported"],
  ]);
  assert.match(report.matrix.find(({ property }) => property === "immutable-resume").evidence, /parentUnchanged=true/u);
  assert.match(report.matrix.find(({ property }) => property === "changed-input-invalidation").evidence, /changedExecuted=1; changedReused=0; repeatedReused=1/u);
  assert.match(report.matrix.find(({ property }) => property === "process-tree-cancellation-escalation").evidence, /descendantsReaped=true/u);
});

test("long-process mode labels supervisor heartbeats without claiming five-minute admission", () => {
  const result = spawnSync(process.execPath, [
    "scripts/factory/taskflow-conformance.mjs", "--json", "--long-process", "--step-ms", "40", "--heartbeat-ms", "10",
  ], { encoding: "utf8", timeout: 10_000 });
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.mode, "long-process-headless-black-box");
  assert.equal(report.timings.realTimeOverFiveMinutes, false);
  assert.ok(report.timings.supervisorHeartbeats > 0);
  assert.match(result.stderr, /supervisor-heartbeat/u);
  assert.deepEqual(report.matrix.slice(0, 2).map(({ property, status }) => [property, status]), [
    ["saved-flow-long-script-admission", "supported"],
    ["direct-executor-long-script", "unverified"],
  ]);
});

test("a five-minute-plus probe requires explicit long-process mode", () => {
  const result = spawnSync(process.execPath, [
    "scripts/factory/taskflow-conformance.mjs", "--json", "--step-ms", "300001",
  ], { encoding: "utf8", timeout: 10_000 });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /requires explicit --long-process mode/u);
});
