#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { contributionPolicy } from "../ci/contribution-policy.mjs";

const TYPE_COLORS = Object.freeze({
  build: "7057ff",
  chore: "e4e669",
  ci: "5319e7",
  docs: "0075ca",
  feat: "1f883d",
  fix: "d73a4a",
  perf: "fbca04",
  refactor: "d4c5f9",
  revert: "b60205",
  style: "cfd3d7",
  test: "0e8a16",
});

export function desiredContributionLabels() {
  return [
    ...contributionPolicy.types.map((type) => ({
      name: `${contributionPolicy.labels.typePrefix}${type}`,
      color: TYPE_COLORS[type],
      description: `Conventional Commit type: ${type}`,
    })),
    ...contributionPolicy.scopes.map((scope) => ({
      name: `${contributionPolicy.labels.scopePrefix}${scope}`,
      color: "bfdadc",
      description: `Primary contribution scope: ${scope}`,
    })),
  ];
}

function run() {
  const execute = process.argv.includes("--execute");
  const repositoryIndex = process.argv.indexOf("--repo");
  const repository = repositoryIndex >= 0 ? process.argv[repositoryIndex + 1] : "MediaNoxLabs/oxid";
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository)) throw new Error("--repo must be OWNER/REPO");
  for (const label of desiredContributionLabels()) {
    process.stdout.write(`${execute ? "sync" : "would sync"} ${label.name}\n`);
    if (execute) {
      execFileSync("gh", ["label", "create", label.name, "--repo", repository, "--color", label.color, "--description", label.description, "--force"], {
        stdio: "inherit",
      });
    }
  }
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) {
  try {
    run();
  } catch (cause) {
    process.stderr.write(`[sync-contribution-labels] ${cause.message}\n`);
    process.exitCode = 1;
  }
}
