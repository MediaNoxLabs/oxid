#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { lstat, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

export const INTEGRATION_BLOB_PREFIX = "https://github.com/MediaNoxLabs/oxid/blob/integration/";

function markdownWithoutCode(markdown) {
  let fence = null;
  const lines = [];
  for (const line of markdown.split("\n")) {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/u)?.[1];
    if (marker && !fence) {
      fence = marker;
      lines.push("");
    } else if (marker && fence && marker[0] === fence[0] && marker.length >= fence.length) {
      fence = null;
      lines.push("");
    } else {
      lines.push(fence ? "" : line.replace(/(`+)[^`\n]*\1/gu, ""));
    }
  }
  return lines.join("\n");
}

export function candidateIntegrationUrls(markdown) {
  const pattern = /https:\/\/github\.com\/MediaNoxLabs\/oxid\/blob\/integration\/[^\s<>"'`)\]]+/g;
  return (markdownWithoutCode(markdown).match(pattern) ?? []).map((url) => url.replace(/[.,;:!]+$/u, ""));
}

export function candidatePath(rawUrl) {
  if (!rawUrl.startsWith(INTEGRATION_BLOB_PREFIX)) throw new Error(`not a candidate integration URL: ${rawUrl}`);
  const suffix = rawUrl.slice(INTEGRATION_BLOB_PREFIX.length).split(/[?#]/u, 1)[0];
  if (!suffix || suffix.includes("%") || suffix.includes("\\")) {
    throw new Error(`ambiguous candidate integration URL: ${rawUrl}`);
  }
  const segments = suffix.split("/");
  if (segments.some((segment) => !segment || segment === "." || segment === "..")) {
    throw new Error(`unsafe candidate integration URL: ${rawUrl}`);
  }
  return suffix;
}

export function buildLycheeArgs(repoRoot) {
  return [
    "--config", ".lychee.toml",
    "--no-progress",
    "--exclude-path", "LICENSE",
    "--remap", `${INTEGRATION_BLOB_PREFIX} ${pathToFileURL(`${repoRoot}${path.sep}`).href}`,
    "./**/*.md",
  ];
}

async function trackedFiles(repoRoot) {
  const result = spawnSync("git", ["ls-files", "-z"], {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`git ls-files failed: ${result.stderr?.trim() ?? "unknown error"}`);
  return result.stdout.split("\0").filter(Boolean);
}

export async function validateCandidateLinks(repoRoot, markdownPaths = undefined) {
  const tracked = new Set(await trackedFiles(repoRoot));
  const documents = markdownPaths ?? [...tracked].filter((file) => file.endsWith(".md"));
  for (const document of documents) {
    const markdown = await readFile(path.join(repoRoot, document), "utf8");
    for (const rawUrl of candidateIntegrationUrls(markdown)) {
      const candidate = candidatePath(rawUrl);
      if (!tracked.has(candidate)) throw new Error(`candidate integration link target is not tracked: ${candidate}`);
      const target = path.resolve(repoRoot, candidate);
      const relative = path.relative(repoRoot, target);
      if (!relative || relative.startsWith("..") || path.isAbsolute(relative)) {
        throw new Error(`candidate integration link escapes the repository: ${candidate}`);
      }
      const metadata = await lstat(target);
      if (!metadata.isFile() || metadata.isSymbolicLink()) {
        throw new Error(`candidate integration link target is not a regular file: ${candidate}`);
      }
    }
  }
}

export async function main() {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  await validateCandidateLinks(repoRoot);
  const result = spawnSync("lychee", buildLycheeArgs(repoRoot), { cwd: repoRoot, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.signal) throw new Error(`lychee terminated by ${result.signal}`);
  process.exitCode = result.status ?? 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`[docs-links] ${error.message}\n`);
    process.exitCode = 1;
  });
}
