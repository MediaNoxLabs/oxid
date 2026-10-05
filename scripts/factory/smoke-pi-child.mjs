#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { parseAgentFrontmatter, resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

function agentBody(source) {
  return source.replace(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/u, "").trim();
}

export async function runPiChildSmoke({
  cwd = process.cwd(), resolve = resolveDevLoopsPackageRoot, loadChildModule,
} = {}) {
  const resolved = await resolve({ cwd, includeAllPinnedPackages: true });
  const subagents = resolved.packageRoots.find(({ name }) => name === "pi-subagents");
  if (!subagents) throw new Error("project Pi settings do not pin pi-subagents");

  const developerPath = path.join(resolved.gitRoot, ".pi", "agents", "developer.agent.md");
  const developerSource = await readFile(developerPath, "utf8");
  const developer = parseAgentFrontmatter(developerSource, developerPath);
  if (developer.name !== "developer" || !developer.tools?.includes("read")) {
    throw new Error("tracked developer agent must be named developer and allow read for the child smoke");
  }

  const childSessionPath = path.join(subagents.packageRoot, "src", "runs", "shared", "child-session.js");
  let childModule;
  try {
    childModule = loadChildModule
      ? await loadChildModule(childSessionPath)
      : await import(pathToFileURL(childSessionPath).href);
  } catch (error) {
    throw new Error(
      `pi-subagents@${subagents.version} lacks its verified child-session runtime at ${childSessionPath}; rebuild the exact Pi package closure`,
      { cause: error },
    );
  }
  if (typeof childModule.createDefaultChildSessionFactory !== "function") {
    throw new Error(`pi-subagents@${subagents.version} child-session runtime lacks createDefaultChildSessionFactory`);
  }
  const factory = childModule.createDefaultChildSessionFactory({ shutdownTimeoutMs: 1_000 });
  const devLoopExtension = path.join(resolved.gitRoot, ".pi", "extensions", "dev-loop-preflight.ts");

  async function smoke({ label, extensionPaths = [], requiredExtensions = [] }) {
    const child = await factory.create({
      cwd: resolved.gitRoot,
      storage: { kind: "memory" },
      tools: ["read"],
      extensionPaths,
      requiredExtensions,
      ambientExtensions: false,
      hooks: [],
      noSkills: true,
      noContextFiles: true,
      systemPrompt: `${agentBody(developerSource)}\n\nOxid ${label} read-only startup smoke. Do not call a provider.`,
      runtime: {},
      processEnv: { PI_SUBAGENT_CHILD_AGENT: label },
    });
    if (!child.sessionId) throw new Error(`${label} child did not create a session`);
    await child.dispose();
  }

  let stopping = false;
  const stop = async (code) => {
    if (stopping) return;
    stopping = true;
    await factory.dispose();
    process.exitCode = code;
  };
  const terminate = () => { void stop(143); };
  const interrupt = () => { void stop(130); };
  process.once("SIGTERM", terminate);
  process.once("SIGINT", interrupt);
  try {
    await smoke({ label: "developer" });
    await smoke({
      label: "dev-loop",
      extensionPaths: [devLoopExtension],
      requiredExtensions: [{ id: "oxid-dev-loop-preflight", path: devLoopExtension }],
    });
    return { direct: "developer", devLoop: "developer", tools: ["read"] };
  } finally {
    process.removeListener("SIGTERM", terminate);
    process.removeListener("SIGINT", interrupt);
    await factory.dispose();
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    await runPiChildSmoke();
    process.stdout.write("Pi child smoke passed: direct developer and dev-loop local implementation sessions started without provider calls.\n");
  } catch (error) {
    process.stderr.write(`[smoke-pi-child] ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
