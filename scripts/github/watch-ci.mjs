#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

import { assertMinimumGhVersion, assertRepositoryName, parseGhVersion, runGhCommand } from "./rest-client.mjs";
import { summarizeCurrentCi } from "../lib/ci-check-selection.mjs";

const MAX_TIMEOUT_MS = 30 * 60 * 1000;
const MAX_POLL_INTERVAL_MS = 5 * 60 * 1000;

function positiveInteger(value, name, { allowZero = false } = {}) {
  const number = Number(value);
  if (!Number.isInteger(number) || number < (allowZero ? 0 : 1)) throw new Error(`${name} must be ${allowZero ? "a non-negative" : "a positive"} integer`);
  return number;
}

export function parseWatchCiArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      repo: { type: "string" }, pr: { type: "string" },
      "timeout-ms": { type: "string", default: String(MAX_TIMEOUT_MS) },
      "poll-interval-ms": { type: "string", default: "60000" },
      help: { type: "boolean", short: "h" },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  assertRepositoryName(values.repo);
  const pr = positiveInteger(values.pr, "--pr");
  const timeoutMs = positiveInteger(values["timeout-ms"], "--timeout-ms", { allowZero: true });
  const pollIntervalMs = positiveInteger(values["poll-interval-ms"], "--poll-interval-ms");
  if (timeoutMs > MAX_TIMEOUT_MS) throw new Error(`--timeout-ms exceeds ${MAX_TIMEOUT_MS}`);
  if (pollIntervalMs > MAX_POLL_INTERVAL_MS) throw new Error(`--poll-interval-ms exceeds ${MAX_POLL_INTERVAL_MS}`);
  return { repo: values.repo, pr, timeoutMs, pollIntervalMs };
}

function jsonGh(ghCommand, args) {
  const output = runGhCommand(ghCommand, args, { failureLabel: "GitHub CI capability request" });
  if (output.trim() === "") throw new Error("GitHub CI capability request returned empty output");
  try { return JSON.parse(output); } catch (error) { throw new Error(`GitHub CI capability request returned malformed JSON: ${error.message}`); }
}

function fetchHead(ghCommand, repo, pr) {
  const payload = jsonGh(ghCommand, ["pr", "view", String(pr), "--repo", repo, "--json", "headRefOid"]);
  if (typeof payload.headRefOid !== "string" || !/^[a-f0-9]{40}$/.test(payload.headRefOid)) throw new Error("GitHub PR response is missing a full headRefOid");
  return payload.headRefOid;
}

function fetchCi(ghCommand, repo, headSha) {
  const checks = jsonGh(ghCommand, ["api", `repos/${repo}/commits/${headSha}/check-runs?per_page=100`]);
  const statuses = jsonGh(ghCommand, ["api", `repos/${repo}/commits/${headSha}/status?per_page=100`]);
  if (!Array.isArray(checks.check_runs) || !Array.isArray(statuses.statuses)) throw new Error("GitHub CI response is missing check_runs or statuses arrays");
  return summarizeCurrentCi({ checkRuns: checks.check_runs, statuses: statuses.statuses });
}

export async function watchCurrentCi(options, { ghCommand = "gh", delayImpl = delay, now = Date.now } = {}) {
  assertMinimumGhVersion(parseGhVersion(runGhCommand(ghCommand, ["--version"], { failureLabel: "GitHub CLI version probe" })));
  const baseline = fetchHead(ghCommand, options.repo, options.pr);
  const started = now();
  let attempts = 0;
  let emptyPolls = 0;
  while (true) {
    attempts += 1;
    const headSha = fetchHead(ghCommand, options.repo, options.pr);
    if (headSha !== baseline) return { ok: true, status: "changed", settled: false, headSha, attempts };
    const state = fetchCi(ghCommand, options.repo, headSha);
    emptyPolls = state.ciStatus === "none" ? emptyPolls + 1 : 0;
    if (state.ciStatus === "failure" || state.ciStatus === "success" || (state.ciStatus === "none" && (options.timeoutMs === 0 || emptyPolls >= 2))) {
      return { ok: true, status: state.ciStatus === "failure" ? "failure" : "success", settled: true, headSha, attempts, ...state };
    }
    const elapsed = now() - started;
    if (elapsed >= options.timeoutMs) return { ok: true, status: options.timeoutMs === 0 ? "pending" : "timeout", settled: false, headSha, attempts, ...state };
    await delayImpl(Math.min(options.pollIntervalMs, options.timeoutMs - elapsed));
  }
}

export async function runCli(argv = process.argv.slice(2), runtime = {}) {
  const options = parseWatchCiArgs(argv);
  if (options.help) {
    process.stdout.write("Usage: dev-loops loop watch-ci --repo OWNER/REPO --pr NUMBER [--timeout-ms N] [--poll-interval-ms N]\n");
    return;
  }
  process.stdout.write(`${JSON.stringify(await watchCurrentCi(options, runtime))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli().catch((error) => {
    process.stderr.write(`[watch-ci] ${error.message}\n`);
    process.exitCode = 1;
  });
}
