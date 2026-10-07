#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_SOURCES = Object.freeze({
  manifest: "package.json",
  types: "src/shared/types.d.ts",
  agents: "src/agents/agents.js",
  toolBudget: "src/runs/shared/tool-budget.js",
  waitTool: "src/runs/background/wait-tool.js",
  waitRuntime: "src/runs/background/subagent-wait.js",
  autoDrain: "src/runs/background/auto-drain.js",
  foregroundSettlement: "src/runs/foreground/workflow-detach-reconcile.js",
  childSession: "src/runs/shared/child-session.js",
});

async function readRequiredSource(root, relativePath) {
  try {
    return await readFile(path.join(root, relativePath), "utf8");
  } catch (error) {
    throw new Error(`pi-subagents is missing required compiled artifact ${relativePath}`, { cause: error });
  }
}

function requireCapability(source, capability, relativePath) {
  if (!source.includes(capability)) {
    throw new Error(`pi-subagents compiled artifact ${relativePath} lacks required capability ${capability}`);
  }
}

const PI_SDK_PEER_PREFIX = "@earendil-works/pi-";
const EXACT_VERSION = /^\d+\.\d+\.\d+$/u;

function compareVersions(left, right) {
  const a = left.split(".").map(Number);
  const b = right.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return 0;
}

function satisfiesPeerRange(version, range) {
  if (range === "*") return true;
  return range.split("||").some((alternative) => {
    const comparisons = alternative.trim().split(/\s+/u);
    return comparisons.length > 0 && comparisons.every((comparison) => {
      const match = comparison.match(/^(>=|<=|>|<|=)?(\d+\.\d+\.\d+)$/u);
      if (!match) return false;
      const order = compareVersions(version, match[2]);
      return match[1] === ">=" ? order >= 0
        : match[1] === "<=" ? order <= 0
          : match[1] === ">" ? order > 0
            : match[1] === "<" ? order < 0
              : order === 0;
    });
  });
}

function exactProjectPins(settings) {
  const pins = new Map();
  for (const entry of settings?.packages ?? []) {
    const source = typeof entry === "string" ? entry : entry?.source;
    if (typeof source !== "string" || !source.startsWith("npm:")) continue;
    const match = source.match(/^npm:(@[^/]+\/[^@]+|[^@]+)@(.+)$/u);
    if (!match) continue;
    if (pins.has(match[1])) throw new Error(`project Pi settings contain duplicate package pin ${match[1]}`);
    pins.set(match[1], match[2]);
  }
  return pins;
}

export function verifyPiSdkPeerPins(manifest, projectSettings, { expectedPiSdkVersion = "0.85.1" } = {}) {
  if (!EXACT_VERSION.test(expectedPiSdkVersion)) {
    throw new Error(`expected Pi SDK version must be exact; found ${expectedPiSdkVersion}`);
  }
  const peers = Object.entries(manifest?.peerDependencies ?? {})
    .filter(([name]) => name.startsWith(PI_SDK_PEER_PREFIX))
    .sort(([left], [right]) => left.localeCompare(right));
  if (peers.length === 0) throw new Error("pi-subagents manifest declares no Pi SDK peer dependencies");

  const pins = exactProjectPins(projectSettings);
  const failures = [];
  for (const [name, range] of peers) {
    const pin = pins.get(name);
    if (pin === undefined) failures.push(`missing ${name}@${expectedPiSdkVersion}`);
    else if (!EXACT_VERSION.test(pin)) failures.push(`non-exact ${name}@${pin}`);
    else if (pin !== expectedPiSdkVersion) failures.push(`mismatched ${name}@${pin}; expected ${expectedPiSdkVersion}`);
    else if (typeof range !== "string" || !satisfiesPeerRange(pin, range)) {
      failures.push(`incompatible ${name} peer range ${String(range)} for ${pin}`);
    }
  }
  if (failures.length > 0) throw new Error(`Pi SDK peer policy failed: ${failures.join("; ")}`);
}

export async function verifyPiSubagentsPackage(root, {
  expectedVersion = "0.70.0", expectedPiSdkVersion = "0.85.1", projectSettings,
} = {}) {
  if (!root) throw new Error("pi-subagents package root is required");
  if (!projectSettings) throw new Error("project Pi settings are required");

  const entries = await Promise.all(Object.entries(REQUIRED_SOURCES).map(async ([key, relativePath]) => [
    key,
    await readRequiredSource(root, relativePath),
  ]));
  const sources = Object.fromEntries(entries);
  const manifest = JSON.parse(sources.manifest);
  if (manifest.name !== "pi-subagents" || manifest.version !== expectedVersion) {
    throw new Error(
      `unexpected pi-subagents package ${manifest.name ?? "<missing>"}@${manifest.version ?? "<missing>"}; expected pi-subagents@${expectedVersion}`,
    );
  }
  verifyPiSdkPeerPins(manifest, projectSettings, { expectedPiSdkVersion });

  for (const [sourceKey, capability] of [
    ["waitTool", "remembered detached foreground descendant"],
    ["waitRuntime", "attentionRunsForSession"],
    ["waitRuntime", "stopOnAttention"],
    ["autoDrain", "hasPendingSupervisorRequest"],
    ["foregroundSettlement", "reconcileDetachedWorkflowChildCompletion"],
    ["foregroundSettlement", "planWorkflowSettlement"],
  ]) {
    requireCapability(sources[sourceKey], capability, REQUIRED_SOURCES[sourceKey]);
  }

  for (const field of [
    "asyncByDefault", "forceTopLevelAsync", "maxSubagentDepth",
    "maxSubagentSpawnsPerSession", "maxSubagentSpawnsPerRun",
    "globalConcurrencyLimit", "toolBudget", "usageBudget", "parallel",
    "chain", "dynamicFanout", "maxItems", "artifactDir",
  ]) {
    requireCapability(sources.types, `${field}?`, REQUIRED_SOURCES.types);
  }
  for (const field of ["frontmatter.timeoutMs", "frontmatter.toolBudget", "frontmatter.maxSubagentDepth"]) {
    requireCapability(sources.agents, field, REQUIRED_SOURCES.agents);
  }
  for (const field of ["soft", "hard", "block"]) {
    requireCapability(sources.toolBudget, field, REQUIRED_SOURCES.toolBudget);
  }
  requireCapability(sources.childSession, "createDefaultChildSessionFactory", REQUIRED_SOURCES.childSession);

  return { name: manifest.name, version: manifest.version };
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const root = process.argv[2];
  try {
    const settingsPath = process.argv[3];
    const expectedPiSdkVersion = process.argv[4];
    if (!settingsPath) throw new Error("project Pi settings path is required");
    if (!expectedPiSdkVersion) throw new Error("Nix-pinned Pi SDK version is required");
    const projectSettings = JSON.parse(await readFile(settingsPath, "utf8"));
    const verified = await verifyPiSubagentsPackage(root, { projectSettings, expectedPiSdkVersion });
    process.stdout.write(`verified ${verified.name}@${verified.version} compiled capability surface\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
