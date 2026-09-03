#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { realpathSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

export const REVIEWED_WORKTREE_PIN = "0.9.0";

export function assertReviewedWorktreePin(version) {
  if (version !== REVIEWED_WORKTREE_PIN) {
    throw new Error(
      `Oxid consumer provisioning supports only reviewed dev-loops@${REVIEWED_WORKTREE_PIN}; review the worktree injection contract before using ${version}`,
    );
  }
}

/** Oxid resolves exact Pi packages from the common checkout; no worktree files are provisioned. */
export function oxidConsumerProvision() {
  return {
    ok: true,
    actions: [],
    summary: { copied: 0, linked: 0, skipped: 0, rejected: 0, warnings: 0 },
  };
}

async function loadPinnedWorktreeModules(packageRoot) {
  const fromPackage = (relativePath) => import(pathToFileURL(path.join(packageRoot, relativePath)).href);
  let cli;
  let output;
  let helpers;
  try {
    [cli, output, helpers] = await Promise.all([
      fromPackage(path.join("scripts", "loop", "ensure-worktree.mjs")),
      fromPackage(path.join("scripts", "lib", "jq-output.mjs")),
      fromPackage(path.join("scripts", "_core-helpers.mjs")),
    ]);
  } catch (error) {
    throw new Error(`reviewed dev-loops@${REVIEWED_WORKTREE_PIN} worktree module layout is unavailable: ${error.message}`, { cause: error });
  }
  for (const [name, value] of [
    ["parseEnsureWorktreeCliArgs", cli.parseEnsureWorktreeCliArgs],
    ["ensureWorktree", cli.ensureWorktree],
    ["runCli", cli.runCli],
    ["emitResult", output.emitResult],
    ["formatCliError", helpers.formatCliError],
  ]) {
    if (typeof value !== "function") {
      throw new Error(`reviewed dev-loops@${REVIEWED_WORKTREE_PIN} worktree contract is missing ${name}`);
    }
  }
  return { cli, output, helpers };
}

export async function runConsumerEnsureWorktree(argv = process.argv.slice(2), {
  cwd = process.cwd(),
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const resolved = await resolveDevLoopsPackageRoot({ cwd });
  assertReviewedWorktreePin(resolved.version);
  const { cli, output, helpers } = await loadPinnedWorktreeModules(resolved.packageRoot);
  let options;
  try {
    options = cli.parseEnsureWorktreeCliArgs(argv);
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
  if (options.help) {
    const previousExitCode = process.exitCode;
    process.exitCode = undefined;
    await cli.runCli(argv, { stdout, stderr });
    const code = process.exitCode ?? 0;
    process.exitCode = previousExitCode;
    return code;
  }
  try {
    const result = await cli.ensureWorktree(options, { provision: oxidConsumerProvision });
    return output.emitResult(result, { jq: options.jq, silent: options.silent, stdout, stderr });
  } catch (error) {
    stderr.write(`${helpers.formatCliError(error)}\n`);
    return 1;
  }
}

function isDirectRun(metaUrl) {
  return process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(metaUrl));
}

if (isDirectRun(import.meta.url)) {
  runConsumerEnsureWorktree().then((code) => {
    process.exitCode = code;
  }).catch((error) => {
    process.stderr.write(`[ensure-worktree] ${error.message}\n`);
    process.exitCode = 1;
  });
}
