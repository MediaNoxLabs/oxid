#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { checkUserPolicy } from "./pi-policy.mjs";

const DEFAULT_REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const EXPECTED_PACKAGES = new Map([
  ["dev-loops", "0.9.0"],
  ["pi-subagents", "0.42.1"],
  ["@input-output-hk/agent-review-pi", "0.5.0"],
]);
const EXPECTED_PROJECT_VALUES = Object.freeze({
  "compaction.enabled": true,
  "compaction.reserveTokens": 16384,
  "compaction.keepRecentTokens": 20000,
  "retry.enabled": true,
  "retry.maxRetries": 1,
  "retry.baseDelayMs": 2000,
  "retry.provider.timeoutMs": 600000,
  "retry.provider.maxRetries": 0,
  "retry.provider.maxRetryDelayMs": 60000,
  "subagents.projectRootResolution": "git-root",
});

function getAtPath(object, dotted) {
  return dotted.split(".").reduce((value, key) => value?.[key], object);
}

function check(id, status, summary, details = undefined, category = "configuration") {
  return { id, status, category, summary, ...(details === undefined ? {} : { details }) };
}

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 8 * 1024 * 1024,
    ...options,
  }).trim();
}

function resolveGitLayout(repoRoot) {
  const raw = run("git", ["rev-parse", "--git-common-dir"], { cwd: repoRoot });
  const commonGitDir = path.resolve(repoRoot, raw);
  return { commonGitDir, commonCheckout: path.dirname(commonGitDir) };
}

function stripYamlComment(value, file) {
  let quote = null;
  let escaped = false;
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (quote === '"' && escaped) {
      escaped = false;
      continue;
    }
    if (quote === '"' && character === "\\") {
      escaped = true;
      continue;
    }
    if (quote !== null) {
      if (character === quote) quote = null;
      continue;
    }
    if ((character === '"' || character === "'")
      && (index === 0 || /[\s[{,:]/u.test(value[index - 1]))) quote = character;
    else if (character === "#" && (index === 0 || /\s/u.test(value[index - 1]))) return value.slice(0, index).trimEnd();
  }
  if (quote !== null) throw new Error(`${file}: unmatched quote in frontmatter`);
  return value;
}

function parseFrontmatter(source, file) {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/u);
  if (!match) throw new Error(`${file}: missing YAML frontmatter`);
  const result = {};
  for (const line of match[1].split(/\r?\n/u)) {
    if (/^\s*(?:#.*)?$/u.test(line) || /^\s/u.test(line)) continue;
    const field = line.match(/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/u);
    if (!field) continue;
    const [, key, raw = ""] = field;
    let value = stripYamlComment(raw, file).trim();
    if (["|", ">", "|-", ">-"].includes(value)) {
      throw new Error(`${file}: ${key} must use a scalar; YAML block values are outside the budget contract`);
    }
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1);
    }
    result[key] = value;
  }
  return result;
}

function validateAgentBudget(file, fields) {
  const problems = [];
  const timeoutMs = Number(fields.timeoutMs);
  if (!Number.isInteger(timeoutMs) || timeoutMs < 60_000 || timeoutMs > 3_600_000) {
    problems.push(`${file}: timeoutMs must be 60000..3600000`);
  }
  try {
    const budget = JSON.parse(fields.turnBudget ?? "null");
    if (!budget || !Number.isInteger(budget.maxTurns) || budget.maxTurns < 1 || budget.maxTurns > 32) {
      problems.push(`${file}: turnBudget.maxTurns must be 1..32`);
    }
    if (!Number.isInteger(budget?.graceTurns) || budget.graceTurns < 0 || budget.graceTurns > 2) {
      problems.push(`${file}: turnBudget.graceTurns must be 0..2`);
    }
  } catch {
    problems.push(`${file}: turnBudget must be valid JSON`);
  }
  if (file === "dev-loop.agent.md" && Number(fields.maxSubagentDepth) !== 2) {
    problems.push(`${file}: maxSubagentDepth must be 2`);
  }
  return problems;
}

async function inspectInstalledPackages(commonCheckout) {
  const store = path.join(commonCheckout, ".pi", "npm", "node_modules");
  const installed = {};
  const problems = [];
  for (const [name, expected] of EXPECTED_PACKAGES) {
    try {
      const manifest = JSON.parse(await readFile(path.join(store, name, "package.json"), "utf8"));
      installed[name] = manifest.version ?? null;
      if (manifest.name !== name || manifest.version !== expected) {
        problems.push(`${name}: expected ${expected}, installed ${manifest.version ?? "unknown"}`);
      }
    } catch (error) {
      if (error?.code === "ENOENT") problems.push(`${name}: exact package is not installed in the common store`);
      else problems.push(`${name}: ${error.message}`);
    }
  }
  return { installed, problems };
}

function inspectOperationalState(repoRoot) {
  try {
    const lifecycle = JSON.parse(run(process.execPath, [path.join(repoRoot, "scripts", "worktree-lifecycle.mjs"), "audit", "--json"], {
      cwd: repoRoot,
      timeout: 60_000,
    }));
    if (!Array.isArray(lifecycle) || lifecycle.some((item) => !item || typeof item.merged !== "boolean"
      || !Number.isFinite(item.targetGiB) || item.targetGiB < 0 || typeof item.removableAfterSevenDays !== "boolean")) {
      throw new Error("lifecycle audit returned an invalid JSON contract");
    }
    const nonPrimary = lifecycle.slice(1);
    const active = nonPrimary.filter((item) => !item.merged);
    const targetGiB = lifecycle.reduce((sum, item) => sum + (Number(item.targetGiB) || 0), 0);
    const removable = nonPrimary.filter((item) => item.removableAfterSevenDays).length;
    const worktreeStatus = active.length <= 2 ? "pass" : "fail";
    const diskStatus = targetGiB <= 100 ? "pass" : targetGiB <= 200 ? "warn" : "fail";
    return [
      check("worktree-admission", worktreeStatus,
        `${active.length} active and ${lifecycle.length} registered worktrees in this Git common checkout; green active limit is 2`,
        { active: active.length, registered: lifecycle.length, removable }, "operational"),
      check("worktree-target-storage", diskStatus,
        `${targetGiB.toFixed(1)} GiB in worktree-local target directories`,
        { targetGiB, greenMaximumGiB: 100, amberMaximumGiB: 200 }, "operational"),
    ];
  } catch (error) {
    try {
      const worktrees = run("git", ["worktree", "list", "--porcelain"], { cwd: repoRoot })
        .split(/\r?\n/u)
        .filter((line) => line.startsWith("worktree "))
        .map((line) => line.slice("worktree ".length));
      if (worktrees.length === 0) throw new Error("git reported no registered worktrees");
      const nonPrimary = worktrees.slice(1);
      const targetGiB = worktrees.reduce((sum, worktree) => {
        const target = path.join(worktree, "target");
        if (!existsSync(target)) return sum;
        const kib = Number(run("du", ["-sk", target]).split(/\s/u, 1)[0]);
        if (!Number.isFinite(kib) || kib < 0) throw new Error(`du returned an invalid size for ${target}`);
        return sum + kib / 1024 / 1024;
      }, 0);
      return [
        check("worktree-admission", nonPrimary.length <= 2 ? "pass" : "fail",
          `${nonPrimary.length} non-primary registered worktrees from conservative fallback; green limit is 2`,
          { active: nonPrimary.length, registered: worktrees.length, fallback: true }, "operational"),
        check("worktree-target-storage", targetGiB <= 100 ? "pass" : targetGiB <= 200 ? "warn" : "fail",
          `${targetGiB.toFixed(1)} GiB in worktree-local target directories from conservative fallback`,
          { targetGiB, greenMaximumGiB: 100, amberMaximumGiB: 200, fallback: true }, "operational"),
      ];
    } catch (fallbackError) {
      return [check("worktree-admission", "fail",
        `Worktree capacity audit unavailable: ${error.message}; fallback failed: ${fallbackError.message}`,
        undefined, "operational")];
    }
  }
}

async function inspectMetrics(repoRoot) {
  try {
    const audit = JSON.parse(run(process.execPath, [path.join(repoRoot, "scripts", "factory", "metrics.mjs"), "audit", "--json"], {
      cwd: repoRoot,
      timeout: 30_000,
    }));
    const valid = audit.records?.valid ?? 0;
    const invalid = audit.records?.invalid ?? 0;
    const status = invalid > 0 ? "fail" : valid > 0 ? "pass" : "warn";
    return check("metrics-coverage", status,
      status === "pass"
        ? `${valid} valid private work-item metric records available`
        : status === "warn"
          ? "No private work-item metrics exist yet; record the first completed work item to establish coverage"
          : `${valid} valid and ${invalid} invalid work-item metric records`,
      { valid, invalid }, "observability");
  } catch (error) {
    return check("metrics-coverage", "warn", `Metrics store unavailable: ${error.message}`, undefined, "observability");
  }
}

async function inspectDeliveryProfiles(repoRoot) {
  const problems = [];
  try {
    const profiles = JSON.parse(await readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8"));
    const names = Object.keys(profiles.profiles ?? {}).sort();
    if (profiles.schemaVersion !== 1) problems.push("schemaVersion must be 1");
    if (profiles.defaultProfile !== "production-ready") problems.push("production-ready must remain the default");
    if (JSON.stringify(names) !== JSON.stringify(["production-ready", "prototype"])) {
      problems.push("profiles must contain exactly prototype and production-ready");
    }

    const prototype = profiles.profiles?.prototype;
    if (prototype?.remoteMutation !== false || prototype?.mergeEligible !== false
      || prototype?.evidenceClass !== "provisional") {
      problems.push("prototype must be local-only, non-mergeable, and provisional");
    }
    if (prototype?.maximumReviewers !== 1) problems.push("prototype must use at most one reviewer");
    if (JSON.stringify(prototype?.targets?.required) !== JSON.stringify(["basic"])) {
      problems.push("prototype must require only the basic target");
    }
    if (JSON.stringify(prototype?.targets?.optionalHostedOnDemand) !== JSON.stringify(["unit-linux", "headless-linux"])
      || prototype?.targets?.maximumFocusedQualifications !== 1) {
      problems.push("prototype must allow at most one focused qualification and only bounded hosted targets");
    }
    if (prototype?.sloSeconds?.firstFeedback !== 180 || prototype?.sloSeconds?.focusedIteration !== 600) {
      problems.push("prototype feedback and iteration SLOs must remain bounded");
    }

    const production = profiles.profiles?.["production-ready"];
    if (production?.remoteMutation !== "authority-gated"
      || production?.mergeEligible !== "after-required-gates"
      || production?.evidenceClass !== "production") {
      problems.push("production-ready must retain authority, gate, and evidence controls");
    }
    if (production?.maximumReviewers !== 2) problems.push("production-ready reviewer cap must remain two");

    const promotion = profiles.promotion;
    if (promotion?.explicit !== true || promotion?.refreshBase !== "origin/develop"
      || promotion?.auditPrototypeGaps !== true || promotion?.invalidateProvisionalEvidence !== true
      || promotion?.recomputeTargets !== true) {
      problems.push("promotion must refresh develop, audit gaps, invalidate provisional evidence, and recompute targets");
    }

    const [devLoopAgent, rootAgent] = await Promise.all([
      readFile(path.join(repoRoot, ".pi", "agents", "dev-loop.agent.md"), "utf8"),
      readFile(path.join(repoRoot, "AGENT.md"), "utf8"),
    ]);
    for (const [file, source] of [[".pi/agents/dev-loop.agent.md", devLoopAgent], ["AGENT.md", rootAgent]]) {
      if (!source.includes("/dev-loop prototype issue <n>")) problems.push(`${file} is missing the prototype entrypoint`);
      if (!source.includes("/dev-loop production-ready issue <n>")) problems.push(`${file} is missing the production-ready entrypoint`);
    }
    if (!devLoopAgent.includes("--delivery-profile <profile>")) {
      problems.push(".pi/agents/dev-loop.agent.md does not bind the profile into the handoff envelope");
    }
  } catch (error) {
    problems.push(error.message);
  }

  return check("delivery-profiles", problems.length ? "fail" : "pass",
    problems.length
      ? "Tracked delivery profiles are incomplete or unsafe"
      : "Prototype and production-ready profiles have bounded selection and promotion rules",
    problems.length ? problems : undefined);
}

/**
 * Worktree creation needs only host-capacity admission. It deliberately does
 * not depend on Pi being installed, user policy being configured, documentation
 * prose, or prior metrics existing, so a fresh checkout can create its first
 * isolated worker.
 */
export async function auditWorktreeAdmission({ repoRoot = DEFAULT_REPO_ROOT } = {}) {
  const checks = inspectOperationalState(repoRoot);
  const capacityEvidenceAvailable = !checks.some((item) => item.id === "worktree-admission" && item.status === "warn");
  return {
    schemaVersion: 1,
    operationalChecked: true,
    configReady: null,
    admissionReady: checks.every((item) => item.status !== "fail"),
    capacityEvidenceAvailable,
    checks,
  };
}

export async function auditPi({
  repoRoot = DEFAULT_REPO_ROOT,
  includeOperational = true,
  env = process.env,
  piVersion = undefined,
  userPolicyResult = undefined,
} = {}) {
  const checks = [];
  const settings = JSON.parse(await readFile(path.join(repoRoot, ".pi", "settings.json"), "utf8"));
  const settingProblems = Object.entries(EXPECTED_PROJECT_VALUES)
    .filter(([field, expected]) => JSON.stringify(getAtPath(settings, field)) !== JSON.stringify(expected))
    .map(([field, expected]) => `${field}: expected ${JSON.stringify(expected)}, found ${JSON.stringify(getAtPath(settings, field))}`);
  const parentModel = typeof settings.defaultProvider === "string" && typeof settings.defaultModel === "string"
    ? `${settings.defaultProvider}/${settings.defaultModel}`
    : null;
  if (!parentModel || !/^[a-z0-9-]+\/[a-z0-9.-]+$/u.test(parentModel)) {
    settingProblems.push("defaultProvider/defaultModel: expected a well-formed provider/model pair");
  }
  if (settings.subagents?.defaultModel !== parentModel) {
    settingProblems.push(`subagents.defaultModel: expected ${JSON.stringify(parentModel)}, found ${JSON.stringify(settings.subagents?.defaultModel)}`);
  }
  if (settings.subagents?.defaultThinking !== settings.defaultThinkingLevel) {
    settingProblems.push("subagents.defaultThinking must match defaultThinkingLevel");
  }
  if (!["off", "minimal", "low", "medium", "high", "xhigh", "max"].includes(settings.defaultThinkingLevel)) {
    settingProblems.push(`defaultThinkingLevel: unsupported value ${JSON.stringify(settings.defaultThinkingLevel)}`);
  }
  checks.push(check("project-pi-policy", settingProblems.length ? "fail" : "pass",
    settingProblems.length ? "Project Pi defaults are not bounded" : "Project model, retry, provider deadline, and compaction are bounded",
    settingProblems.length ? settingProblems : undefined));

  const packageProblems = [];
  const configuredPackages = new Map((settings.packages ?? []).map((entry) => {
    const match = String(entry).match(/^npm:(@[^/]+\/[^@]+|[^@]+)@(.+)$/u);
    return match ? [match[1], match[2]] : [String(entry), null];
  }));
  for (const [name, expected] of EXPECTED_PACKAGES) {
    if (configuredPackages.get(name) !== expected) packageProblems.push(`${name}: expected exact pin ${expected}`);
  }
  checks.push(check("package-pins", packageProblems.length ? "fail" : "pass",
    packageProblems.length ? "Package pins are incomplete or floating" : "All Pi packages use exact tracked pins",
    packageProblems.length ? packageProblems : undefined));

  let layout;
  try {
    layout = resolveGitLayout(repoRoot);
    const installed = await inspectInstalledPackages(layout.commonCheckout);
    checks.push(check("installed-packages", installed.problems.length ? "fail" : "pass",
      installed.problems.length ? "The common Pi package store does not match tracked pins" : "Installed Pi packages match tracked pins",
      { installed: installed.installed, problems: installed.problems }, "runtime"));
  } catch (error) {
    checks.push(check("installed-packages", "fail", `Cannot resolve the common Pi package store: ${error.message}`, undefined, "runtime"));
  }

  let effectivePiVersion = piVersion;
  if (effectivePiVersion === undefined) {
    try { effectivePiVersion = run("pi", ["--version"], { cwd: repoRoot }); } catch { effectivePiVersion = null; }
  }
  const validPiVersion = typeof effectivePiVersion === "string" && /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u.test(effectivePiVersion);
  checks.push(check("pi-runtime", validPiVersion ? "pass" : "fail",
    validPiVersion
      ? `Nix-pinned Pi ${effectivePiVersion} is active`
      : `The Pi executable is unavailable or reported an invalid version: ${effectivePiVersion ?? "unavailable"}`));

  const agentProblems = [];
  const agentDir = path.join(repoRoot, ".pi", "agents");
  for (const file of (await readdir(agentDir)).filter((entry) => entry.endsWith(".agent.md")).sort()) {
    try {
      agentProblems.push(...validateAgentBudget(file, parseFrontmatter(await readFile(path.join(agentDir, file), "utf8"), file)));
    } catch (error) {
      agentProblems.push(error.message);
    }
  }
  checks.push(check("tracked-agent-budgets", agentProblems.length ? "fail" : "pass",
    agentProblems.length ? "One or more tracked agents are unbounded" : "Every tracked agent has a bounded runtime and turn budget",
    agentProblems.length ? agentProblems : undefined));

  const effectiveUserPolicy = userPolicyResult ?? await checkUserPolicy({ env });
  checks.push(check("user-subagent-policy", effectiveUserPolicy.ok ? "pass" : "fail",
    effectiveUserPolicy.ok ? "Effective pi-subagents concurrency, spawn, turn, token, and artifact policy is aligned" : "Effective user pi-subagents policy is not aligned",
    effectiveUserPolicy.ok ? { configPath: effectiveUserPolicy.configPath } : {
      configPath: effectiveUserPolicy.configPath,
      mismatches: effectiveUserPolicy.mismatches.map((item) => item.field),
    }));

  const devloops = await readFile(path.join(repoRoot, ".devloops"), "utf8");
  const devloopBounds = [
    /fanOut:\s*2/u,
    /maxFanoutReviewers:\s*2/u,
    /maxParallel:\s*1/u,
    /reDispatchMaxRetries:\s*0/u,
  ];
  checks.push(check("dev-loop-bounds", devloopBounds.every((pattern) => pattern.test(devloops)) ? "pass" : "fail",
    devloopBounds.every((pattern) => pattern.test(devloops))
      ? "Dev-loop review, queue, retry, and develop merge concurrency are bounded"
      : "One or more dev-loop constitutional bounds are missing"));

  checks.push(await inspectDeliveryProfiles(repoRoot));

  if (includeOperational) {
    checks.push(...inspectOperationalState(repoRoot));
    checks.push(await inspectMetrics(repoRoot));
  }

  const configurationFailures = checks.filter((item) => item.category === "configuration" && item.status === "fail");
  const operationalFailures = checks.filter((item) => item.category === "operational" && item.status === "fail");
  const capacityEvidenceAvailable = !checks.some((item) => item.id === "worktree-admission" && item.status === "warn");
  return {
    schemaVersion: 1,
    operationalChecked: includeOperational,
    configReady: configurationFailures.length === 0,
    admissionReady: includeOperational
      ? configurationFailures.length === 0 && operationalFailures.length === 0 && capacityEvidenceAvailable
      : null,
    capacityEvidenceAvailable: includeOperational ? capacityEvidenceAvailable : null,
    checks,
  };
}

function renderText(result) {
  const lines = result.checks.map((item) => `[${item.status.toUpperCase()}] ${item.id}: ${item.summary}`);
  lines.push(`Config ready: ${result.configReady ? "yes" : "no"}`);
  lines.push(`Factory admission ready: ${result.operationalChecked ? (result.admissionReady ? "yes" : "no") : "not checked (config-only)"}`);
  return `${lines.join("\n")}\n`;
}

async function main(argv = process.argv.slice(2)) {
  const known = new Set(["--json", "--config-only", "--enforce-config", "--enforce-admission", "--help", "-h"]);
  const unknown = argv.find((value) => !known.has(value));
  if (unknown) throw new Error(`Unknown argument: ${unknown}`);
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write("Usage: node scripts/factory/audit-pi.mjs [--json] [--config-only] [--enforce-config|--enforce-admission]\n");
    return 0;
  }
  if (argv.includes("--config-only") && argv.includes("--enforce-admission")) {
    throw new Error("--config-only cannot be combined with --enforce-admission");
  }
  const result = await auditPi({ includeOperational: !argv.includes("--config-only") });
  process.stdout.write(argv.includes("--json") ? `${JSON.stringify(result, null, 2)}\n` : renderText(result));
  if (argv.includes("--enforce-admission") && !result.admissionReady) return 1;
  if (argv.includes("--enforce-config") && !result.configReady) return 1;
  if (!argv.includes("--enforce-admission") && !argv.includes("--enforce-config")
    && result.checks.some((item) => item.status === "fail")) return 1;
  return 0;
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  main().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[audit-pi] ${error.message}\n`);
    process.exitCode = 1;
  });
}
