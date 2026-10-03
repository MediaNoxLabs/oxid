// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  acquireMaestroIosLane,
  cleanupMaestroIosLane,
  releaseMaestroIosLane,
} from "../../scripts/lib/maestro-ios-lane.mjs";

const owner = (overrides = {}) => ({
  pid: process.pid,
  host: os.hostname(),
  startedAt: new Date().toISOString(),
  worktree: "/fixture/worktree",
  flow: "fixture-flow",
  ...overrides,
});

test("Maestro iOS lane serializes a live local owner", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-maestro-lane-live-"));
  const lockPath = path.join(root, "lane.lock");
  t.after(() => rm(root, { recursive: true, force: true }));
  const first = await acquireMaestroIosLane({ lockPath, ...owner() });
  const second = await acquireMaestroIosLane({ lockPath, ...owner({ flow: "other" }) });
  assert.equal(first.outcome, "acquired");
  assert.equal(second.outcome, "waiting");
  assert.equal(second.owner.token, first.owner.token);
  assert.equal(await releaseMaestroIosLane(lockPath, first.owner.token), true);
});

test("Maestro iOS lane recovers a demonstrably dead local owner", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-maestro-lane-dead-"));
  const lockPath = path.join(root, "lane.lock");
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(lockPath);
  await writeFile(path.join(lockPath, "owner.json"), `${JSON.stringify({
    ...owner({ pid: 2_147_483_647 }), token: "dead-owner-token",
  })}\n`);
  const result = await acquireMaestroIosLane({ lockPath, ...owner() });
  assert.equal(result.outcome, "recovered");
  assert.equal(result.recoveredOwner.token, "dead-owner-token");
  assert.equal(await releaseMaestroIosLane(lockPath, result.owner.token), true);
});

test("Maestro iOS lane fails closed for remote and malformed ownership", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-maestro-lane-ambiguous-"));
  const remotePath = path.join(root, "remote.lock");
  const malformedPath = path.join(root, "malformed.lock");
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(remotePath);
  await writeFile(path.join(remotePath, "owner.json"), `${JSON.stringify({
    ...owner({ host: "other-host" }), token: "remote-token",
  })}\n`);
  await mkdir(malformedPath);
  await writeFile(path.join(malformedPath, "owner.json"), "{}\n");
  assert.equal((await acquireMaestroIosLane({ lockPath: remotePath, ...owner() })).outcome, "ambiguous");
  assert.equal((await acquireMaestroIosLane({ lockPath: malformedPath, ...owner() })).outcome, "ambiguous");
});

test("explicit cleanup is token-bound to the exact lane", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-maestro-lane-cleanup-"));
  const lockPath = path.join(root, "lane.lock");
  t.after(() => rm(root, { recursive: true, force: true }));
  const acquired = await acquireMaestroIosLane({ lockPath, ...owner() });
  await assert.rejects(cleanupMaestroIosLane(lockPath, "wrong-token"), /token changed/u);
  const cleaned = await cleanupMaestroIosLane(lockPath, acquired.owner.token);
  assert.equal(cleaned.token, acquired.owner.token);
});
