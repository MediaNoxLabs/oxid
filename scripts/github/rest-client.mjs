// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";

export const MINIMUM_GH_VERSION = [2, 67, 0];
export const GITHUB_REST_HEADERS = [
  "-H", "Accept: application/vnd.github+json",
  "-H", "X-GitHub-Api-Version: 2022-11-28",
];

const REPOSITORY_PATTERN = /^(?!\.{1,2}\/)(?!.*\/\.{1,2}$)[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

export function isRepositoryName(value) {
  return typeof value === "string" && REPOSITORY_PATTERN.test(value);
}

export function assertRepositoryName(value) {
  if (!isRepositoryName(value)) throw new Error("--repo must be OWNER/REPO");
  return value;
}

export function parseGhVersion(output) {
  const match = String(output).match(/(?:^|\n)gh version (\d+)\.(\d+)\.(\d+)(?:\s|$)/);
  if (!match) throw new Error("could not parse GitHub CLI version");
  return match.slice(1).map(Number);
}

export function assertMinimumGhVersion(version, minimum = MINIMUM_GH_VERSION) {
  if (!Array.isArray(version) || version.length !== 3 || version.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error("GitHub CLI version must be a semantic version triple");
  }
  for (let index = 0; index < 3; index += 1) {
    if (version[index] > minimum[index]) return version;
    if (version[index] < minimum[index]) {
      throw new Error(`GitHub CLI ${version.join(".")} is unsupported; require >= ${minimum.join(".")}`);
    }
  }
  return version;
}

export function runGhCommand(ghCommand, args, { failureLabel = "GitHub REST request", ...options } = {}) {
  try {
    return execFileSync(ghCommand, args, {
      encoding: "utf8",
      timeout: 120_000,
      stdio: ["ignore", "pipe", "pipe"],
      ...options,
    });
  } catch (error) {
    const diagnostic = String(error?.stderr ?? error?.message ?? "GitHub CLI failed").trim();
    throw new Error(`${failureLabel} failed: ${diagnostic}`, { cause: error });
  }
}
