// SPDX-License-Identifier: Apache-2.0
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

const settings = JSON.parse(await readFile(".pi/settings.json", "utf8"));
if (!/^[a-z0-9-]+$/u.test(settings.defaultProvider ?? "")
  || !/^[a-z0-9.-]+$/u.test(settings.defaultModel ?? "")) {
  throw new Error("tracked defaultProvider/defaultModel is malformed");
}
if (settings.subagents?.defaultModel !== `${settings.defaultProvider}/${settings.defaultModel}`) {
  throw new Error("parent and subagent default models are not aligned");
}

const resolved = await resolveDevLoopsPackageRoot({
  cwd: process.cwd(),
  includeAllPinnedPackages: true,
});
const packageRoot = (expectedName) => {
  const candidate = resolved.packageRoots.find(({ name }) => name === expectedName);
  if (!candidate) throw new Error(`project Pi settings do not pin ${expectedName}`);
  return candidate.packageRoot;
};

const subagentPackageRoot = packageRoot("pi-subagents");
const reviewPackageRoot = packageRoot("@input-output-hk/agent-review-pi");
const [subagentManifestSource, types, agents, toolBudget, waitTool, waitRuntime, foregroundSettlement] = await Promise.all([
  readFile(path.join(subagentPackageRoot, "package.json"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "shared", "types.ts"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "agents", "agents.ts"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "runs", "shared", "tool-budget.ts"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "runs", "background", "wait-tool.ts"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "runs", "background", "subagent-wait.ts"), "utf8"),
  readFile(path.join(subagentPackageRoot, "src", "runs", "foreground", "workflow-detach-reconcile.ts"), "utf8"),
]);
const subagentManifest = JSON.parse(subagentManifestSource);
if (subagentManifest.name !== "pi-subagents" || subagentManifest.version !== "0.67.0") {
  throw new Error(`unexpected pi-subagents package ${subagentManifest.name}@${subagentManifest.version}`);
}
for (const [source, capability] of [
  [waitTool, "remembered detached foreground descendant"],
  [waitRuntime, "attentionRunsForSession"],
  [waitRuntime, "stopOnAttention"],
  [foregroundSettlement, "reconcileDetachedWorkflowChildCompletion"],
  [foregroundSettlement, "planWorkflowSettlement"],
]) {
  if (!source.includes(capability)) throw new Error(`pi-subagents lacks ${capability}`);
}
for (const field of [
  "asyncByDefault", "forceTopLevelAsync", "maxSubagentDepth",
  "maxSubagentSpawnsPerSession", "maxSubagentSpawnsPerRun",
  "glo" + "balConcurrencyLimit", "toolBudget", "usageBudget", "parallel",
  "chain", "dynamicFanout", "maxItems", "artifactDir",
]) {
  if (!types.includes(`${field}?`)) throw new Error(`pi-subagents schema does not declare ${field}`);
}
for (const field of ["frontmatter.timeoutMs", "frontmatter.toolBudget", "frontmatter.maxSubagentDepth"]) {
  if (!agents.includes(field)) throw new Error(`pi-subagents agent parser does not consume ${field}`);
}
for (const field of ["soft", "hard", "block"]) {
  if (!toolBudget.includes(field)) throw new Error(`pi-subagents tool budget does not consume ${field}`);
}

const reviewManifest = JSON.parse(await readFile(path.join(reviewPackageRoot, "package.json"), "utf8"));
const expectedReview = {
  name: "@input-output-hk/agent-review-pi",
  version: "0.6.0",
  extension: "./dist/extension.js",
  skill: "./skills",
};
if (reviewManifest.name !== expectedReview.name || reviewManifest.version !== expectedReview.version) {
  throw new Error(`unexpected review package ${reviewManifest.name}@${reviewManifest.version}`);
}
if (!reviewManifest.pi?.extensions?.includes(expectedReview.extension)) {
  throw new Error(`review package does not declare ${expectedReview.extension}`);
}
if (!reviewManifest.pi?.skills?.includes(expectedReview.skill)) {
  throw new Error(`review package does not declare ${expectedReview.skill}`);
}

const extensionPath = path.join(reviewPackageRoot, "dist", "extension.js");
const extension = await import(pathToFileURL(extensionPath).href);
const registered = [];
extension.registerTools({ registerTool(tool) { registered.push(tool.name); } });
const expectedTools = [
  "labels_bootstrap", "pr_approve_dep_upgrade", "pr_create_followup",
  "pr_expedite", "pr_request_review", "pr_self_review", "pr_stabilize",
  "pr_watch", "review_claim", "review_complete", "review_create",
  "review_enrich", "review_list",
].sort();
registered.sort();
if (JSON.stringify(registered) !== JSON.stringify(expectedTools)) {
  throw new Error(`unexpected review tools: ${registered.join(",")}`);
}

process.stdout.write(JSON.stringify({
  expectedProvider: settings.defaultProvider,
  expectedModel: settings.defaultModel,
  reviewPackageRoot,
}));
