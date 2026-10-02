#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resolveCanonicalGithubRepository } from "../dev-loops.mjs";
import { runDevLoopsPackageScript } from "../lib/dev-loop-package-script.mjs";

/** Bind issue reads to the checkout origin before the package may contact GitHub. */
export function bindIssueReadRepository(argv, originRepository) {
  if (argv.includes("--help") || argv.includes("-h") || !originRepository) return argv;
  const repositories = [];
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--repo") repositories.push(argv[++index]);
    else if (argv[index].startsWith("--repo=")) repositories.push(argv[index].slice("--repo=".length));
  }
  if (repositories.length > 1 || repositories.some((repository) => !repository)) {
    throw new Error("issue read accepts at most one nonempty --repo");
  }
  if (repositories.length === 1 && repositories[0].toLowerCase() !== originRepository.toLowerCase()) {
    throw new Error(`issue read repository ${repositories[0]} disagrees with origin repository ${originRepository}; use --repo ${originRepository}`);
  }
  return repositories.length === 0 ? [...argv, "--repo", originRepository] : argv;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const repository = resolveCanonicalGithubRepository(process.cwd());
    process.exitCode = await runDevLoopsPackageScript(
      "scripts/github/view-issue.mjs", bindIssueReadRepository(process.argv.slice(2), repository),
    );
  } catch (error) {
    process.stderr.write(`[view-issue] ${error.message}\n`);
    process.exitCode = 1;
  }
}
