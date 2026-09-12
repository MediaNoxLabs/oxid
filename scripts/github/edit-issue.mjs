#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { assertRepositoryName, GITHUB_REST_HEADERS, runGhCommand } from "./rest-client.mjs";

function parseIssueNumber(value) {
  if (!/^[1-9]\d*$/u.test(value ?? "")) throw new Error("--issue must be a positive integer");
  const issue = Number(value);
  if (!Number.isSafeInteger(issue)) throw new Error("--issue must be a positive integer");
  return issue;
}

export function parseEditIssueArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      issue: { type: "string" },
      "add-assignee": { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  assertRepositoryName(values.repo);
  const issue = parseIssueNumber(values.issue);
  if (values["add-assignee"] !== "@me") throw new Error("this facade only supports `--add-assignee @me`");
  return { repository: values.repo, issue, addAssignee: "@me" };
}

function authenticatedLogin(runGh) {
  let user;
  try {
    user = JSON.parse(runGh(["api", ...GITHUB_REST_HEADERS, "user"]));
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error("GitHub authentication probe returned invalid JSON", { cause: error });
    throw error;
  }
  if (typeof user?.login !== "string" || user.login.length === 0) {
    throw new Error("GitHub authentication probe returned no authenticated GitHub login");
  }
  return user.login;
}

export function editIssue({ repository, issue, addAssignee, ghCommand = "gh", runGh } = {}) {
  assertRepositoryName(repository);
  if (!Number.isSafeInteger(issue) || issue < 1) throw new Error("--issue must be a positive integer");
  if (addAssignee !== "@me") throw new Error("this facade only supports `--add-assignee @me`");
  const run = runGh ?? ((args) => runGhCommand(ghCommand, args, { failureLabel: "GitHub issue claim" }));
  const login = authenticatedLogin(run);
  run([
    "api", "--method", "POST", ...GITHUB_REST_HEADERS,
    `repos/${repository}/issues/${issue}/assignees`,
    "-f", `assignees[]=${login}`,
  ]);
  return { ok: true, repository, issue, assignee: login };
}

export function runCli(argv = process.argv.slice(2), { stdout = process.stdout, ghCommand = "gh", runGh } = {}) {
  const parsed = parseEditIssueArgs(argv);
  if (parsed.help) {
    stdout.write("Usage: edit-issue.mjs --repo OWNER/REPO --issue NUMBER --add-assignee @me\n");
    return;
  }
  stdout.write(`${JSON.stringify(editIssue({ ...parsed, ghCommand, runGh }))}\n`);
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[edit-issue] ${error.message}\n`);
    process.exitCode = 1;
  }
}
