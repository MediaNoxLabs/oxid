#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { constants as fsConstants } from "node:fs";
import { mkdir, open, rename, unlink, lstat } from "node:fs/promises";
import path from "node:path";

export const GIB = 1024 ** 3;
export const SWAP_LIMIT_BYTES = 20 * GIB;
export const AVAILABLE_MEMORY_FLOOR_BYTES = 2 * GIB;
const MAX_TREND_WINDOW_MS = 120_000;

function blocked(reasonCode, summary) { return { decision: "block", reasonCode, summary }; }

/** Pure decision function: collection and process inventory stay at the CLI boundary. */
export function evaluateResourceAdmission(sample) {
  const summary = {
    swapUsedBytes: null, swapTrendBytes: null, availableBytes: sample?.availableBytes ?? null,
    memoryPressure: sample?.memoryPressure ?? null, diskAvailableBytes: sample?.diskAvailableBytes ?? null,
    recentUnresolvedAlert: sample?.recentUnresolvedAlert ?? null,
    workloads: Array.isArray(sample?.workloads) ? sample.workloads.slice(0, 32).map(({ ownership, label }) => ({ ownership, label })) : [],
  };
  if (!sample || !Array.isArray(sample.samples) || sample.samples.length !== 2
    || !sample.samples.every((item) => Number.isFinite(item?.atMs) && Number.isFinite(item?.swapUsedBytes) && item.swapUsedBytes >= 0)) {
    return blocked("telemetry-missing", summary);
  }
  const [first, last] = sample.samples;
  const windowMs = last.atMs - first.atMs;
  if (sample.contradictory || windowMs < 0 || windowMs > MAX_TREND_WINDOW_MS
    || !Number.isFinite(sample.availableBytes) || !Number.isFinite(sample.diskAvailableBytes)
    || !Number.isFinite(sample.diskFloorBytes) || !["healthy", "unhealthy"].includes(sample.memoryPressure)
    || typeof sample.recentUnresolvedAlert !== "boolean") return blocked("telemetry-contradictory", summary);
  const trend = last.swapUsedBytes - first.swapUsedBytes;
  summary.swapUsedBytes = last.swapUsedBytes;
  summary.swapTrendBytes = trend;
  if (sample.recentUnresolvedAlert) return blocked("monitor-alert-unresolved", summary);
  if (sample.availableBytes <= AVAILABLE_MEMORY_FLOOR_BYTES) return blocked("available-memory-low", summary);
  if (sample.memoryPressure !== "healthy") return blocked("memory-pressure-unhealthy", summary);
  if (sample.diskAvailableBytes <= sample.diskFloorBytes) return blocked("disk-headroom-low", summary);
  if (trend >= GIB) return blocked("swap-growing", summary);
  if (last.swapUsedBytes > SWAP_LIMIT_BYTES) {
    if (!["headless", "factory"].includes(sample.requestedLane)) return blocked("degraded-lane-disallowed", summary);
    if (!Number.isInteger(sample.activeHeavyLanes) || sample.activeHeavyLanes !== 0) return blocked("degraded-concurrency-disallowed", summary);
    return { decision: "degraded", reasonCode: "historical-swap-stable", summary };
  }
  return { decision: "allow", reasonCode: "healthy", summary };
}

function command(commandName, args, options = {}) {
  return execFileSync(commandName, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 10_000, ...options }).trim();
}

function parseSwap(value) {
  const match = value.match(/used = ([0-9.]+)([MG])(?:|B)/u);
  if (!match) throw new Error("could not parse sysctl vm.swapusage");
  return Number(match[1]) * (match[2] === "G" ? GIB : 1024 ** 2);
}

function parseAvailable(value) {
  const pages = value.match(/Pages free:\s+(\d+)\./u);
  if (!pages) throw new Error("could not parse vm_stat free pages");
  return Number(pages[1]) * 4096;
}

function parseDisk(value) {
  const fields = value.trim().split(/\s+/u);
  const availableKib = Number(fields.at(-3));
  if (!Number.isFinite(availableKib)) throw new Error("could not parse df availability");
  return availableKib * 1024;
}

function alertIsRecentUnresolved(log, nowMs) {
  const lines = log.trim().split(/\r?\n/u).filter(Boolean).slice(-64);
  return lines.some((line) => /(?:unresolved|alert)/iu.test(line) && !/resolved/iu.test(line)
    && (!line.match(/\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d/u) || nowMs - Date.parse(line.match(/\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z?/u)[0]) <= MAX_TREND_WINDOW_MS));
}

export function collectMacOSSample({ run = command, now = () => Date.now(), requestedLane = "factory", activeHeavyLanes = 0 } = {}) {
  if (process.platform !== "darwin") return null;
  try {
    const atMs = now();
    const swap = parseSwap(run("sysctl", ["vm.swapusage"]));
    const memoryPressure = /System-wide memory free percentage:\s+(?:[5-9]\d|100)%/u.test(run("memory_pressure", ["-Q"])) ? "healthy" : "unhealthy";
    const availableBytes = parseAvailable(run("vm_stat", []));
    const diskAvailableBytes = parseDisk(run("df", ["-k", "/"]));
    let alerts = "";
    try { alerts = run("tail", ["-n", "64", path.join(process.env.HOME ?? "", ".claude-resmon", "alerts.log")]); } catch {}
    return { samples: [{ atMs, swapUsedBytes: swap }, { atMs, swapUsedBytes: swap }], availableBytes, memoryPressure,
      diskAvailableBytes, diskFloorBytes: 20 * GIB, recentUnresolvedAlert: alertIsRecentUnresolved(alerts, atMs), requestedLane, activeHeavyLanes, workloads: [] };
  } catch { return null; }
}

export async function defaultResourceAdmissionDirectory(cwd = process.cwd()) {
  const common = command("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd });
  return path.join(common, "oxid-factory", "resource-admission-v1");
}

export async function writeResourceAdmissionReceipt(result, { outputDir, issue = null, headSha = null, run = null } = {}) {
  if (!result || !["allow", "degraded", "block"].includes(result.decision)) throw new Error("invalid resource admission result");
  await mkdir(outputDir, { recursive: true, mode: 0o700 });
  const info = await lstat(outputDir);
  if (!info.isDirectory() || info.isSymbolicLink() || (info.mode & 0o077) !== 0) throw new Error("resource admission output must be a private real directory");
  const safe = (value, fallback) => typeof value === "string" && /^[A-Za-z0-9._-]{1,80}$/u.test(value) ? value : fallback;
  const name = `issue-${Number.isInteger(issue) ? issue : "unknown"}-${safe(headSha, "nohead")}-${safe(run, randomUUID())}.json`;
  const destination = path.join(outputDir, name);
  try { await lstat(destination); throw new Error("resource admission receipt already exists"); } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const temporary = `${destination}.${randomUUID()}.tmp`;
  let handle;
  try {
    handle = await open(temporary, "wx", 0o600);
    await handle.writeFile(`${JSON.stringify({ schemaVersion: 1, recordedAt: new Date().toISOString(), issue, headSha, run, ...result }, null, 2)}\n`);
    await handle.sync(); await handle.close(); handle = null;
    await rename(temporary, destination);
  } catch (error) { await handle?.close().catch(() => {}); await unlink(temporary).catch(() => {}); throw error; }
  return destination;
}

async function main(argv = process.argv.slice(2)) {
  if (argv.includes("--help")) { process.stdout.write("Usage: node scripts/factory/resource-admission.mjs [--lane headless|factory] [--issue N] [--head SHA] [--run ID]\n"); return 0; }
  const value = (name, fallback = undefined) => { const index = argv.indexOf(name); return index < 0 ? fallback : argv[index + 1]; };
  const lane = value("--lane", "factory");
  const sample = collectMacOSSample({ requestedLane: lane });
  const result = evaluateResourceAdmission(sample);
  const outputDir = await defaultResourceAdmissionDirectory();
  const receipt = await writeResourceAdmissionReceipt(result, { outputDir, issue: Number(value("--issue")) || null, headSha: value("--head", null), run: value("--run", null) });
  process.stdout.write(`${JSON.stringify({ ...result, receipt })}\n`);
  return result.decision === "block" ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === new URL(import.meta.url).pathname) main().then((code) => { process.exitCode = code; }).catch((error) => { process.stderr.write(`[resource-admission] ${error.message}\n`); process.exitCode = 1; });
