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

export async function verifyPiSubagentsPackage(root, { expectedVersion = "0.70.0" } = {}) {
  if (!root) throw new Error("pi-subagents package root is required");

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
    const verified = await verifyPiSubagentsPackage(root);
    process.stdout.write(`verified ${verified.name}@${verified.version} compiled capability surface\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
