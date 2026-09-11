#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { validateBranchName } from "./ci/contribution-policy.mjs";
import { extractDeliveryTargetOption } from "./lib/delivery-target.mjs";
import { normalizeHandoffEnvelopeCwd } from "./lib/handoff-envelope-cwd.mjs";
import { applyRepositoryAcceptance, resolveHandoffRequiredReads } from "./lib/handoff-required-reads.mjs";
import { runManagedChild } from "./lib/managed-child-process.mjs";
import { resolveDevLoopsPackageRoot } from "./lib/dev-loop-runtime.mjs";
import { enforceSingleBase, pinnedPublicRoute, readLongOptionValues } from "./lib/pinned-dev-loops-args.mjs";
import { runEnsureWorktree } from "./loop/ensure-worktree.mjs";

const DELIVERY_PROFILE_OPTION = "--delivery-profile";
const PRE_MUTATION_ASSESSMENT_OPTION = "--pre-mutation-assessment";

function bindPrBase(args, target) {
  const bases = readLongOptionValues(args, "--base");
  if (bases.length === 0) {
    return enforceSingleBase(args, target.branch, { addWhenMissing: true, label: "repository dev-loops operations" });
  }
  if (bases.length > 1) throw new Error("repository dev-loops operations accepts exactly one --base");
  if (bases[0] === target.branch) return args;
  const stackBase = validateBranchName(bases[0]);
  if (!stackBase.ok) {
    throw new Error(`PR base must be delivery target ${target.branch} or a conventional issue branch: ${stackBase.errors.join("; ")}`);
  }
  return args;
}

/** Bind public PR routes to the exact issue target; never infer a milestone. */
export function normalizeDevLoopsArgs(argv) {
  if (argv.length === 1 && (argv[0] === "--help" || argv[0] === "-h")) return ["help"];
  const args = [...argv];
  const route = pinnedPublicRoute(args);
  const isPrCreate = route.category === "pr" && (route.command === "create" || route.command === "create-draft");
  const isRemoteRefRoute = (route.category === "loop" && route.command === "ensure-worktree")
    || (route.category === "gate" && route.command === "size-budget");
  const isEnvelope = route.category === "loop" && route.command === "build-envelope";
  if (isEnvelope) return args;
  const hasBase = readLongOptionValues(args, "--base").length > 0;
  const selected = extractDeliveryTargetOption(args, { required: isPrCreate || hasBase });
  if (!selected.target) return selected.args;
  if (isPrCreate) return bindPrBase(selected.args, selected.target);
  return enforceSingleBase(selected.args, isRemoteRefRoute ? selected.target.remoteRef : selected.target.branch, {
    addWhenMissing: isRemoteRefRoute,
    label: "repository dev-loops operations",
  });
}

function routedCommandArgs(args, expectedCategory, expectedCommand) {
  const route = pinnedPublicRoute(args);
  if (route.category !== expectedCategory || route.command !== expectedCommand) return null;
  let categoryIndex = 0;
  while (categoryIndex < args.length) {
    const argument = args[categoryIndex];
    if (["--silent", "-s", "--json"].includes(argument)) {
      categoryIndex += 1;
    } else if (["--jq", "--repo", "--cwd", "--config"].includes(argument)) {
      categoryIndex += 2;
    } else if (["--jq", "--repo", "--cwd", "--config"].some((option) => argument.startsWith(`${option}=`))) {
      categoryIndex += 1;
    } else {
      break;
    }
  }
  const leading = args.slice(0, categoryIndex).filter((argument) => argument !== "--json");
  return [...leading, ...args.slice(categoryIndex + 2)];
}

function buildEnvelopeArgs(args) {
  return routedCommandArgs(args, "loop", "build-envelope");
}

export function extractDeliveryProfileArgs(args) {
  const forwarded = [];
  let requested;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === DELIVERY_PROFILE_OPTION) {
      if (requested !== undefined) throw new Error(`${DELIVERY_PROFILE_OPTION} may be specified only once`);
      const value = args[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${DELIVERY_PROFILE_OPTION} requires a value`);
      requested = value;
      index += 1;
      continue;
    }
    if (argument.startsWith(`${DELIVERY_PROFILE_OPTION}=`)) {
      if (requested !== undefined) throw new Error(`${DELIVERY_PROFILE_OPTION} may be specified only once`);
      requested = argument.slice(`${DELIVERY_PROFILE_OPTION}=`.length);
      if (!requested) throw new Error(`${DELIVERY_PROFILE_OPTION} requires a value`);
      continue;
    }
    forwarded.push(argument);
  }
  return { args: forwarded, requested };
}

export function extractPreMutationAssessmentArgs(args) {
  const forwarded = [];
  let assessment;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    let value;
    if (argument === PRE_MUTATION_ASSESSMENT_OPTION) {
      if (assessment !== undefined) throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} may be specified only once`);
      value = args[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} requires a JSON object`);
      index += 1;
    } else if (argument.startsWith(`${PRE_MUTATION_ASSESSMENT_OPTION}=`)) {
      if (assessment !== undefined) throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} may be specified only once`);
      value = argument.slice(`${PRE_MUTATION_ASSESSMENT_OPTION}=`.length);
      if (!value) throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} requires a JSON object`);
    } else {
      forwarded.push(argument);
      continue;
    }
    try {
      assessment = JSON.parse(value);
    } catch {
      throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} must be valid JSON`);
    }
    if (!assessment || Array.isArray(assessment) || typeof assessment !== "object") {
      throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} must be a JSON object`);
    }
  }
  return { args: forwarded, assessment };
}

async function loadDeliveryProfile(repoRoot, requested) {
  const contract = JSON.parse(await readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8"));
  const profile = requested ?? contract.defaultProfile;
  if (!Object.hasOwn(contract.profiles ?? {}, profile)) throw new Error(`unknown delivery profile: ${profile}`);
  return { contract, profile };
}

export function selectPreMutationExecution(envelope, fastPath) {
  const fallback = fastPath.fallbackReasons;
  const assessment = envelope.preMutationAssessment;
  if (!assessment || typeof assessment !== "object") {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.missingAssessment };
  }
  if (assessment.tier === "T1" || assessment.t1 === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.t1 };
  }
  if (assessment.ambiguous === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.ambiguous };
  }
  if (assessment.dependency === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.dependency };
  }
  if (assessment.workflow === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.workflow };
  }
  if (assessment.release === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.release };
  }
  if (assessment.crossRepository === true) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.crossRepository };
  }
  if (assessment.refined !== fastPath.requiredAssessment.refined) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.notRefined };
  }
  if (assessment.risk !== fastPath.requiredAssessment.risk) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.riskTooHigh };
  }
  if (assessment.scope !== fastPath.requiredAssessment.scope) {
    return { executionProfile: "regular-production-ready", fallbackReason: fallback.scopeTooLarge };
  }
  return { executionProfile: fastPath.executionProfile, fallbackReason: null };
}

export function applyDeliveryProfile(envelope, contract, profile, deliveryTarget) {
  const requiredReads = [...new Set([...envelope.requiredReads, ".pi/delivery-profiles.json"])];
  const routed = {
    ...envelope,
    deliveryBase: deliveryTarget.remoteRef,
    deliveryTargetKind: deliveryTarget.kind,
    deliveryProfile: profile,
    requiredReads,
  };
  if (profile === "production-ready") {
    const fastPath = contract.profiles[profile].preMutationFastPath;
    if (!fastPath) {
      return { ...routed, executionProfile: "regular-production-ready", fallbackReason: "pre-mutation-fast-path-unconfigured" };
    }
    const selection = selectPreMutationExecution(envelope, fastPath);
    return {
      ...routed,
      ...selection,
      preMutationFastPath: {
        maximumToolCallsBeforeOutcome: fastPath.maximumToolCallsBeforeOutcome,
        readPolicy: fastPath.readPolicy,
        preservedGates: fastPath.preservedGates,
      },
      terminalMetrics: fastPath.terminalMetrics,
      nextAction: selection.executionProfile === fastPath.executionProfile
        ? "Complete the scoped required reads, then make the first source mutation or return an evidence-backed blocker before 20 tool calls; retain every production-ready gate."
        : routed.nextAction,
    };
  }
  if (profile !== "prototype") throw new Error(`unsupported delivery profile: ${profile}`);
  if (Object.hasOwn(envelope, "preMutationAssessment")) {
    throw new Error(`${PRE_MUTATION_ASSESSMENT_OPTION} is available only for production-ready delivery`);
  }
  const issueBacked = envelope.target?.kind === "issue"
    || (envelope.target?.kind === "local_phase" && Number.isInteger(envelope.target.issue));
  if (!issueBacked) throw new Error("prototype delivery requires an issue-backed target");

  const prototype = contract.profiles.prototype;
  const criteria = prototype.closeoutFields.map((field) => ({
    id: `prototype-${field.replace(/[A-Z]/gu, (character) => `-${character.toLowerCase()}`)}`,
    must: `Record the prototype ${field} in the provisional closeout.`,
    severity: "required",
  }));
  const profiled = {
    ...routed,
    executionMode: "bounded_handoff",
    currentGate: "default",
    nextAction: "Execute one explicit prototype hypothesis locally, run the bounded focused evidence, and return a provisional closeout.",
    requiredReads,
    stopRules: [...new Set(["remote-mutation", "hosted-ci", "merge-readiness", "merge", ...envelope.stopRules])],
    maxCopilotRounds: 0,
    requireDraftFirst: false,
    acceptance: {
      criteria,
      evidence: ["commands-run", "validation-output", "changed-files", "manual-notes"],
      maxFinalizationTurns: 2,
    },
    control: {
      needsAttentionAfterMs: prototype.sloSeconds.firstFeedback * 1000,
      activeNoticeAfterMs: prototype.sloSeconds.focusedIteration * 1000,
    },
  };
  delete profiled.gateConfig;
  return profiled;
}

export async function resolvePinnedCoreModulePath(packageRoot) {
  const packageManifest = JSON.parse(await readFile(path.join(packageRoot, "package.json"), "utf8"));
  const candidates = [
    path.join(packageRoot, "node_modules", "@dev-loops", "core"),
    path.join(packageRoot, "..", "@dev-loops", "core"),
  ];
  for (const candidate of candidates) {
    try {
      const resolvedRoot = await realpath(candidate);
      if (resolvedRoot !== path.resolve(candidate)) continue;
      const manifest = JSON.parse(await readFile(path.join(resolvedRoot, "package.json"), "utf8"));
      if (manifest.name !== "@dev-loops/core" || manifest.version !== packageManifest.version) continue;
      const modulePath = path.join(resolvedRoot, "src", "loop", "handoff-envelope.mjs");
      if (await realpath(modulePath) !== modulePath) continue;
      return modulePath;
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  throw new Error(`expected @dev-loops/core@${packageManifest.version} beneath the resolved dev-loops package installation`);
}

async function loadPinnedEnvelopeModules(packageRoot) {
  const fromPackage = (relativePath) => import(pathToFileURL(path.join(packageRoot, relativePath)).href);
  const corePath = await resolvePinnedCoreModulePath(packageRoot);
  const [cli, output, helpers, core] = await Promise.all([
    fromPackage(path.join("scripts", "loop", "build-handoff-envelope.mjs")),
    fromPackage(path.join("scripts", "lib", "jq-output.mjs")),
    fromPackage(path.join("scripts", "_core-helpers.mjs")),
    import(pathToFileURL(corePath).href),
  ]);
  return { cli, output, helpers, core };
}

async function runBuildEnvelope(args, { cwd, stdout, stderr, resolved }) {
  const { cli, output, helpers, core } = await loadPinnedEnvelopeModules(resolved.packageRoot);
  let deliveryArgs;
  let assessmentArgs;
  let deliveryTarget;
  try {
    const selected = extractDeliveryTargetOption(args, { required: !args.includes("--help") });
    deliveryTarget = selected.target;
    deliveryArgs = extractDeliveryProfileArgs(selected.args);
    assessmentArgs = extractPreMutationAssessmentArgs(deliveryArgs.args);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  let options;
  try {
    options = cli.parseBuildHandoffEnvelopeCliArgs(assessmentArgs.args);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    const previousExitCode = process.exitCode;
    process.exitCode = undefined;
    await cli.runCli(assessmentArgs.args, {
      stdout,
      stderr,
      adapter: { getCwd: () => cwd, getRepoRoot: () => resolved.gitRoot },
    });
    stdout.write("Repository options:\n  --delivery-base <origin/develop|origin/milestone-x.y.z>  Use the exact target recorded on the issue.\n  --delivery-profile <prototype|production-ready>  Select a bounded delivery profile (default: production-ready).\n  --pre-mutation-assessment <json>  Deterministic refined/low-risk/small-scope assessment for the internal production-ready fast path.\n");
    const code = process.exitCode ?? 0;
    process.exitCode = previousExitCode;
    return code;
  }
  try {
    const candidate = await cli.buildHandoffEnvelopeCli(options, {
      adapter: { getCwd: () => cwd, getRepoRoot: () => resolved.gitRoot },
    });
    const normalized = await normalizeHandoffEnvelopeCwd(candidate, resolved, core);
    const { contract, profile } = await loadDeliveryProfile(resolved.gitRoot, deliveryArgs.requested);
    const profiled = applyDeliveryProfile({
      ...normalized,
      ...(assessmentArgs.assessment === undefined ? {} : { preMutationAssessment: assessmentArgs.assessment }),
    }, contract, profile, deliveryTarget);
    const repositoryAcceptance = applyRepositoryAcceptance(profiled);
    const envelope = await resolveHandoffRequiredReads(repositoryAcceptance, {
      repositoryRoot: repositoryAcceptance.cwd,
      packageRoot: resolved.packageRoot,
    });
    const validation = core.validateHandoffEnvelope(envelope);
    if (!validation.ok) throw new Error(`profiled handoff envelope failed core validation: ${JSON.stringify(validation.errors)}`);
    return output.emitResult(envelope, { jq: options.jq, silent: options.silent, stdout, stderr });
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
}

export function resolveOxidCompatibilityRoute(args) {
  if (args[0] === "gate" && args[1] === "size-budget") {
    return async (routeArgs, runtime) => {
      const { main } = await import("./loop/oxid-size-budget.mjs");
      return main(routeArgs, runtime);
    };
  }
  if (args[0] === "pr" && args[1] === "ready-for-review") {
    return async (routeArgs, runtime) => {
      const { main } = await import("./github/ready-for-review.mjs");
      return main(routeArgs, runtime);
    };
  }
  return null;
}

export async function runDevLoops(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const route = argv.length === 1 && (argv[0] === "--help" || argv[0] === "-h")
    ? {}
    : pinnedPublicRoute(argv);
  // Worktree lifecycle is repository-owned so its consumer provisioning and
  // branch-local delivery metadata remain coupled to the same operation.
  if (route.category === "loop" && route.command === "ensure-worktree") {
    return runEnsureWorktree(routedCommandArgs(argv, "loop", "ensure-worktree"), { cwd, stdout, stderr });
  }
  const args = normalizeDevLoopsArgs(argv);
  const compatibilityRoute = resolveOxidCompatibilityRoute(args);
  if (compatibilityRoute) return compatibilityRoute(args.slice(2), { repoRoot: cwd, stdout, stderr });
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  const envelopeArgs = buildEnvelopeArgs(args);
  if (envelopeArgs) return runBuildEnvelope(envelopeArgs, { cwd, stdout, stderr, resolved });

  const cli = path.join(resolved.packageRoot, "cli", "index.mjs");
  return runManagedChild(process.execPath, [cli, ...args], {
    cwd,
    stdout,
    stderr,
    label: "dev-loops",
  });
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  runDevLoops().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[dev-loops] ${error.message}\n`);
    process.exitCode = 1;
  });
}
