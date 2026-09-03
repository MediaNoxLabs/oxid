#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { normalizeHandoffEnvelopeCwd } from "./lib/handoff-envelope-cwd.mjs";
import { runManagedChild } from "./lib/managed-child-process.mjs";
import { resolveDevLoopsPackageRoot } from "./lib/dev-loop-runtime.mjs";
import { enforceSingleBase, pinnedPublicRoute } from "./lib/pinned-dev-loops-args.mjs";

const DELIVERY_BASE = "develop";
const DELIVERY_PROFILE_OPTION = "--delivery-profile";

/** Force the base on dev-loops' public PR create route and deprecated alias. */
export function normalizeDevLoopsArgs(argv) {
  if (argv.length === 1 && (argv[0] === "--help" || argv[0] === "-h")) return ["help"];
  const args = [...argv];
  const route = pinnedPublicRoute(args);
  const isPrCreate = route.category === "pr" && (route.command === "create" || route.command === "create-draft");
  return enforceSingleBase(args, DELIVERY_BASE, {
    addWhenMissing: isPrCreate,
    label: "repository dev-loops operations",
  });
}

function buildEnvelopeArgs(args) {
  const route = pinnedPublicRoute(args);
  if (route.category !== "loop" || route.command !== "build-envelope") return null;
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

async function loadDeliveryProfile(repoRoot, requested) {
  const contract = JSON.parse(await readFile(path.join(repoRoot, ".pi", "delivery-profiles.json"), "utf8"));
  const profile = requested ?? contract.defaultProfile;
  if (!Object.hasOwn(contract.profiles ?? {}, profile)) throw new Error(`unknown delivery profile: ${profile}`);
  return { contract, profile };
}

export function applyDeliveryProfile(envelope, contract, profile) {
  const requiredReads = [...new Set([...envelope.requiredReads, ".pi/delivery-profiles.json"])];
  if (profile === "production-ready") return { ...envelope, deliveryProfile: profile, requiredReads };
  if (profile !== "prototype") throw new Error(`unsupported delivery profile: ${profile}`);
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
    ...envelope,
    deliveryProfile: profile,
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
  try {
    deliveryArgs = extractDeliveryProfileArgs(args);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  let options;
  try {
    options = cli.parseBuildHandoffEnvelopeCliArgs(deliveryArgs.args);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    const previousExitCode = process.exitCode;
    process.exitCode = undefined;
    await cli.runCli(deliveryArgs.args, {
      stdout,
      stderr,
      adapter: { getCwd: () => cwd, getRepoRoot: () => resolved.gitRoot },
    });
    stdout.write("Repository option:\n  --delivery-profile <prototype|production-ready>  Select a bounded delivery profile (default: production-ready).\n");
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
    const envelope = applyDeliveryProfile(normalized, contract, profile);
    const validation = core.validateHandoffEnvelope(envelope);
    if (!validation.ok) throw new Error(`profiled handoff envelope failed core validation: ${JSON.stringify(validation.errors)}`);
    return output.emitResult(envelope, { jq: options.jq, silent: options.silent, stdout, stderr });
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
}

export async function runDevLoops(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const args = normalizeDevLoopsArgs(argv);
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
