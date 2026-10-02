// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { realpath } from "node:fs/promises";
import path from "node:path";

import { resolveCanonicalGithubRepository } from "../dev-loops.mjs";
import { resolveDevLoopsPackageRoot } from "./dev-loop-runtime.mjs";

const PACKAGE_SCRIPT_PATTERN = /^scripts\/(?:github|loop)\/[a-z0-9-]+\.mjs$/;

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

/** Every sanctioned package GitHub command uses the checkout's exact origin. */
export function bindPackageGithubRepository(argv, originRepository) {
  if (argv.includes("--help") || argv.includes("-h")) return argv;
  const repositories = [];
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--repo") repositories.push(argv[++index]);
    else if (argv[index].startsWith("--repo=")) repositories.push(argv[index].slice("--repo=".length));
  }
  if (repositories.length > 1 || repositories.some((repository) => !repository)) {
    throw new Error("GitHub command accepts at most one nonempty --repo");
  }
  if (!originRepository) {
    if (repositories.length === 0) throw new Error("GitHub command needs an exact origin or explicit --repo");
    return argv;
  }
  if (repositories.length === 1 && repositories[0].toLowerCase() !== originRepository.toLowerCase()) {
    throw new Error(`GitHub command repository ${repositories[0]} disagrees with origin repository ${originRepository}`);
  }
  return repositories.length === 0 ? [...argv, "--repo", originRepository] : argv;
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
  let boundArgv;
  try {
    boundArgv = packageScript.startsWith("scripts/github/")
      ? bindPackageGithubRepository(argv, resolveCanonicalGithubRepository(cwd))
      : argv;
  } catch (error) {
    process.stderr.write(`[${path.basename(packageScript)}] ${error.message}\n`);
    return 1;
  }
  const { packageRoot } = await resolveDevLoopsPackageRoot({ cwd });
  const entry = await realpath(path.join(packageRoot, packageScript));
  if (!isContained(packageRoot, entry)) {
    throw new Error(`dev-loops package script resolves outside the exact package root: ${entry}`);
  }
  const result = spawn(nodeCommand, [entry, ...boundArgv], { cwd, env, stdio: "inherit" });
  if (result.error) throw result.error;
  return result.status ?? 1;
}
