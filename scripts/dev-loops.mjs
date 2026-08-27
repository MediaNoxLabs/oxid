#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";
import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { normalizeHandoffEnvelopeCwd } from "./lib/handoff-envelope-cwd.mjs";
import { resolveDevLoopsPackageRoot } from "./lib/dev-loop-runtime.mjs";
import { enforceSingleBase, pinnedPublicRoute } from "./lib/pinned-dev-loops-args.mjs";

const INTEGRATION_BASE = "integration";

/** Force the base on dev-loops' public PR create route and deprecated alias. */
export function normalizeDevLoopsArgs(argv) {
  if (argv.length === 1 && (argv[0] === "--help" || argv[0] === "-h")) return ["help"];
  const args = [...argv];
  const route = pinnedPublicRoute(args);
  const isPrCreate = route.category === "pr" && (route.command === "create" || route.command === "create-draft");
  return enforceSingleBase(args, INTEGRATION_BASE, {
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
  let options;
  try {
    options = cli.parseBuildHandoffEnvelopeCliArgs(args);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    const previousExitCode = process.exitCode;
    process.exitCode = undefined;
    await cli.runCli(args, {
      stdout,
      stderr,
      adapter: { getCwd: () => cwd, getRepoRoot: () => resolved.gitRoot },
    });
    const code = process.exitCode ?? 0;
    process.exitCode = previousExitCode;
    return code;
  }
  try {
    const candidate = await cli.buildHandoffEnvelopeCli(options, {
      adapter: { getCwd: () => cwd, getRepoRoot: () => resolved.gitRoot },
    });
    const envelope = await normalizeHandoffEnvelopeCwd(candidate, resolved, core);
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
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cli, ...args], { cwd, stdio: ["inherit", "pipe", "pipe"] });
    child.stdout.pipe(stdout, { end: false });
    child.stderr.pipe(stderr, { end: false });
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (signal) reject(new Error(`dev-loops terminated by ${signal}`));
      else resolve(code ?? 1);
    });
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
