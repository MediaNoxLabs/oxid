// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { mkdtemp, readFile, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  alertIsRecentUnresolved,
  evaluateResourceAdmission,
  parseResourceMonitorLog,
  writeResourceAdmissionReceipt,
} from "../../scripts/factory/resource-admission.mjs";

const GiB = 1024 ** 3;
const healthy = {
  samples: [
    { atMs: 0, swapUsedBytes: 10 * GiB },
    { atMs: 120_000, swapUsedBytes: 10 * GiB + 16 * 1024 ** 2 },
  ],
  availableBytes: 3 * GiB,
  memoryPressure: "healthy",
  diskAvailableBytes: 30 * GiB,
  diskFloorBytes: 20 * GiB,
  recentUnresolvedAlert: false,
  requestedLane: "factory",
  activeHeavyLanes: 0,
  workloads: [{ ownership: "unowned", label: "Docker" }],
};

test("resource admission keeps the pressure decision fail-closed and lane-bounded", () => {
  assert.equal(evaluateResourceAdmission(healthy).decision, "allow");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: [{ atMs: 0, swapUsedBytes: 25 * GiB }, { atMs: 120_000, swapUsedBytes: 25 * GiB }] }).decision, "degraded");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: null }).reasonCode, "telemetry-missing");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: [{ atMs: 0, swapUsedBytes: 1 }, { atMs: 1, swapUsedBytes: 1 }] }).reasonCode, "telemetry-contradictory");
  assert.equal(evaluateResourceAdmission({ ...healthy, recentUnresolvedAlert: true }).reasonCode, "monitor-alert-unresolved");
  assert.equal(evaluateResourceAdmission({ ...healthy, availableBytes: 2 * GiB }).reasonCode, "available-memory-low");
  assert.equal(evaluateResourceAdmission({ ...healthy, diskAvailableBytes: 20 * GiB }).reasonCode, "disk-headroom-low");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: [{ atMs: 0, swapUsedBytes: 1 }, { atMs: 120_000, swapUsedBytes: GiB + 1 }] }).reasonCode, "swap-growing");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: [{ atMs: 0, swapUsedBytes: 25 * GiB }, { atMs: 120_000, swapUsedBytes: 25 * GiB }] , requestedLane: "simulator" }).reasonCode, "degraded-lane-disallowed");
  assert.equal(evaluateResourceAdmission({ ...healthy, samples: [{ atMs: 0, swapUsedBytes: 25 * GiB }, { atMs: 120_000, swapUsedBytes: 25 * GiB }] , activeHeavyLanes: 1 }).reasonCode, "degraded-concurrency-disallowed");
  const result = evaluateResourceAdmission({ ...healthy, contradictory: true });
  assert.equal(result.decision, "block");
  assert.equal(result.summary.workloads[0].ownership, "unowned");
  assert.equal(Object.keys(result).some((key) => /signal|stop|restart/i.test(key)), false);
});

test("resource-monitor evidence is fresh, normalized, and alert-aware", () => {
  const now = new Date("2026-09-30T13:13:30+08:00").getTime();
  const resources = [
    "2026-09-30 13:11:06 | swap=25000MB docker_krun=4616MB qemu=1978MB | 95G used, 555M unused. | top: docker",
    "2026-09-30 13:13:17 | swap=25008MB docker_krun=4477MB qemu=1945MB | 94G used, 3G unused. | top: docker",
  ].join("\n");
  const parsed = parseResourceMonitorLog(resources, now);
  assert.equal(parsed.samples[1].swapUsedBytes, 25008 * 1024 ** 2);
  assert.equal(parsed.availableBytes, 3 * GiB);
  assert.deepEqual(parsed.workloads, [
    { ownership: "unowned", label: "docker" },
    { ownership: "unowned", label: "android-emulator" },
  ]);
  assert.equal(parseResourceMonitorLog(resources, now + 10 * 60_000), null);
  assert.equal(alertIsRecentUnresolved("2026-09-30 13:12:00 ALERT pressure", now), true);
  assert.equal(alertIsRecentUnresolved("2026-09-30 13:12:00 ALERT pressure\n2026-09-30 13:13:00 RESOLVED pressure", now), false);
  assert.equal(alertIsRecentUnresolved("2026-09-30 12:00:00 ALERT old", now), false);
});

test("resource admission receipts are private, bounded, and correlated", async (t) => {
  const outputDir = await mkdtemp(path.join(os.tmpdir(), "oxid-resource-admission-"));
  t.after(() => import("node:fs/promises").then(({ rm }) => rm(outputDir, { recursive: true, force: true })));
  const result = evaluateResourceAdmission(healthy);
  const destination = await writeResourceAdmissionReceipt(result, { outputDir, issue: 853, headSha: "a".repeat(40), run: "run-1" });
  assert.equal((await stat(destination)).mode & 0o777, 0o600);
  const saved = JSON.parse(await readFile(destination, "utf8"));
  assert.equal(saved.issue, 853);
  assert.equal(saved.decision, "allow");
  assert.deepEqual(saved.summary.workloads, [{ ownership: "unowned", label: "Docker" }]);
  await assert.rejects(writeResourceAdmissionReceipt(result, { outputDir, issue: 853, headSha: "a".repeat(40), run: "run-1" }), /already exists/);
});
