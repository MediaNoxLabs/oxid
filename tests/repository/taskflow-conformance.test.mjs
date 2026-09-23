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
  assert.equal(report.timings.realTimeOverFiveMinutes, "on-demand only");
  assert.deepEqual(report.matrix.map(({ property, status }) => [property, status]), [
    ["bounded-progress-visibility", "supported"],
    ["slow-versus-stalled-classification", "unverified"],
    ["process-tree-cancellation-escalation", "unverified"],
    ["terminal-cleanup", "unverified"],
    ["immutable-resume", "supported"],
    ["changed-input-invalidation", "supported"],
  ]);
  assert.match(report.matrix.find(({ property }) => property === "immutable-resume").evidence, /parentUnchanged=true/u);
  assert.match(report.matrix.find(({ property }) => property === "changed-input-invalidation").evidence, /changedExecuted=1; changedReused=0; repeatedReused=1/u);
});
