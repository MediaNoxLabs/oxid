// SPDX-License-Identifier: Apache-2.0

import { realpath, stat } from "node:fs/promises";
import path from "node:path";

const PACKAGE_REQUIRED_READ_PREFIXES = new Set(["skills"]);

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

function validatePortableRequiredRead(logicalPath) {
  if (typeof logicalPath !== "string" || logicalPath.trim() !== logicalPath || logicalPath.length === 0) {
    throw new Error("handoff requiredReads entries must be non-empty strings without surrounding whitespace");
  }
  if (logicalPath.includes("\\") || path.isAbsolute(logicalPath) || path.win32.isAbsolute(logicalPath)) {
    throw new Error(`handoff required read must be a portable relative path: ${logicalPath}`);
  }
  if (path.posix.normalize(logicalPath) !== logicalPath || logicalPath.split("/").some((segment) => segment === "" || segment === "." || segment === "..")) {
    throw new Error(`handoff required read contains traversal or ambiguous segments: ${logicalPath}`);
  }
  return logicalPath;
}

async function resolveRequiredReadRoot(candidate, owner) {
  try {
    const root = await realpath(candidate);
    if (!(await stat(root)).isDirectory()) throw new Error("not a directory");
    return root;
  } catch (error) {
    throw new Error(
      `handoff ${owner} read root is unavailable: ${candidate}. Create or reuse the canonical worktree and prepare its exact package closure before dispatch.`,
      { cause: error },
    );
  }
}

/** Resolve every handoff read before provider dispatch; consumers never guess roots. */
export async function resolveHandoffRequiredReads(envelope, { repositoryRoot, packageRoot }) {
  const roots = {
    repository: await resolveRequiredReadRoot(repositoryRoot, "repository"),
    package: await resolveRequiredReadRoot(packageRoot, "package"),
  };
  const entries = [];
  const seen = new Set();
  for (const candidate of envelope.requiredReads) {
    const logicalPath = validatePortableRequiredRead(candidate);
    const owner = PACKAGE_REQUIRED_READ_PREFIXES.has(logicalPath.split("/", 1)[0]) ? "package" : "repository";
    const requestedPath = path.resolve(roots[owner], logicalPath);
    if (!isContained(roots[owner], requestedPath)) {
      throw new Error(`handoff required read escapes its ${owner} root: ${logicalPath}`);
    }
    let resolvedPath;
    try {
      resolvedPath = await realpath(requestedPath);
    } catch (error) {
      if (error?.code === "ENOENT" || error?.code === "ENOTDIR") {
        throw new Error(`handoff required read is missing from its ${owner} root: ${logicalPath}`);
      }
      throw error;
    }
    if (!isContained(roots[owner], resolvedPath)) {
      throw new Error(`handoff required read resolves outside its ${owner} root: ${logicalPath}`);
    }
    if (!(await stat(resolvedPath)).isFile()) throw new Error(`handoff required read is not a file: ${logicalPath}`);
    if (seen.has(resolvedPath)) throw new Error(`handoff required read resolves ambiguously: ${logicalPath}`);
    seen.add(resolvedPath);
    entries.push({ logicalPath, owner, root: roots[owner], resolvedPath });
  }
  return {
    ...envelope,
    requiredReads: entries.map(({ resolvedPath }) => resolvedPath),
    requiredReadManifest: { schemaVersion: 1, roots, entries },
  };
}

/** Replace the upstream Node-workspace default with Oxid's repository target plan. */
export function applyRepositoryAcceptance(envelope) {
  return {
    ...envelope,
    acceptance: {
      ...envelope.acceptance,
      criteria: envelope.acceptance.criteria.map((criterion) => criterion.id === "verify-green" && /npm run verify/u.test(criterion.must)
        ? {
          ...criterion,
          must: "The Oxid target plan selected for this change passes through its sanctioned Cargo, Just, Nix, or focused platform commands.",
        }
        : criterion),
    },
  };
}
