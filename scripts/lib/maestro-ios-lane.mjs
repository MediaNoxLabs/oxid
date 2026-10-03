#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { randomUUID } from "node:crypto";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const OWNER_FILE = "owner.json";

const ownerPath = (lockPath) => path.join(lockPath, OWNER_FILE);

async function readOwner(lockPath) {
  try {
    const owner = JSON.parse(await readFile(ownerPath(lockPath), "utf8"));
    if (!Number.isSafeInteger(owner.pid) || owner.pid <= 0
      || typeof owner.host !== "string" || !owner.host
      || typeof owner.token !== "string" || !owner.token
      || typeof owner.startedAt !== "string" || !owner.startedAt
      || typeof owner.worktree !== "string" || !owner.worktree
      || typeof owner.flow !== "string" || !owner.flow) {
      return null;
    }
    return owner;
  } catch {
    return null;
  }
}

function processIsAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    return null;
  }
}

async function writeOwner(lockPath, owner) {
  await writeFile(ownerPath(lockPath), `${JSON.stringify(owner)}\n`, { mode: 0o600, flag: "wx" });
}

async function quarantineExactLock(lockPath, expectedToken) {
  const current = await readOwner(lockPath);
  if (!current || current.token !== expectedToken) return false;
  const quarantine = `${lockPath}.recovered-${process.pid}-${randomUUID()}`;
  try {
    await rename(lockPath, quarantine);
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
  await rm(quarantine, { recursive: true, force: false });
  return true;
}

export async function acquireMaestroIosLane({
  lockPath,
  pid = process.pid,
  host = os.hostname(),
  startedAt = new Date().toISOString(),
  worktree,
  flow,
}) {
  const candidate = { pid, host, startedAt, worktree, flow, token: randomUUID() };
  try {
    await mkdir(lockPath, { mode: 0o700 });
    await writeOwner(lockPath, candidate);
    return { outcome: "acquired", owner: candidate };
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
  }

  const owner = await readOwner(lockPath);
  if (!owner) return { outcome: "ambiguous", owner: null };
  if (owner.host !== host) return { outcome: "ambiguous", owner };
  const alive = processIsAlive(owner.pid);
  if (alive !== false) return { outcome: alive ? "waiting" : "ambiguous", owner };

  if (!await quarantineExactLock(lockPath, owner.token)) {
    return { outcome: "ambiguous", owner: await readOwner(lockPath) };
  }
  await mkdir(lockPath, { mode: 0o700 });
  await writeOwner(lockPath, candidate);
  return { outcome: "recovered", owner: candidate, recoveredOwner: owner };
}

export async function releaseMaestroIosLane(lockPath, token) {
  const owner = await readOwner(lockPath);
  if (!owner || owner.token !== token) return false;
  return quarantineExactLock(lockPath, token);
}

export async function cleanupMaestroIosLane(lockPath, token) {
  const owner = await readOwner(lockPath);
  if (!owner || owner.token !== token) {
    throw new Error("Maestro iOS lane owner token changed; refusing cleanup");
  }
  if (!await quarantineExactLock(lockPath, token)) {
    throw new Error("Maestro iOS lane changed during cleanup; refusing cleanup");
  }
  return owner;
}

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
}

async function main() {
  const command = process.argv[2];
  const lockPath = argument("--lock");
  if (!lockPath) throw new Error("--lock is required");
  if (command === "acquire") {
    const result = await acquireMaestroIosLane({
      lockPath,
      pid: Number(argument("--pid")),
      host: argument("--host"),
      startedAt: argument("--started-at"),
      worktree: argument("--worktree"),
      flow: argument("--flow"),
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
    if (!["acquired", "recovered"].includes(result.outcome)) process.exitCode = 75;
    return;
  }
  if (command === "release") {
    if (!await releaseMaestroIosLane(lockPath, argument("--token"))) process.exitCode = 1;
    return;
  }
  if (command === "cleanup") {
    process.stdout.write(`${JSON.stringify(await cleanupMaestroIosLane(lockPath, argument("--token")))}\n`);
    return;
  }
  throw new Error("expected acquire, release, or cleanup");
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
