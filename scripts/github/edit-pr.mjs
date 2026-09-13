#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { assertRepositoryName, GITHUB_REST_HEADERS, runGhCommand } from "./rest-client.mjs";

function parsePrNumber(value) {
  if (!/^[1-9]\d*$/u.test(value ?? "")) throw new Error("--pr must be a positive integer");
  const pr = Number(value);
  if (!Number.isSafeInteger(pr)) throw new Error("--pr must be a positive integer");
  return pr;
}

export function parseEditPrArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" },
      pr: { type: "string" },
      body: { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  assertRepositoryName(values.repo);
  const body = values.body;
  if (typeof body !== "string" || body.trim().length === 0) {
    throw new Error("--body must be a non-empty string");
  }
  return { repository: values.repo, pr: parsePrNumber(values.pr), body };
}

export function editPrBody({ repository, pr, body, ghCommand = "gh", runGh } = {}) {
  assertRepositoryName(repository);
  if (!Number.isSafeInteger(pr) || pr < 1) throw new Error("--pr must be a positive integer");
  if (typeof body !== "string" || body.trim().length === 0) throw new Error("--body must be a non-empty string");
  const run = runGh ?? ((args) => runGhCommand(ghCommand, args, { failureLabel: "GitHub PR body update" }));
  run([
    "api", "--method", "PATCH", ...GITHUB_REST_HEADERS,
    `repos/${repository}/pulls/${pr}`,
    "-f", `body=${body}`,
  ]);
  return { ok: true, repository, pr, edited: ["body"] };
}

export function main(argv = process.argv.slice(2), { stdout = process.stdout, ghCommand = "gh", runGh } = {}) {
  const parsed = parseEditPrArgs(argv);
  if (parsed.help) {
    stdout.write("Usage: dev-loops pr edit --repo OWNER/REPO --pr NUMBER --body TEXT\n");
    return 0;
  }
  stdout.write(`${JSON.stringify(editPrBody({ ...parsed, ghCommand, runGh }))}\n`);
  return 0;
}

function isDirectRun(metaUrl) {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(metaUrl);
}

if (isDirectRun(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (error) {
    process.stderr.write(`[edit-pr] ${error.message}\n`);
    process.exitCode = 1;
  }
}
