#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import {
  assertMinimumGhVersion,
  assertRepositoryName,
  GITHUB_REST_HEADERS,
  parseGhVersion,
  runGhCommand,
} from "./rest-client.mjs";

/** Probe the exact read-only REST behavior used by repository loop wrappers. */
export function preflightGh({ repository, issue, ghCommand = "gh" }) {
  assertRepositoryName(repository);
  if (!Number.isInteger(issue) || issue < 1) throw new Error("--issue must be a positive integer");

  const run = (args) => runGhCommand(ghCommand, args, { failureLabel: "GitHub CLI REST capability probe" });
  const versionOutput = run(["--version"]);
  const version = assertMinimumGhVersion(parseGhVersion(versionOutput));
  const issueSource = run(["api", ...GITHUB_REST_HEADERS, `repos/${repository}/issues/${issue}`]);
  const timelineSource = run([
    "api", "--paginate", "--slurp", ...GITHUB_REST_HEADERS,
    `repos/${repository}/issues/${issue}/timeline`,
  ]);

  let issueFact;
  let timeline;
  try {
    issueFact = JSON.parse(issueSource);
    timeline = JSON.parse(timelineSource);
  } catch (error) {
    throw new Error(`GitHub CLI REST capability probe returned invalid JSON: ${error.message}`, { cause: error });
  }
  if (Number(issueFact?.number) !== issue) throw new Error("GitHub issue REST probe returned the wrong issue");
  if (!Array.isArray(timeline) || timeline.some((page) => !Array.isArray(page))) {
    throw new Error("GitHub timeline REST probe did not return paginated arrays");
  }

  return { ok: true, version, repository, issue, timelinePages: timeline.length };
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
    stdout.write("Usage: preflight-gh.mjs --repo OWNER/REPO --issue NUMBER\n");
    return;
  }
  const result = preflightGh({ repository: values.repo, issue: Number(values.issue) });
  stdout.write(`${JSON.stringify(result)}\n`);
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[preflight-gh] ${error.message}\n`);
    process.exitCode = 1;
  }
}
