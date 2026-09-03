// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";

import { runManagedChild, signalProcessTree } from "../../scripts/lib/managed-child-process.mjs";

test("POSIX termination addresses the owned process group", () => {
  const calls = [];
  assert.equal(signalProcessTree(42, "SIGTERM", {
    platform: "darwin",
    kill: (...args) => calls.push(args),
  }), true);
  assert.deepEqual(calls, [[-42, "SIGTERM"]]);
});

test("a parent signal terminates the group and preserves signal exit status", async () => {
  const processRef = new EventEmitter();
  const child = new EventEmitter();
  child.pid = 314;
  const calls = [];
  const completion = runManagedChild("node", ["fixture"], {
    processRef,
    platform: "darwin",
    kill: (...args) => calls.push(args),
    spawnImpl: () => child,
    graceMs: 100,
  });
  processRef.emit("SIGTERM");
  child.emit("close", null, "SIGTERM");
  assert.equal(await completion, 143);
  assert.deepEqual(calls, [[-314, "SIGTERM"]]);
  assert.equal(processRef.listenerCount("SIGTERM"), 0);
  assert.equal(processRef.listenerCount("exit"), 0);
});

test("an unexpected child signal is a failure", async () => {
  const processRef = new EventEmitter();
  const child = new EventEmitter();
  child.pid = 271;
  const completion = runManagedChild("node", ["fixture"], {
    processRef,
    platform: "darwin",
    kill: () => {},
    spawnImpl: () => child,
    label: "fixture",
  });
  child.emit("close", null, "SIGKILL");
  await assert.rejects(completion, /fixture terminated by SIGKILL/);
});
