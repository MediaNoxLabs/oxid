// SPDX-License-Identifier: Apache-2.0
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { verifyPiSubagentsPackage } from "./verify-pi-subagents-package.mjs";

const settings = JSON.parse(await readFile(".pi/settings.json", "utf8"));
if (!/^[a-z0-9-]+$/u.test(settings.defaultProvider ?? "")
  || !/^[a-z0-9.-]+$/u.test(settings.defaultModel ?? "")) {
  throw new Error("tracked defaultProvider/defaultModel is malformed");
}
if (settings.subagents?.defaultModel !== `${settings.defaultProvider}/${settings.defaultModel}`) {
  throw new Error("parent and subagent default models are not aligned");
}
const scope = settings.subagents?.modelScope;
if (scope?.enforce !== true || scope?.strict !== true
  || JSON.stringify(scope?.agents?.["dev-loop"]?.allow) !== JSON.stringify(["inherit"])) {
  throw new Error("dev-loop must be strictly restricted to the active supervisor model");
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
await verifyPiSubagentsPackage(subagentPackageRoot);

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
