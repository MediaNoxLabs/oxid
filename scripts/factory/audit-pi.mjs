#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { accessSync, constants as fsConstants, existsSync, realpathSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { resolvePinnedCoreModulePath } from "../dev-loops.mjs";
import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";
import { checkUserPolicy } from "./pi-policy.mjs";

const DEFAULT_REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const EXPECTED_PACKAGES = new Map([
  ["dev-loops", "1.0.2"],
  ["pi-subagents", "0.66.0"],
  ["@playwright/test", "1.60.0"],
  ["@axe-core/playwright", "4.10.0"],
  ["typebox", "1.3.9"],
  ["pi-taskflow", "0.2.10"],
  ["@input-output-hk/agent-review-pi", "0.6.0"],
]);
const DEV_LOOPS_RESOURCE_POLICY = Object.freeze({
  source: "npm:dev-loops@1.0.2",
  extensions: [],
});
const TASKFLOW_SUPPRESSION = Object.freeze({
  source: "npm:pi-taskflow@0.2.10",
  extensions: [],
  skills: [],
  prompts: [],
  themes: [],
});
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

async function inspectDevLoopsLayer(repoRoot) {
  let resolved;
  try {
    resolved = await resolveDevLoopsPackageRoot({ cwd: repoRoot });
  } catch (error) {
    return check(
      "dev-loop-effective-config",
      "warn",
      "Effective .devloops validation awaits the exact installed Pi package",
      [error.message],
      "runtime",
    );
  }
  try {
    const handoffModulePath = await resolvePinnedCoreModulePath(resolved.packageRoot);
    const configModulePath = path.resolve(path.dirname(handoffModulePath), "..", "config", "config.mjs");
    const { loadDevLoopConfig, resolveFanoutMaxConcurrent, resolveGateConfig, resolveRefinement } = await import(
      pathToFileURL(configModulePath).href
    );
    const loaded = await loadDevLoopConfig({ repoRoot });
    const refinement = resolveRefinement(loaded.config);
    const draft = resolveGateConfig(loaded.config, "draft");
    const preApproval = resolveGateConfig(loaded.config, "preApproval");
    const problems = [
      ...loaded.errors.map((error) => `${error.layer}: ${error.message}`),
      ...(loaded.config.strategy === "local-first" ? [] : [`strategy: expected local-first, found ${JSON.stringify(loaded.config.strategy)}`]),
      ...(refinement.fanOut === 1 && refinement.maxCopilotRounds === 0
        && refinement.stopOnLowSignal === true && refinement.lowSignalRoundThreshold === 1 && refinement.lowSignalMaxComments === 1
        ? [] : ["refinement: expected bounded fan-out, disabled Copilot, and 1/1 enabled low-signal policy"]),
      ...(JSON.stringify(draft.angles) === JSON.stringify(["correctness"])
        && JSON.stringify(draft.mandatoryAngles) === JSON.stringify(["correctness"])
        && JSON.stringify(draft.blockCleanOnFindingSeverities) === JSON.stringify(["high"])
        && draft.requireCi === false
        ? [] : ["draft gate: expected only mandatory correctness with requireCi: false"]),
      ...(JSON.stringify(preApproval.angles) === JSON.stringify(["security"])
        && JSON.stringify(preApproval.mandatoryAngles) === JSON.stringify(["security"])
        && JSON.stringify(preApproval.blockCleanOnFindingSeverities) === JSON.stringify(["high"])
        && preApproval.requireCi === true
        ? [] : ["pre-approval gate: expected only mandatory security with requireCi: true"]),
      ...(resolveFanoutMaxConcurrent(loaded.config) === 1
        ? [] : ["fan-out: expected maxConcurrent: 1"]),
    ];
    return check("dev-loop-effective-config", problems.length ? "fail" : "pass",
      problems.length ? "Repository .devloops was rejected or its effective bounded gate policy drifted" : "Repository .devloops loaded and resolves to the bounded gate policy",
      problems.length ? problems : undefined);
  } catch (error) {
    return check("dev-loop-effective-config", "fail", "Repository .devloops could not be loaded through the pinned config loader", [error.message]);
  }
}

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 8 * 1024 * 1024,
    ...options,
  }).trim();
}

function executableFromPath(name, env) {
  for (const directory of String(env.PATH ?? "").split(path.delimiter)) {
    if (directory.length === 0) continue;
    const candidate = path.join(directory, name);
    try {
      accessSync(candidate, fsConstants.X_OK);
      return realpathSync(candidate);
    } catch {
      // Continue through PATH exactly as process lookup would.
    }
  }
  return null;
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
    const budget = JSON.parse(fields.toolBudget ?? "null");
    if (!budget || !Number.isInteger(budget.soft) || budget.soft < 1 || budget.soft > 64) {
      problems.push(`${file}: toolBudget.soft must be 1..64`);
    }
    if (!Number.isInteger(budget?.hard) || budget.hard < budget.soft || budget.hard > 96) {
      problems.push(`${file}: toolBudget.hard must be >= soft and <= 96`);
    }
    if (budget?.block !== "*") {
      problems.push(`${file}: toolBudget.block must be \"*\"`);
    }
  } catch {
    problems.push(`${file}: toolBudget must be valid JSON`);
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
    if (!Array.isArray(lifecycle) || lifecycle.some((item) => !item || typeof item.clean !== "boolean"
      || typeof item.merged !== "boolean"
      || !Number.isFinite(item.targetGiB) || item.targetGiB < 0 || typeof item.removableAfterSevenDays !== "boolean")) {
      throw new Error("lifecycle audit returned an invalid JSON contract");
    }
    return lifecycleCapacityChecks(lifecycle);
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

export function lifecycleCapacityChecks(lifecycle) {
  if (!Array.isArray(lifecycle) || lifecycle.length === 0) {
    throw new Error("lifecycle audit must contain the primary checkout");
  }
  const nonPrimary = lifecycle.slice(1);
  const active = nonPrimary.filter((item) => !item.merged || !item.clean);
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
    if (production?.maximumReviewers !== 1) problems.push("production-ready must default to one reviewer");
    if (production?.qualityBudget?.targetPercent !== 70
      || production?.qualityBudget?.mandatoryInvariantsPercent !== 100
      || production?.qualityBudget?.maximumAutomaticReviewRounds !== 1
      || production?.qualityBudget?.advisoryDisposition !== "follow-up") {
      problems.push("production-ready must preserve the 70 percent quality budget and complete mandatory invariants");
    }

    const promotion = profiles.promotion;
    if (promotion?.explicit !== true || promotion?.refreshBase !== "recorded-delivery-base"
      || promotion?.auditPrototypeGaps !== true || promotion?.invalidateProvisionalEvidence !== true
      || promotion?.recomputeTargets !== true) {
      problems.push("promotion must refresh the recorded delivery base, audit gaps, invalidate provisional evidence, and recompute targets");
    }
    const target = profiles.deliveryTarget;
    if (target?.required !== true || target?.productPattern !== "origin/milestone-<x.y.z>"
      || target?.factoryTarget !== "origin/develop" || target?.inferNewest !== false
      || target?.sessionLocal !== true) {
      problems.push("delivery target must be explicit, session-local, and never inferred");
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
    if (!devLoopAgent.includes("--delivery-base <target>")) {
      problems.push(".pi/agents/dev-loop.agent.md does not bind the issue target into the handoff envelope");
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
  piExecutable = undefined,
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
    const source = typeof entry === "string" ? entry : entry?.source;
    const match = String(source).match(/^npm:(@[^/]+\/[^@]+|[^@]+)@(.+)$/u);
    return match ? [match[1], { version: match[2], entry }] : [String(source), { version: null, entry }];
  }));
  for (const [name, expected] of EXPECTED_PACKAGES) {
    if (configuredPackages.get(name)?.version !== expected) packageProblems.push(`${name}: expected exact pin ${expected}`);
  }
  const devLoopsEntry = configuredPackages.get("dev-loops")?.entry;
  if (JSON.stringify(devLoopsEntry) !== JSON.stringify(DEV_LOOPS_RESOURCE_POLICY)) {
    packageProblems.push("dev-loops: package extension must be suppressed so session_start cannot overwrite tracked project agents");
  }
  const taskflowEntry = configuredPackages.get("pi-taskflow")?.entry;
  if (JSON.stringify(taskflowEntry) !== JSON.stringify(TASKFLOW_SUPPRESSION)) {
    packageProblems.push("pi-taskflow: inherited extension and skills must be fully suppressed until #301 and #196 pass");
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
  let effectivePiExecutable = piExecutable;
  if (effectivePiVersion === undefined) {
    try { effectivePiVersion = run("pi", ["--version"], { cwd: repoRoot, env }); } catch { effectivePiVersion = null; }
  }
  if (effectivePiExecutable === undefined) {
    effectivePiExecutable = executableFromPath("pi", env);
  }
  const validPiVersion = typeof effectivePiVersion === "string" && /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u.test(effectivePiVersion);
  const nixPinnedPi = typeof effectivePiExecutable === "string"
    && /^\/nix\/store\/[a-z0-9]{32}-[^/]+\/bin\/pi$/u.test(effectivePiExecutable);
  checks.push(check("pi-runtime", validPiVersion && nixPinnedPi ? "pass" : "fail",
    validPiVersion && nixPinnedPi
      ? `Nix-pinned Pi ${effectivePiVersion} is active`
      : "Pi must be the versioned executable supplied by the pinned Nix development shell",
    validPiVersion && nixPinnedPi ? undefined : {
      versionValid: validPiVersion,
      nixStoreExecutable: nixPinnedPi,
    }));

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
    agentProblems.length ? "One or more tracked agents are unbounded" : "Every tracked agent has a bounded runtime and tool budget",
    agentProblems.length ? agentProblems : undefined));

  const helperProblems = [];
  for (const relative of [
    "scripts/loop/pre-flight-gate.mjs",
    "scripts/loop/pre-commit-branch-guard.mjs",
    "scripts/loop/ensure-worktree.mjs",
  ]) {
    try {
      accessSync(path.join(repoRoot, relative), fsConstants.R_OK | fsConstants.X_OK);
    } catch (error) {
      helperProblems.push(`${relative}: ${error.message}`);
    }
  }
  checks.push(check("local-implementation-helpers", helperProblems.length ? "fail" : "pass",
    helperProblems.length
      ? "One or more required local-implementation helper routes are unavailable"
      : "Required local-implementation helper routes are tracked and executable",
    helperProblems.length ? helperProblems : undefined));

  const effectiveUserPolicy = userPolicyResult ?? await checkUserPolicy({ env });
  checks.push(check("user-subagent-policy", effectiveUserPolicy.ok ? "pass" : "fail",
    effectiveUserPolicy.ok ? "Effective pi-subagents concurrency, spawn, tool, token, and artifact policy is aligned" : "Effective user pi-subagents policy is not aligned",
    effectiveUserPolicy.ok ? { configPath: effectiveUserPolicy.configPath } : {
      configPath: effectiveUserPolicy.configPath,
      mismatches: effectiveUserPolicy.mismatches.map((item) => item.field),
    }));

  const devloops = await readFile(path.join(repoRoot, ".devloops"), "utf8");
  const devloopBounds = [
    /fanOut:\s*1/u,
    /maxFanoutReviewers:\s*1/u,
    /fanout:\s*\n\s*maxConcurrent:\s*1/u,
    /draft:[\s\S]*?blockCleanOnFindingSeverities:\s*\n\s*- high/u,
    /preApproval:[\s\S]*?blockCleanOnFindingSeverities:\s*\n\s*- high/u,
    /maxParallel:\s*1/u,
    /reDispatchMaxRetries:\s*0/u,
  ];
  checks.push(check("dev-loop-bounds", devloopBounds.every((pattern) => pattern.test(devloops)) ? "pass" : "fail",
    devloopBounds.every((pattern) => pattern.test(devloops))
      ? "Dev-loop review, queue, retry, and develop merge concurrency are bounded"
      : "One or more dev-loop constitutional bounds are missing"));
  checks.push(await inspectDevLoopsLayer(repoRoot));

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
