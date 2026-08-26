#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

export const MINIMUM_GH_VERSION = [2, 67, 0];

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

function flattenPages(value) {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => Array.isArray(entry) ? flattenPages(entry) : [entry]);
}

function repositoryFromUrl(url) {
  if (typeof url !== "string") return undefined;
  const match = url.match(/^https:\/\/(?:api\.)?github\.com\/(?:repos\/)?([^/]+)\/([^/]+)\/(?:pulls?|issues)\/\d+(?:$|[/?#])/i);
  return match ? `${match[1]}/${match[2]}` : undefined;
}

function pullRequestFromTimelineEvent(event) {
  if (!event || typeof event !== "object") return undefined;
  const issue = event.event === "cross-referenced" ? event.source?.issue : event.subject;
  if (!issue || typeof issue !== "object" || !issue.pull_request) return undefined;
  const number = Number(issue.number);
  if (!Number.isInteger(number) || number < 1) return undefined;
  const url = issue.html_url;
  const apiUrl = issue.pull_request.url;
  return { number, url, apiUrl, event: event.event };
}

/** Normalize paginated issue-timeline events into same-repository PR facts. */
export function normalizeTimelinePullRequests(pages, repository) {
  const expectedRepository = repository.toLowerCase();
  const links = new Map();
  for (const event of flattenPages(pages)) {
    const link = pullRequestFromTimelineEvent(event);
    if (!link) continue;
    const linkedRepository = repositoryFromUrl(link.apiUrl) ?? repositoryFromUrl(link.url);
    if (!linkedRepository || linkedRepository.toLowerCase() !== expectedRepository) continue;
    if (!links.has(link.number)) links.set(link.number, link);
  }
  return [...links.values()].sort((left, right) => left.number - right.number);
}

function runGh(ghCommand, args, options = {}) {
  try {
    return execFileSync(ghCommand, args, { encoding: "utf8", timeout: 120_000, stdio: ["ignore", "pipe", "pipe"], ...options });
  } catch (error) {
    const diagnostic = String(error?.stderr ?? error?.message ?? "GitHub CLI failed").trim();
    throw new Error(`GitHub REST request failed: ${diagnostic}`, { cause: error });
  }
}

export function bodyClosesIssue(body, issue) {
  if (typeof body !== "string") return false;
  const escaped = String(issue).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\\s+#${escaped}(?:\\b|$)`, "i").test(body);
}

export function resolveIssuePullRequestLinks({ repository, issue, ghCommand = "gh" }) {
  if (!/^(?!\.{1,2}\/)(?!.*\/\.{1,2}$)[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? "")) throw new Error("--repo must be OWNER/REPO");
  if (!Number.isInteger(issue) || issue < 1) throw new Error("--issue must be a positive integer");
  assertMinimumGhVersion(parseGhVersion(runGh(ghCommand, ["--version"])));
  const endpoint = `repos/${repository}/issues/${issue}/timeline`;
  const source = runGh(ghCommand, [
    "api", "--paginate", "--slurp",
    "-H", "Accept: application/vnd.github+json",
    "-H", "X-GitHub-Api-Version: 2022-11-28",
    endpoint,
  ]);
  let pages;
  try {
    pages = JSON.parse(source);
  } catch (error) {
    throw new Error(`GitHub timeline returned invalid JSON: ${error.message}`, { cause: error });
  }
  const links = normalizeTimelinePullRequests(pages, repository);
  return links.flatMap((link) => {
    const source = runGh(ghCommand, ["api", "-H", "Accept: application/vnd.github+json", "-H", "X-GitHub-Api-Version: 2022-11-28", `repos/${repository}/pulls/${link.number}`]);
    let pull;
    try { pull = JSON.parse(source); } catch (error) { throw new Error(`GitHub pull REST response was invalid JSON: ${error.message}`, { cause: error }); }
    if (!bodyClosesIssue(pull.body, issue)) return [];
    return [{ ...link, state: pull.state, draft: pull.draft === true, mergedAt: pull.merged_at ?? null, baseRefName: pull.base?.ref ?? null, headRefName: pull.head?.ref ?? null }];
  });
}

export function runCli(argv = process.argv.slice(2), { stdout = process.stdout } = {}) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      issue: { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) {
    stdout.write("Usage: resolve-issue-pr-links.mjs --repo OWNER/REPO --issue NUMBER\n");
    return;
  }
  const issue = Number(values.issue);
  const result = resolveIssuePullRequestLinks({ repository: values.repo, issue });
  stdout.write(`${JSON.stringify({ ok: true, repository: values.repo, issue, pullRequests: result })}\n`);
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[resolve-issue-pr-links] ${error.message}\n`);
    process.exitCode = 1;
  }
}
