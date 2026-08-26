#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import {
  assertMinimumGhVersion,
  assertRepositoryName,
  GITHUB_REST_HEADERS,
  isRepositoryName,
  parseGhVersion,
  runGhCommand,
} from "./rest-client.mjs";

export { assertMinimumGhVersion, parseGhVersion } from "./rest-client.mjs";

function flattenPages(value) {
  return value.flat();
}

export function assertTimelinePages(value) {
  if (!Array.isArray(value) || value.some((page) => !Array.isArray(page))) {
    throw new Error("GitHub timeline REST response did not return paginated arrays");
  }
  return value;
}

function repositoryFromUrl(url) {
  if (typeof url !== "string") return undefined;
  const match = url.match(/^https:\/\/(?:api\.)?github\.com\/(?:repos\/)?([^/]+)\/([^/]+)\/(?:pulls?|issues)\/\d+(?:$|[/?#])/i);
  return match ? `${match[1]}/${match[2]}` : undefined;
}

function pullRequestFromTimelineEvent(event) {
  if (!event || typeof event !== "object" || event.event !== "cross-referenced") return undefined;
  const issue = event.source?.issue;
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

function issueReferencePattern(issue, repository) {
  const escapedIssue = String(issue).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const references = [`#${escapedIssue}`, `GH-${escapedIssue}`];
  if (isRepositoryName(repository)) {
    const escapedRepository = repository.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    references.push(
      `${escapedRepository}#${escapedIssue}`,
      `https:\/\/github\\.com\/${escapedRepository}\/issues\/${escapedIssue}`,
    );
  }
  return `(?:${references.join("|")})(?:\\b|$)`;
}

export function bodyReferencesIssue(body, issue, repository) {
  if (typeof body !== "string") return false;
  return new RegExp(`(?:refs?|references?|close[sd]?|fix(?:e[sd])?|resolve[sd]?)\\s+${issueReferencePattern(issue, repository)}`, "i").test(body);
}

export function resolveIssuePullRequestLinks({ repository, issue, ghCommand = "gh" }) {
  assertRepositoryName(repository);
  if (!Number.isInteger(issue) || issue < 1) throw new Error("--issue must be a positive integer");
  const runGh = (args) => runGhCommand(ghCommand, args);
  assertMinimumGhVersion(parseGhVersion(runGh(["--version"])));
  const endpoint = `repos/${repository}/issues/${issue}/timeline`;
  const source = runGh([
    "api", "--paginate", "--slurp", ...GITHUB_REST_HEADERS, endpoint,
  ]);
  let pages;
  try {
    pages = JSON.parse(source);
  } catch (error) {
    throw new Error(`GitHub timeline returned invalid JSON: ${error.message}`, { cause: error });
  }
  assertTimelinePages(pages);
  const links = normalizeTimelinePullRequests(pages, repository);
  return links.flatMap((link) => {
    const source = runGh(["api", ...GITHUB_REST_HEADERS, `repos/${repository}/pulls/${link.number}`]);
    let pull;
    try { pull = JSON.parse(source); } catch (error) { throw new Error(`GitHub pull REST response was invalid JSON: ${error.message}`, { cause: error }); }
    if (!bodyReferencesIssue(pull.body, issue, repository)) return [];
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
