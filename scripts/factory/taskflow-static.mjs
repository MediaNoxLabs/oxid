#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createRequire } from "node:module";
import { readFile, realpath } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

const EXPECTED_TASKFLOW_VERSION = "0.2.10";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function usage() {
  process.stderr.write("Usage: node scripts/factory/taskflow-static.mjs <verify|plan|compile> <flow.json> [args-json]\n");
}

function contained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

async function loadPinnedCore() {
  const resolved = await resolveDevLoopsPackageRoot({ cwd: root, includeAllPinnedPackages: true });
  const piTaskflow = resolved.packageRoots.find(({ name }) => name === "pi-taskflow");
  if (!piTaskflow || piTaskflow.version !== EXPECTED_TASKFLOW_VERSION) {
    throw new Error(`expected installed pi-taskflow@${EXPECTED_TASKFLOW_VERSION}`);
  }
  const require = createRequire(path.join(piTaskflow.packageRoot, "package.json"));
  const entry = require.resolve("taskflow-core");
  const coreRoot = path.resolve(path.dirname(entry), "..");
  const manifest = JSON.parse(await readFile(path.join(coreRoot, "package.json"), "utf8"));
  if (manifest.name !== "taskflow-core" || manifest.version !== EXPECTED_TASKFLOW_VERSION) {
    throw new Error(`expected taskflow-core@${EXPECTED_TASKFLOW_VERSION}, found ${manifest.name}@${manifest.version}`);
  }
  return import(pathToFileURL(entry).href);
}

async function loadFlow(file) {
  const canonical = await realpath(path.resolve(root, file));
  if (!contained(root, canonical)) throw new Error("flow definition escapes the repository");
  if (!canonical.endsWith(".json")) throw new Error("flow definition must be JSON");
  return JSON.parse(await readFile(canonical, "utf8"));
}

function resolveArgs(flow, supplied) {
  const resolved = { ...supplied };
  for (const [name, spec] of Object.entries(flow.args ?? {})) {
    if (resolved[name] === undefined && Object.hasOwn(spec, "default")) resolved[name] = spec.default;
  }
  return resolved;
}

const [action, flowFile, rawArgs = "{}"] = process.argv.slice(2);
if (!["verify", "plan", "compile"].includes(action) || !flowFile) {
  usage();
  process.exit(2);
}

try {
  const [core, flow] = await Promise.all([loadPinnedCore(), loadFlow(flowFile)]);
  let args;
  try {
    args = JSON.parse(rawArgs);
  } catch (error) {
    throw new Error(`invalid args JSON: ${error.message}`, { cause: error });
  }
  if (!args || Array.isArray(args) || typeof args !== "object") throw new Error("args JSON must be an object");
  args = resolveArgs(flow, args);

  if (action === "plan") {
    const result = core.preflightTaskflow(flow, { args, cwd: root });
    process.stdout.write(`${core.formatPreflightReport(result)}\n`);
    if (!result.ok) process.exitCode = 1;
  } else {
    const validation = core.validateTaskflow(flow, { args, cwd: root });
    for (const warning of validation.warnings) process.stderr.write(`[taskflow-static] warning: ${warning}\n`);
    if (!validation.ok) throw new Error(`invalid taskflow:\n- ${validation.errors.join("\n- ")}`);
    const compiled = core.compileTaskflow(flow, { title: flow.description ?? flow.name });
    if (action === "compile") process.stdout.write(`${compiled.markdown}\n`);
    else {
      const issues = compiled.verification.issues;
      process.stdout.write(`taskflow-static: VERIFIED name=${flow.name} phases=${flow.phases.length} issues=${issues.length}\n`);
      for (const issue of issues) {
        process.stdout.write(`${issue.severity.toUpperCase()} ${issue.category}${issue.phaseId ? ` ${issue.phaseId}` : ""}: ${issue.message}\n`);
      }
    }
    if (!compiled.verification.ok) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`[taskflow-static] ${error.message}\n`);
  process.exitCode = 1;
}
