#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { validateBranchName } from "../ci/contribution-policy.mjs";
import { assertRepositoryName, runGhCommand } from "./rest-client.mjs";

const OXID_REPOSITORY = "MediaNoxLabs/oxid";

function requireText(value, option) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${option} must be a non-empty string`);
  }
  return value.trim();
}

export function parseCreateReadyPrArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      head: { type: "string" },
      base: { type: "string" },
      assignee: { type: "string" },
      title: { type: "string" },
      "body-file": { type: "string" },
      silent: { type: "boolean", short: "s" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  const repository = values.repo ?? OXID_REPOSITORY;
  assertRepositoryName(repository);
  if (repository.toLowerCase() !== OXID_REPOSITORY.toLowerCase()) {
    throw new Error(`ready PR creation is repository-owned and accepts only ${OXID_REPOSITORY}`);
  }
  const head = requireText(values.head, "--head");
  const validatedHead = validateBranchName(head);
  if (!validatedHead.ok) throw new Error(`--head must be a conventional issue branch: ${validatedHead.errors.join("; ")}`);
  const base = requireText(values.base, "--base");
  const title = requireText(values.title, "--title");
  const bodyFile = requireText(values["body-file"], "--body-file");
  if (readFileSync(bodyFile, "utf8").trim().length === 0) throw new Error("--body-file must not be empty");
  return {
    repository,
    head,
    base,
    title,
    bodyFile,
    ...(values.assignee ? { assignee: requireText(values.assignee, "--assignee") } : {}),
    silent: values.silent ?? false,
  };
}

export function createReadyPr({ repository, head, base, title, bodyFile, assignee, ghCommand = "gh", runGh } = {}) {
  assertRepositoryName(repository);
  const args = [
    "pr", "create", "--repo", repository,
    "--head", head,
    "--base", base,
    "--title", title,
    "--body-file", bodyFile,
  ];
  if (assignee) args.push("--assignee", assignee);
  const run = runGh ?? ((commandArgs) => runGhCommand(ghCommand, commandArgs, { failureLabel: "ready GitHub PR creation" }));
  const url = run(args).trim();
  if (!/^https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/\d+$/u.test(url)) {
    throw new Error("ready GitHub PR creation did not return a pull-request URL");
  }
  return { ok: true, url, draft: false, repository, head, base };
}

export function main(argv = process.argv.slice(2), { stdout = process.stdout, ghCommand = "gh", runGh } = {}) {
  const parsed = parseCreateReadyPrArgs(argv);
  if (parsed.help) {
    stdout.write("Usage: dev-loops pr create --repo MediaNoxLabs/oxid --head BRANCH --base BRANCH --title TITLE --body-file PATH [--assignee LOGIN]\n");
    return 0;
  }
  const result = createReadyPr({ ...parsed, ghCommand, runGh });
  if (!parsed.silent) stdout.write(`${result.url}\n`);
  return 0;
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (error) {
    process.stderr.write(`[create-ready-pr] ${error.message}\n`);
    process.exitCode = 1;
  }
}
