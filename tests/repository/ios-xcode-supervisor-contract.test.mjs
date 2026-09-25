// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  acquireLease,
  parseProcessSnapshot,
  readLease,
  releaseLease,
} from "../../scripts/e2e/ios-xcode-supervisor.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const supervisor = path.join(root, "scripts/e2e/ios-xcode-supervisor.mjs");
const deterministicDriver = path.join(root, "tests/fixtures/ios-xcode-supervisor-driver.mjs");

function fixture() {
  const directory = mkdtempSync(path.join(os.tmpdir(), "oxid-ios-xcode-test."));
  return { directory, lease: path.join(directory, "lease") };
}

test("process contention recognizes active Xcode and test owners but ignores a persistent Simulator UI", () => {
  assert.deepEqual(parseProcessSnapshot(" 12 /usr/bin/xcodebuild\n13 /usr/bin/simctl\n14 /bin/sleep\n15 /Applications/Xcode.app/Simulator\n16 /usr/bin/xctest\n"), [
    { pid: 12, command: "xcodebuild" },
    { pid: 13, command: "simctl" },
    { pid: 16, command: "xctest" },
  ]);
});

test("host lease reports a live owner and releases only the exact receipt", () => {
  const { directory, lease } = fixture();
  try {
    const receipt = acquireLease(lease, "profile-flow", { contenders: [], now: "2026-09-25T00:00:00.000Z" });
    assert.deepEqual(readLease(lease), receipt);
    assert.throws(() => acquireLease(lease, "portal-flow", { contenders: [] }), /lease-busy-profile-flow/u);
    assert.throws(() => releaseLease(lease, { ...receipt, scenario: "foreign" }), /lease-owner-changed/u);
    releaseLease(lease, receipt);
    assert.throws(() => readLease(lease), /ENOENT/u);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a valid dead-owner receipt is reclaimed but external Xcode contention is not", () => {
  const { directory, lease } = fixture();
  try {
    mkdirSync(lease, { mode: 0o700 });
    writeFileSync(path.join(lease, "owner.json"), `${JSON.stringify({
      schema: "oxid-ios-xcode-admission-v1",
      pid: 2_000_000_000,
      scenario: "stale-run",
      startedAt: "2026-09-24T00:00:00.000Z",
    })}\n`, { mode: 0o600 });
    const receipt = acquireLease(lease, "recovered-run", { contenders: [] });
    assert.equal(receipt.scenario, "recovered-run");
    releaseLease(lease, receipt);
    assert.throws(() => acquireLease(lease, "blocked-run", {
      contenders: [{ pid: 42, command: "xcodebuild" }],
    }), /external-contention-xcodebuild/u);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("CLI timeout is compact, bounded, group-owned, and releases admission", () => {
  const { directory, lease } = fixture();
  const started = Date.now();
  try {
    assert.throws(() => execFileSync(process.execPath, [
      deterministicDriver,
      "--scenario", "timeout-fixture",
      "--timeout-seconds", "0.1",
      "--cwd", root,
      "--", process.execPath, "-e", "setInterval(() => {}, 1000)",
    ], {
      encoding: "utf8",
      env: { ...process.env, OXID_IOS_XCODE_LEASE_DIR: lease },
      stdio: ["ignore", "pipe", "pipe"],
    }), (error) => {
      assert.equal(error.status, 124);
      assert.match(error.stderr, /phase=admitted scenario=timeout-fixture/u);
      assert.match(error.stderr, /outcome=timed-out log=.*child\.log/u);
      assert.doesNotMatch(error.stderr, /setInterval/u);
      return true;
    });
    assert.ok(Date.now() - started < 5_000);
    assert.throws(() => readLease(lease), /ENOENT/u);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("child scenarios require an admitted host owner", () => {
  assert.throws(() => execFileSync(process.execPath, [
    supervisor,
    "--child-only",
    "--scenario", "unowned-child",
    "--timeout-seconds", "1",
    "--cwd", root,
    "--", process.execPath, "-e", "process.exit(0)",
  ], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }), (error) => {
    assert.equal(error.status, 1);
    assert.match(error.stderr, /child-without-host-admission/u);
    return true;
  });
});

test("parent interruption terminates descendants and removes the lease", async () => {
  const { directory, lease } = fixture();
  try {
    const child = spawn(process.execPath, [
      deterministicDriver,
      "--scenario", "interrupt-fixture",
      "--timeout-seconds", "30",
      "--cwd", root,
      "--", process.execPath, "-e", "setInterval(() => {}, 1000)",
    ], {
      env: { ...process.env, OXID_IOS_XCODE_LEASE_DIR: lease },
      stdio: ["ignore", "ignore", "pipe"],
    });
    let stderr = "";
    let interrupted = false;
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (!interrupted && stderr.includes("phase=admitted")) {
        interrupted = true;
        child.kill("SIGTERM");
      }
    });
    const exit = await new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("close", (code) => resolve(code));
    });
    assert.equal(exit, 143);
    assert.match(stderr, /outcome=failed/u);
    assert.throws(() => readLease(lease), /ENOENT/u);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
