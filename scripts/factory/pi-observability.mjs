#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { loopbackJson } from "../lib/loopback-http.mjs";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PACKAGE_NAME = "@grafana/agento11y-pi";
const PACKAGE_VERSION = "0.25.0";
const RECEIVER_URL = "http://127.0.0.1:8765";
const ENUMS = Object.freeze({
  lane: ["supervisor", "host-headless", "host-mobile", "docker"],
  profile: ["prototype", "production-ready", "research"],
  "work-type": ["feat", "fix", "docs", "test", "refactor", "chore"],
  "delivery-target": ["develop", "milestone", "none"],
});

function usage() {
  return [
    "Usage:",
    "  node scripts/factory/pi-observability.mjs on --lane L --profile P --work-type W --delivery-target D [-- PI_ARGS...]",
    "  node scripts/factory/pi-observability.mjs off [-- PI_ARGS...]",
    "  node scripts/factory/pi-observability.mjs status",
    "  node scripts/factory/pi-observability.mjs stop",
  ].join("\n");
}

export function parseObservedArgs(argv) {
  const result = { piArgs: [] };
  let index = 0;
  for (; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--") {
      result.piArgs = argv.slice(index + 1);
      break;
    }
    if (!argument.startsWith("--")) throw new Error(`unexpected argument: ${argument}`);
    const key = argument.slice(2);
    if (!Object.hasOwn(ENUMS, key) || result[key] !== undefined) {
      throw new Error(`unknown or repeated option: ${argument}`);
    }
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`${argument} requires a value`);
    if (!ENUMS[key].includes(value)) throw new Error(`${argument} must be one of: ${ENUMS[key].join(", ")}`);
    result[key] = value;
    index += 1;
  }
  for (const key of Object.keys(ENUMS)) {
    if (result[key] === undefined) throw new Error(`missing --${key}`);
  }
  return result;
}

export function observabilityTags(parsed) {
  return [
    "project=oxid",
    "factory=pi-dev",
    "environment=local",
    `lane=${parsed.lane}`,
    `profile=${parsed.profile}`,
    `work_type=${parsed["work-type"]}`,
    `delivery_target=${parsed["delivery-target"]}`,
  ];
}

export function observabilityEnvironment(parsed, base = process.env) {
  return {
    ...base,
    // Agento11y v0.48 local mode forces full capture. Point normal
    // metadata-only export at the already-running loopback receiver instead.
    AGENTO11Y_LOCAL: "false",
    AGENTO11Y_LOCAL_FORWARD: "false",
    AGENTO11Y_ENDPOINT: RECEIVER_URL,
    AGENTO11Y_AUTH_TENANT_ID: "local",
    AGENTO11Y_AUTH_TOKEN: "local",
    AGENTO11Y_OTEL_EXPORTER_OTLP_ENDPOINT: `${RECEIVER_URL}/otlp`,
    AGENTO11Y_CONTENT_CAPTURE_MODE: "metadata_only",
    AGENTO11Y_GUARDS_ENABLED: "false",
    AGENTO11Y_AUTO_CODING_AGENT_TAGS: "false",
    AGENTO11Y_TAGS: observabilityTags(parsed).join(","),
  };
}

async function observedExtensionPath() {
  const resolved = await resolveDevLoopsPackageRoot({ cwd: REPO_ROOT, includeAllPinnedPackages: true });
  const candidate = resolved.packageRoots.find(({ name }) => name === PACKAGE_NAME);
  if (!candidate) throw new Error(`${PACKAGE_NAME}@${PACKAGE_VERSION} is absent from the project package closure; run ./bootstrap.sh --check`);
  const manifest = JSON.parse(await readFile(path.join(candidate.packageRoot, "package.json"), "utf8"));
  if (manifest.name !== PACKAGE_NAME || manifest.version !== PACKAGE_VERSION) {
    throw new Error(`expected ${PACKAGE_NAME}@${PACKAGE_VERSION}, found ${manifest.name}@${manifest.version}`);
  }
  const extension = path.join(candidate.packageRoot, "dist", "index.js");
  if (!existsSync(extension)) throw new Error(`observability extension is missing: ${extension}`);
  return extension;
}

async function personalIntegrationConfigured() {
  const file = path.join(os.homedir(), ".pi", "agent", "settings.json");
  if (!existsSync(file)) return false;
  try {
    const settings = JSON.parse(await readFile(file, "utf8"));
    return (settings.packages ?? []).some((entry) => String(typeof entry === "string" ? entry : entry?.source).includes(PACKAGE_NAME));
  } catch (error) {
    throw new Error(`cannot verify personal Pi settings at ${file}: ${error.message}`);
  }
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}: ${(result.stderr || result.stdout).trim()}`);
  return result;
}

function jsonFrom(command, args) {
  return JSON.parse(execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
}

async function status() {
  let receiver = { running: false, url: RECEIVER_URL };
  try {
    const response = await loopbackJson(8765, "/api/v1/metrics/conversations", 1500);
    if (response.ok) {
      const metrics = response.json;
      receiver = { running: true, url: RECEIVER_URL, conversations: metrics.matched_conversations ?? 0 };
    }
  } catch {
    // An absent receiver is a valid disabled state.
  }
  let doctor = null;
  try { doctor = jsonFrom("agento11y", ["doctor", "--json"]); } catch { /* Report absence without leaking configuration. */ }
  let grafana = false;
  try {
    const response = await loopbackJson(3000, "/api/health", 1500);
    grafana = response.ok;
  } catch { /* Grafana is optional. */ }
  process.stdout.write(`${JSON.stringify({
    mode: "opt-in",
    package: `${PACKAGE_NAME}@${PACKAGE_VERSION}`,
    projectExtensionDefault: "disabled",
    personalIntegrationConfigured: await personalIntegrationConfigured(),
    receiver,
    grafana: { running: grafana, url: "http://127.0.0.1:3000/d/oxid-pi-factory" },
    doctorAvailable: doctor !== null,
    privacy: "metadata_only",
  }, null, 2)}\n`);
}

async function main(argv) {
  const [command, ...rest] = argv;
  if (command === "status" && rest.length === 0) return status();
  if (command === "stop" && rest.length === 0) {
    run("agento11y", ["local", "stop"], { stdio: "inherit" });
    return;
  }
  if (command === "off") {
    const piArgs = rest[0] === "--" ? rest.slice(1) : rest;
    if (await personalIntegrationConfigured()) {
      throw new Error(`cannot guarantee off mode while ${PACKAGE_NAME} is enabled in personal Pi settings; remove the personal integration first`);
    }
    const result = spawnSync("pi", piArgs, { cwd: REPO_ROOT, env: process.env, stdio: "inherit" });
    if (result.error) throw result.error;
    process.exitCode = result.status ?? 1;
    return;
  }
  if (command === "on") {
    const parsed = parseObservedArgs(rest);
    run("agento11y", ["local", "start"], { stdio: "inherit" });
    const tagArgs = observabilityTags(parsed).flatMap((tag) => ["--tag", tag]);
    const extension = await observedExtensionPath();
    const result = spawnSync("agento11y", ["pi", "--no-local", ...tagArgs, "--", "-e", extension, ...parsed.piArgs], {
      cwd: REPO_ROOT,
      env: observabilityEnvironment(parsed),
      stdio: "inherit",
    });
    if (result.error) throw result.error;
    process.exitCode = result.status ?? 1;
    return;
  }
  throw new Error(usage());
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`[pi-observability] ${error.message}\n`);
    process.exitCode = 1;
  });
}
