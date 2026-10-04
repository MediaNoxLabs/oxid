#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parseArgs } from "node:util";

const REPOSITORY = "MediaNoxLabs/oxid";
const MILESTONE = /^milestone-(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/u;
const SHA = /^[0-9a-f]{40}$/u;
const REQUIRED_JOBS = ["Hermetic nix flake check (full sandboxed test suite)", "Scorecard analysis"];

export function validateReleaseQualification(run, { branch, sha }) {
  const failures = [];
  if (!MILESTONE.test(branch ?? "")) failures.push("branch must be milestone-<x.y.z>");
  if (!SHA.test(sha ?? "")) failures.push("expected head SHA is malformed");
  if (run?.event !== "workflow_dispatch") failures.push("qualification must be a deliberate workflow_dispatch run");
  if (run?.head_branch !== branch) failures.push("qualification branch does not match the milestone");
  if (run?.head_sha !== sha) failures.push("qualification head SHA does not match the selected milestone head");
  if (run?.status !== "completed" || run?.conclusion !== "success") failures.push("qualification workflow did not complete successfully");
  const jobs = Array.isArray(run?.jobs) ? run.jobs : [];
  for (const name of REQUIRED_JOBS) {
    const matching = jobs.filter((job) => job?.name === name);
    if (matching.length !== 1 || matching[0].status !== "completed" || matching[0].conclusion !== "success") {
      failures.push(`${name} is not a successful completed job`);
    }
  }
  const artifacts = Array.isArray(run?.artifacts) ? run.artifacts : [];
  const matchingArtifacts = artifacts.filter((artifact) => artifact?.name === `scorecard-${sha}`);
  if (matchingArtifacts.length !== 1 || matchingArtifacts[0].expired !== false
    || !Number.isSafeInteger(matchingArtifacts[0].size_in_bytes) || matchingArtifacts[0].size_in_bytes <= 0) {
    failures.push("exact-head Scorecard result artifact is missing, empty, or expired");
  }
  return { ok: failures.length === 0, failures };
}

export function parseReleaseQualificationArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: { repo: { type: "string" }, branch: { type: "string" }, sha: { type: "string" }, help: { type: "boolean", short: "h" } },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.repo !== REPOSITORY) throw new Error(`--repo must be ${REPOSITORY}`);
  if (!MILESTONE.test(values.branch ?? "")) throw new Error("--branch must be milestone-<x.y.z>");
  if (!SHA.test(values.sha ?? "")) throw new Error("--sha must be a 40-character lowercase commit SHA");
  return { help: false, repo: values.repo, branch: values.branch, sha: values.sha };
}

function ghJson(args, { cwd, run = execFileSync }) {
  try {
    return JSON.parse(run("gh", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
  } catch (error) {
    const detail = error?.stderr?.toString().trim() || error?.message || "unknown failure";
    throw new Error(`GitHub qualification evidence is unavailable: ${detail}`, { cause: error });
  }
}

export function verifyReleaseQualification(options, { cwd = process.cwd(), run = execFileSync } = {}) {
  const currentHead = () => ghJson([
    "api", "--method", "GET", `repos/${options.repo}/git/ref/heads/${options.branch}`,
  ], { cwd, run }).object?.sha;
  if (currentHead() !== options.sha) {
    throw new Error(`milestone ${options.branch} no longer points at ${options.sha}`);
  }
  const workflowRuns = ghJson([
    "api", "--method", "GET", `repos/${options.repo}/actions/workflows/nightly.yml/runs`,
    "-f", "event=workflow_dispatch", "-f", `branch=${options.branch}`, "-f", "per_page=100",
  ], { cwd, run }).workflow_runs;
  if (!Array.isArray(workflowRuns)) throw new Error("GitHub qualification evidence is unavailable: workflow runs response is malformed");
  const candidates = workflowRuns.filter((item) => item?.head_sha === options.sha);
  if (candidates.length === 0) throw new Error(`release qualification is missing for ${options.branch}@${options.sha}`);
  const failures = [];
  for (const candidate of candidates) {
    if (candidate.status !== "completed" || candidate.conclusion !== "success") {
      failures.push(`run ${candidate.id} did not complete successfully`);
      continue;
    }
    const jobs = ghJson(["api", "--method", "GET", `repos/${options.repo}/actions/runs/${candidate.id}/jobs`, "-f", "per_page=100"], { cwd, run }).jobs;
    const artifacts = ghJson(["api", "--method", "GET", `repos/${options.repo}/actions/runs/${candidate.id}/artifacts`, "-f", "per_page=100"], { cwd, run }).artifacts;
    const result = validateReleaseQualification({ ...candidate, jobs, artifacts }, options);
    if (!result.ok) {
      failures.push(`run ${candidate.id}: ${result.failures.join("; ")}`);
      continue;
    }
    if (currentHead() !== options.sha) {
      throw new Error(`milestone ${options.branch} moved while qualification evidence was checked`);
    }
    return { ok: true, branch: options.branch, sha: options.sha, runUrl: candidate.html_url, runId: candidate.id };
  }
  throw new Error(`release qualification is invalid: ${failures.join("; ")}`);
}

export function runCli(argv = process.argv.slice(2), runtime = {}) {
  const options = parseReleaseQualificationArgs(argv);
  if (options.help) {
    (runtime.stdout ?? process.stdout).write("Usage: verify-release-qualification.mjs --repo MediaNoxLabs/oxid --branch milestone-x.y.z --sha <40-hex>\n");
    return;
  }
  (runtime.stdout ?? process.stdout).write(`${JSON.stringify(verifyReleaseQualification(options, runtime))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    process.stderr.write(`[verify-release-qualification] ${error.message}\n`);
    process.exitCode = 1;
  }
}
