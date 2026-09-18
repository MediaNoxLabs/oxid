#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { validateFollowUpIssue } from "./review-triage.mjs";

export function buildCreateFollowUpArgs({ repo, originPr, title, bodyFile, body, technicalDebt = false }) {
  if (repo !== "MediaNoxLabs/oxid") throw new Error("--repo must be MediaNoxLabs/oxid");
  if (!Number.isSafeInteger(originPr) || originPr < 1) throw new Error("--origin-pr must be a positive integer");
  if (typeof title !== "string" || title.trim().length < 10 || title.length > 160) throw new Error("--title must be 10-160 characters");
  if (typeof bodyFile !== "string" || !path.isAbsolute(bodyFile)) throw new Error("--body-file must be an absolute path");
  const labels = ["factory:follow-up", ...(technicalDebt ? ["technical-debt"] : [])];
  const validation = validateFollowUpIssue({ state: "OPEN", body, labels }, { originPr });
  if (!validation.ok) throw new Error(`follow-up draft ${validation.failures.join("; ")}`);
  return [
    "issue", "create", "--repo", repo, "--title", title.trim(), "--body-file", bodyFile,
    ...labels.flatMap((label) => ["--label", label]),
  ];
}

function parseCli(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" }, "origin-pr": { type: "string" }, title: { type: "string" },
      "body-file": { type: "string" }, "technical-debt": { type: "boolean" },
      execute: { type: "boolean" }, help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  const originPr = Number(values["origin-pr"]);
  const bodyFile = path.resolve(values["body-file"] ?? "");
  const body = readFileSync(bodyFile, "utf8");
  return {
    help: false,
    execute: values.execute === true,
    args: buildCreateFollowUpArgs({
      repo: values.repo,
      originPr,
      title: values.title,
      bodyFile,
      body,
      technicalDebt: values["technical-debt"] === true,
    }),
  };
}

export function cli(argv = process.argv.slice(2), { run = execFileSync, stdout = process.stdout } = {}) {
  const options = parseCli(argv);
  if (options.help) {
    stdout.write("Usage: create-follow-up.mjs --repo MediaNoxLabs/oxid --origin-pr N --title TITLE --body-file /absolute/draft.md [--technical-debt] [--execute]\n");
    return;
  }
  if (!options.execute) {
    stdout.write(`${JSON.stringify({ ok: true, execute: false, command: ["gh", ...options.args] })}\n`);
    return;
  }
  const result = run("gh", options.args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  stdout.write(result);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { cli(); } catch (error) { process.stderr.write(`[create-follow-up] ${error?.stderr?.trim() || error.message}\n`); process.exitCode = 1; }
}
