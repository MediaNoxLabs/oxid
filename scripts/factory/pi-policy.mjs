#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { randomUUID } from "node:crypto";
import { copyFile, chmod, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
export const TRACKED_POLICY_PATH = path.join(repoRoot, ".pi", "subagent-policy.json");

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function mergePolicy(current, policy) {
  const result = isObject(current) ? { ...current } : {};
  for (const [key, value] of Object.entries(policy)) {
    result[key] = isObject(value) ? mergePolicy(result[key], value) : value;
  }
  return result;
}

export function policyMismatches(current, policy, prefix = "") {
  const mismatches = [];
  for (const [key, expected] of Object.entries(policy)) {
    const field = prefix ? `${prefix}.${key}` : key;
    const actual = isObject(current) ? current[key] : undefined;
    if (isObject(expected)) {
      mismatches.push(...policyMismatches(actual, expected, field));
    } else if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      mismatches.push({ field, expected, actual: actual === undefined ? null : actual });
    }
  }
  return mismatches;
}

function resolveTilde(value) {
  if (value === "~") return os.homedir();
  if (value?.startsWith("~/")) return path.join(os.homedir(), value.slice(2));
  return value;
}

export function resolveUserSubagentConfigPath(env = process.env) {
  const configured = resolveTilde(env.PI_CODING_AGENT_DIR);
  const agentDir = configured ? path.resolve(configured) : path.join(os.homedir(), ".pi", "agent");
  return path.join(agentDir, "extensions", "subagent", "config.json");
}

async function readJson(filePath, { missing = undefined } = {}) {
  try {
    const parsed = JSON.parse(await readFile(filePath, "utf8"));
    if (!isObject(parsed)) throw new Error("expected a JSON object");
    return parsed;
  } catch (error) {
    if (error?.code === "ENOENT") return missing;
    throw new Error(`Cannot read ${filePath}: ${error.message}`);
  }
}

export async function loadTrackedPolicy() {
  return readJson(TRACKED_POLICY_PATH);
}

export async function checkUserPolicy({ env = process.env } = {}) {
  const policy = await loadTrackedPolicy();
  const configPath = resolveUserSubagentConfigPath(env);
  const current = await readJson(configPath, { missing: {} });
  const mismatches = policyMismatches(current, policy);
  return { ok: mismatches.length === 0, configPath, mismatches };
}

export async function applyUserPolicy({ env = process.env, execute = false } = {}) {
  if (!execute) throw new Error("Refusing to modify user Pi configuration without --execute");
  const policy = await loadTrackedPolicy();
  const configPath = resolveUserSubagentConfigPath(env);
  const current = await readJson(configPath, { missing: {} });
  const next = mergePolicy(current, policy);
  if (policyMismatches(current, policy).length === 0) {
    return { changed: false, configPath, backupPath: null };
  }

  const parent = path.dirname(configPath);
  await mkdir(parent, { recursive: true, mode: 0o700 });
  let backupPath = null;
  if (Object.keys(current).length > 0) {
    backupPath = `${configPath}.backup`;
    try {
      await copyFile(configPath, backupPath, fsConstants.COPYFILE_EXCL);
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
    }
    await chmod(backupPath, 0o600);
  }
  const temporary = path.join(parent, `.config.json.${process.pid}.${randomUUID()}.tmp`);
  try {
    await writeFile(temporary, `${JSON.stringify(next, null, 2)}\n`, { mode: 0o600, flag: "wx" });
    await rename(temporary, configPath);
    await chmod(configPath, 0o600);
  } finally {
    await rm(temporary, { force: true });
  }
  return { changed: true, configPath, backupPath };
}

function usage() {
  return [
    "Usage:",
    "  node scripts/factory/pi-policy.mjs check [--json]",
    "  node scripts/factory/pi-policy.mjs apply --execute [--json]",
    "",
    "The apply command changes only ~/.pi/agent/extensions/subagent/config.json",
    "(or the PI_CODING_AGENT_DIR equivalent), preserves unknown keys, and never",
    "reads or writes auth.json.",
  ].join("\n");
}

async function main(argv = process.argv.slice(2)) {
  const command = argv[0];
  const json = argv.includes("--json");
  if (command === "check") {
    const result = await checkUserPolicy();
    if (json) process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    else if (result.ok) process.stdout.write(`Pi subagent policy is aligned: ${result.configPath}\n`);
    else process.stderr.write(`Pi subagent policy is not aligned (${result.mismatches.map((item) => item.field).join(", ")}).\nRun ./bootstrap.sh --configure-pi\n`);
    return result.ok ? 0 : 1;
  }
  if (command === "apply") {
    const result = await applyUserPolicy({ execute: argv.includes("--execute") });
    if (json) process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    else process.stdout.write(result.changed
      ? `Applied bounded Pi subagent policy to ${result.configPath}${result.backupPath ? ` (backup: ${result.backupPath})` : ""}.\n`
      : `Pi subagent policy is already aligned: ${result.configPath}\n`);
    return 0;
  }
  process.stderr.write(`${usage()}\n`);
  return command === "--help" || command === "-h" ? 0 : 2;
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  main().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[pi-policy] ${error.message}\n`);
    process.exitCode = 1;
  });
}
