// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { realpath } from "node:fs/promises";
import path from "node:path";

import { resolveDevLoopsPackageRoot } from "./dev-loop-runtime.mjs";

const PACKAGE_SCRIPT_PATTERN = /^scripts\/(?:github|loop)\/[a-z0-9-]+\.mjs$/;

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

/** Run one exact-pinned dev-loops package CLI through a repository-owned seam. */
export async function runDevLoopsPackageScript(
  packageScript,
  argv = process.argv.slice(2),
  { cwd = process.cwd(), env = process.env, nodeCommand = process.execPath, spawn = spawnSync } = {},
) {
  if (!PACKAGE_SCRIPT_PATTERN.test(packageScript)) {
    throw new Error(`invalid dev-loops package script: ${packageScript}`);
  }
  const { packageRoot } = await resolveDevLoopsPackageRoot({ cwd });
  const entry = await realpath(path.join(packageRoot, packageScript));
  if (!isContained(packageRoot, entry)) {
    throw new Error(`dev-loops package script resolves outside the exact package root: ${entry}`);
  }
  const result = spawn(nodeCommand, [entry, ...argv], { cwd, env, stdio: "inherit" });
  if (result.error) throw result.error;
  return result.status ?? 1;
}
