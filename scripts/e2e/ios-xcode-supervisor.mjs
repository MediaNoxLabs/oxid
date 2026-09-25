// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { createWriteStream, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { runManagedChild } from "../lib/managed-child-process.mjs";

const SCENARIO = /^[a-z0-9][a-z0-9-]{0,79}$/u;
const RECEIPT_SCHEMA = "oxid-ios-xcode-admission-v1";
const DEFAULT_LEASE = path.join(
  os.tmpdir(),
  `oxid-ios-xcode-admission-v1-${typeof process.getuid === "function" ? process.getuid() : "user"}`,
);

function fail(message, code = 1) {
  process.stderr.write(`ios-xcode-supervisor: FAIL classification=${message}\n`);
  process.exitCode = code;
}

export function parseProcessSnapshot(text) {
  return String(text).split(/\r?\n/u).flatMap((line) => {
    const match = line.trim().match(/^(\d+)\s+(.+)$/u);
    if (!match) return [];
    const command = path.basename(match[2].trim().split(/\s+/u)[0] ?? "");
    return ["simctl", "testmanagerd", "xcodebuild", "xctest"].includes(command)
      ? [{ pid: Number(match[1]), command }]
      : [];
  });
}

export function processSnapshot() {
  return parseProcessSnapshot(execFileSync("/bin/ps", ["-axo", "pid=,comm="], { encoding: "utf8" }));
}

function processAlive(pid) {
  if (!Number.isSafeInteger(pid) || pid < 1) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

export function readLease(leaseDir) {
  const directory = lstatSync(leaseDir);
  if (!directory.isDirectory() || directory.isSymbolicLink() || (directory.mode & 0o777) !== 0o700) {
    throw new Error("invalid-lease-directory");
  }
  if (typeof process.getuid === "function" && directory.uid !== process.getuid()) throw new Error("invalid-lease-directory");
  const owner = path.join(leaseDir, "owner.json");
  const stat = lstatSync(owner);
  if (!stat.isFile() || stat.isSymbolicLink() || (stat.mode & 0o777) !== 0o600) throw new Error("invalid-lease-receipt");
  const value = JSON.parse(readFileSync(owner, "utf8"));
  if (value?.schema !== RECEIPT_SCHEMA || !Number.isSafeInteger(value.pid) || value.pid < 1 || !SCENARIO.test(value.scenario ?? "")) {
    throw new Error("invalid-lease-receipt");
  }
  return value;
}

export function reclaimStaleLease(leaseDir, expected, { afterClaim = () => {} } = {}) {
  const claim = path.join(leaseDir, ".reclaim.json");
  const token = `${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  try {
    writeFileSync(claim, `${token}\n`, { encoding: "utf8", mode: 0o600, flag: "wx" });
  } catch (error) {
    if (error?.code === "EEXIST" || error?.code === "ENOENT") return false;
    throw error;
  }
  let removed = false;
  try {
    afterClaim();
    const actual = readLease(leaseDir);
    if (actual.pid !== expected.pid || actual.startedAt !== expected.startedAt || processAlive(actual.pid)) return false;
    rmSync(leaseDir, { recursive: true });
    removed = true;
    return true;
  } finally {
    if (!removed) {
      try {
        const stat = lstatSync(claim);
        if (stat.isFile() && !stat.isSymbolicLink() && (stat.mode & 0o777) === 0o600
          && readFileSync(claim, "utf8") === `${token}\n`) {
          rmSync(claim);
        }
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
  }
}

export function acquireLease(leaseDir, scenario, { contenders = processSnapshot(), now = new Date().toISOString() } = {}) {
  if (!path.isAbsolute(leaseDir) || !SCENARIO.test(scenario)) throw new Error("invalid-admission-input");
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      mkdirSync(leaseDir, { mode: 0o700 });
      const foreign = contenders.filter(({ pid }) => pid !== process.pid);
      if (foreign.length > 0) {
        rmSync(leaseDir, { recursive: true });
        throw new Error(`external-contention-${foreign[0].command}`);
      }
      const receipt = { schema: RECEIPT_SCHEMA, pid: process.pid, scenario, startedAt: now };
      writeFileSync(path.join(leaseDir, "owner.json"), `${JSON.stringify(receipt)}\n`, { encoding: "utf8", mode: 0o600, flag: "wx" });
      return receipt;
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
      const existing = readLease(leaseDir);
      if (processAlive(existing.pid)) throw new Error(`lease-busy-${existing.scenario}`);
      if (!reclaimStaleLease(leaseDir, existing)) continue;
    }
  }
  throw new Error("lease-retry-exhausted");
}

export function releaseLease(leaseDir, receipt) {
  const current = readLease(leaseDir);
  if (current.pid !== receipt.pid || current.startedAt !== receipt.startedAt || current.scenario !== receipt.scenario) {
    throw new Error("lease-owner-changed");
  }
  rmSync(leaseDir, { recursive: true });
}

function parseArgs(argv) {
  const split = argv.indexOf("--");
  if (split < 0 || split === argv.length - 1) throw new Error("missing-command");
  const rawFlags = argv.slice(0, split);
  const childOnlyCount = rawFlags.filter((value) => value === "--child-only").length;
  if (childOnlyCount > 1) throw new Error("duplicate-child-only");
  const childOnly = childOnlyCount === 1;
  const flags = rawFlags.filter((value) => value !== "--child-only");
  const command = argv.slice(split + 1);
  const read = (name) => {
    const index = flags.indexOf(name);
    if (index < 0 || index === flags.length - 1 || flags.indexOf(name, index + 1) >= 0) throw new Error(`missing-${name.slice(2)}`);
    return flags[index + 1];
  };
  const scenario = read("--scenario");
  const timeoutSeconds = Number(read("--timeout-seconds"));
  const cwd = read("--cwd");
  if (!SCENARIO.test(scenario) || !Number.isFinite(timeoutSeconds) || timeoutSeconds <= 0 || timeoutSeconds > 7200 || !path.isAbsolute(cwd)) {
    throw new Error("invalid-arguments");
  }
  return { childOnly, scenario, timeoutMs: Math.ceil(timeoutSeconds * 1000), cwd, command: command[0], args: command.slice(1) };
}

export async function supervise(argv, {
  leaseDir = process.env.OXID_IOS_XCODE_LEASE_DIR || DEFAULT_LEASE,
  contenders,
} = {}) {
  const options = parseArgs(argv);
  if (options.childOnly && process.env.OXID_IOS_XCODE_SUPERVISED !== "1") throw new Error("child-without-host-admission");
  const receipt = options.childOnly ? null : acquireLease(leaseDir, options.scenario, { contenders });
  let privateDir;
  let logPath;
  let log;
  let logEnded = false;
  let primaryError;
  let releaseError;
  let resultCode = 1;
  const endLog = async () => {
    if (!log || logEnded) return;
    logEnded = true;
    await new Promise((resolve) => log.end(resolve));
  };
  try {
    privateDir = mkdtempSync(path.join(os.tmpdir(), "oxid-ios-xcode-run."));
    logPath = path.join(privateDir, "child.log");
    log = createWriteStream(logPath, { flags: "wx", mode: 0o600 });
    const completion = runManagedChild(options.command, options.args, {
      cwd: options.cwd,
      env: { ...process.env, OXID_IOS_XCODE_SUPERVISED: "1" },
      stdout: log,
      stderr: log,
      label: options.scenario,
      graceMs: 30_000,
      timeoutMs: options.timeoutMs,
    });
    process.stderr.write(`ios-xcode-supervisor: phase=admitted scenario=${options.scenario}\n`);
    const code = await completion;
    await endLog();
    if (code === 0) {
      rmSync(privateDir, { recursive: true });
      process.stderr.write(`ios-xcode-supervisor: phase=complete scenario=${options.scenario} outcome=passed\n`);
    } else {
      process.stderr.write(`ios-xcode-supervisor: phase=complete scenario=${options.scenario} outcome=${code === 124 ? "timed-out" : "failed"} log=${logPath}\n`);
    }
    resultCode = code;
  } catch (error) {
    primaryError = error;
  } finally {
    await endLog();
    if (receipt) {
      try {
        releaseLease(leaseDir, receipt);
      } catch (error) {
        releaseError = error;
      }
    }
  }
  if (releaseError) process.stderr.write(`ios-xcode-supervisor: phase=release outcome=failed classification=${releaseError.message}\n`);
  if (primaryError) throw primaryError;
  if (releaseError && resultCode === 0) throw releaseError;
  return resultCode;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  supervise(process.argv.slice(2)).then((code) => { process.exitCode = code; }, (error) => fail(error.message, /contention|busy/u.test(error.message) ? 75 : 1));
}
