#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const policyPath = path.resolve(scriptDirectory, "../../.github/contribution-policy.json");
export const contributionPolicy = Object.freeze(JSON.parse(readFileSync(policyPath, "utf8")));

const TYPE_SET = new Set(contributionPolicy.types);
const SCOPE_SET = new Set(contributionPolicy.scopes);
const PGP_HEADER = "gpgsig -----BEGIN PGP SIGNATURE-----";

function error(message) {
  return { ok: false, errors: [message] };
}

function addError(result, message) {
  result.ok = false;
  result.errors.push(message);
}

function hasBreakingFooter(body) {
  return /(?:^|\n)BREAKING(?: |-)CHANGE: \S/u.test(body);
}

export function parseConventionalSubject(subject, { body = "" } = {}) {
  const result = { ok: true, errors: [], type: null, scope: null, breaking: false, description: null };
  if (typeof subject !== "string" || subject.length === 0) return error("subject is empty");
  if (subject.includes("\n") || subject.includes("\r")) return error("subject must be one line");
  if (subject.length > contributionPolicy.commit.maxSubjectLength) {
    addError(result, `subject exceeds ${contributionPolicy.commit.maxSubjectLength} characters`);
  }

  const match = subject.match(/^([a-z]+)\(([a-z][a-z0-9-]*)\)(!)?: (\S(?:.*\S)?)$/u);
  if (!match) {
    addError(result, "subject must match <type>(<scope>)[!]: <description>");
    return result;
  }

  [, result.type, result.scope] = match;
  result.breaking = match[3] === "!";
  result.description = match[4];
  if (!TYPE_SET.has(result.type)) addError(result, `type '${result.type}' is not allowed`);
  if (!SCOPE_SET.has(result.scope)) addError(result, `scope '${result.scope}' is not allowed`);
  if (result.description.endsWith(".")) addError(result, "description must not end with a period");
  if (/^(?:fixup!|squash!|wip\b)/iu.test(result.description)) addError(result, "temporary fixup, squash, or WIP subjects are forbidden");
  if (result.breaking && contributionPolicy.commit.requireBreakingChangeFooter && !hasBreakingFooter(body)) {
    addError(result, "a ! marker requires a non-empty BREAKING CHANGE footer");
  }
  return result;
}

export function validateBranchName(branch, { actor = "", expectedType = null } = {}) {
  const bot = contributionPolicy.bots[actor];
  if (bot?.branchExempt) return { ok: true, errors: [], type: null, issue: null, exempt: true };
  const result = { ok: true, errors: [], type: null, issue: null, exempt: false };
  const match = typeof branch === "string" ? branch.match(/^([a-z]+)\/issue-([1-9][0-9]*)$/u) : null;
  if (!match) return error(`branch must match ${contributionPolicy.branch.format}`);
  result.type = match[1];
  result.issue = Number(match[2]);
  if (!TYPE_SET.has(result.type)) addError(result, `branch type '${result.type}' is not allowed`);
  if (expectedType && result.type !== expectedType) {
    addError(result, `branch type '${result.type}' does not match PR type '${expectedType}'`);
  }
  return result;
}

export function validatePullRequest({ title, body = "", branch, actor = "" }) {
  const subject = parseConventionalSubject(title, { body });
  const branchResult = validateBranchName(branch, { actor, expectedType: subject.type });
  return {
    ok: subject.ok && branchResult.ok,
    errors: [...subject.errors.map((message) => `title: ${message}`), ...branchResult.errors.map((message) => `branch: ${message}`)],
    subject,
    branch: branchResult,
  };
}

export function validatePullRequestBody(body, { expectedIssue = null, actor = "" } = {}) {
  if (typeof body !== "string" || body.trim().length === 0) return error("pull request body is empty");
  if (expectedIssue && !contributionPolicy.bots[actor]?.branchExempt) {
    const issueReference = new RegExp(`(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\\s+#${expectedIssue}(?![0-9])`, "iu");
    if (!issueReference.test(body)) return error(`pull request body must close issue #${expectedIssue}`);
  }
  return { ok: true, errors: [] };
}

function git(repository, args) {
  return execFileSync("git", args, {
    cwd: repository,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function requireCommit(repository, revision, label) {
  try {
    git(repository, ["rev-parse", "--verify", `${revision}^{commit}`]);
  } catch {
    throw new Error(`${label} is not an available commit: ${revision}`);
  }
}

function dcoExempt(authorName, actor) {
  const bot = contributionPolicy.bots[actor];
  return Boolean(bot?.dcoAuthorNames?.includes(authorName));
}

export function validateCommitEvidence({ message, authorName, authorEmail, rawCommit = null, verification = null, actor = "" }) {
  const messageResult = validateCommitMessage({ message, authorName, authorEmail, actor });
  const errors = [...messageResult.errors];
  if (contributionPolicy.commit.requireOpenPgp) {
    if (verification) {
      const isOpenPgp = verification.signature?.startsWith("-----BEGIN PGP SIGNATURE-----");
      if (!verification.verified || !isOpenPgp) {
        errors.push(`GitHub OpenPGP verification failed (verified=${Boolean(verification.verified)}, reason=${verification.reason ?? "missing"})`);
      }
    } else if (typeof rawCommit !== "string" || !rawCommit.includes(PGP_HEADER)) {
      errors.push("commit does not contain an OpenPGP signature envelope");
    }
  }
  return { ok: errors.length === 0, errors, subject: messageResult.subject };
}

export function validateCommitMessage({ message, authorName, authorEmail, actor = "" }) {
  const normalized = message.replace(/\n+$/u, "");
  const [subject = "", ...bodyLines] = normalized.split("\n");
  const subjectResult = parseConventionalSubject(subject, { body: bodyLines.join("\n") });
  const errors = subjectResult.errors.map((problem) => `subject: ${problem}`);
  if (contributionPolicy.commit.requireDco && !dcoExempt(authorName, actor)) {
    const exactTrailer = `Signed-off-by: ${authorName} <${authorEmail}>`;
    if (!bodyLines.includes(exactTrailer)) errors.push(`missing exact DCO trailer '${exactTrailer}'`);
  }
  return { ok: errors.length === 0, errors, subject: subjectResult };
}

export function verifyOpenPgpCommit(repository, commit) {
  try {
    git(repository, ["verify-commit", "--raw", commit]);
    return null;
  } catch {
    return "local OpenPGP cryptographic verification failed; ensure the signer public key is available and trusted locally";
  }
}

export function validateCommitRange({
  repository,
  base,
  head,
  actor = "",
  verifyOpenPgp = false,
  verifyCommit = verifyOpenPgpCommit,
}) {
  requireCommit(repository, base, "base");
  requireCommit(repository, head, "head");
  const commits = git(repository, ["rev-list", "--reverse", `${base}..${head}`]).trim().split("\n").filter(Boolean);
  if (commits.length === 0) return { ok: false, commits: [], errors: ["commit range is empty"] };
  if (commits.length > 250) return { ok: false, commits: [], errors: ["commit range exceeds the 250-commit policy bound"] };
  const results = [];
  for (const commit of commits) {
    const evidence = validateCommitEvidence({
      message: git(repository, ["show", "-s", "--format=%B", commit]),
      authorName: git(repository, ["show", "-s", "--format=%an", commit]).trim(),
      authorEmail: git(repository, ["show", "-s", "--format=%ae", commit]).trim(),
      rawCommit: git(repository, ["cat-file", "commit", commit]),
      actor,
    });
    if (verifyOpenPgp) {
      const verificationError = verifyCommit(repository, commit);
      if (verificationError) evidence.errors.push(verificationError);
      evidence.ok = evidence.errors.length === 0;
    }
    results.push({ commit, ok: evidence.ok, errors: evidence.errors });
  }
  return { ok: results.every((result) => result.ok), commits: results, errors: [] };
}

export function validateHostedCommits(records, { actor = "", expectedHead = null } = {}) {
  if (!Array.isArray(records) || records.length === 0) return { ok: false, commits: [], errors: ["pull request has no commits"] };
  if (records.length > 250) return { ok: false, commits: [], errors: ["pull request exceeds the 250-commit policy bound"] };
  const errors = [];
  const seen = new Set();
  const results = [];
  for (const record of records) {
    if (!record || !/^[0-9a-f]{40}$/u.test(record.sha ?? "")) {
      errors.push("hosted commit record has an invalid SHA");
      continue;
    }
    if (seen.has(record.sha)) {
      errors.push(`hosted commit record repeats ${record.sha}`);
      continue;
    }
    seen.add(record.sha);
    if (typeof record.message !== "string" || typeof record.authorName !== "string" || typeof record.authorEmail !== "string") {
      errors.push(`${record.sha} has incomplete author or message metadata`);
      continue;
    }
    const evidence = validateCommitEvidence({
      message: record.message,
      authorName: record.authorName,
      authorEmail: record.authorEmail,
      verification: record.verification,
      actor,
    });
    results.push({ commit: record.sha, ok: evidence.ok, errors: evidence.errors });
  }
  if (expectedHead && records.at(-1)?.sha !== expectedHead) {
    errors.push(`last hosted commit is not exact PR head ${expectedHead}`);
  }
  return { ok: errors.length === 0 && results.every((result) => result.ok), commits: results, errors };
}

export function labelsForSubject(subject, { body = "" } = {}) {
  const parsed = parseConventionalSubject(subject, { body });
  if (!parsed.ok) return { ok: false, errors: parsed.errors, labels: [] };
  return {
    ok: true,
    errors: [],
    labels: [
      `${contributionPolicy.labels.typePrefix}${parsed.type}`,
      `${contributionPolicy.labels.scopePrefix}${parsed.scope}`,
    ],
  };
}

function requiredEnvironment(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function report(result) {
  if (result.ok) return;
  for (const problem of result.errors) process.stderr.write(`[contribution-policy] ${problem}\n`);
  process.exitCode = 1;
}

function run() {
  const command = process.argv[2];
  if (command === "pr") {
    const result = validatePullRequest({
      title: requiredEnvironment("PR_TITLE"),
      body: process.env.PR_BODY ?? "",
      branch: requiredEnvironment("PR_HEAD_REF"),
      actor: process.env.PR_ACTOR ?? "",
    });
    report(result);
    if (result.ok) process.stdout.write(`Contribution metadata valid: ${result.subject.type}(${result.subject.scope}).\n`);
    return;
  }
  if (command === "body") {
    const branch = requiredEnvironment("PR_HEAD_REF");
    const branchResult = validateBranchName(branch, { actor: process.env.PR_ACTOR ?? "" });
    const result = validatePullRequestBody(process.env.PR_BODY ?? "", {
      expectedIssue: branchResult.issue,
      actor: process.env.PR_ACTOR ?? "",
    });
    if (!branchResult.ok) result.errors.push(...branchResult.errors.map((problem) => `branch: ${problem}`));
    result.ok = result.ok && branchResult.ok;
    report(result);
    if (result.ok) process.stdout.write("Pull request body valid.\n");
    return;
  }
  if (command === "commits") {
    const result = validateCommitRange({
      repository: requiredEnvironment("REPOSITORY_PATH"),
      base: requiredEnvironment("BASE_SHA"),
      head: requiredEnvironment("HEAD_SHA"),
      actor: process.env.PR_ACTOR ?? "",
    });
    for (const problem of result.errors) process.stderr.write(`[contribution-policy] ${problem}\n`);
    for (const candidate of result.commits) {
      for (const problem of candidate.errors) process.stderr.write(`[contribution-policy] ${candidate.commit}: ${problem}\n`);
    }
    if (!result.ok) process.exitCode = 1;
    else process.stdout.write(`Commit policy passed for ${result.commits.length} commit(s).\n`);
    return;
  }
  if (command === "hosted-commits") {
    const commitsFile = requiredEnvironment("COMMITS_FILE");
    const info = statSync(commitsFile);
    if (!info.isFile() || info.size > 4 * 1024 * 1024) throw new Error("COMMITS_FILE must be a regular file no larger than 4 MiB");
    const result = validateHostedCommits(JSON.parse(readFileSync(commitsFile, "utf8")), {
      actor: process.env.PR_ACTOR ?? "",
      expectedHead: requiredEnvironment("HEAD_SHA"),
    });
    for (const problem of result.errors) process.stderr.write(`[contribution-policy] ${problem}\n`);
    for (const candidate of result.commits) {
      for (const problem of candidate.errors) process.stderr.write(`[contribution-policy] ${candidate.commit}: ${problem}\n`);
    }
    if (!result.ok) process.exitCode = 1;
    else process.stdout.write(`Hosted commit policy passed for ${result.commits.length} commit(s).\n`);
    return;
  }
  if (command === "labels") {
    const result = labelsForSubject(requiredEnvironment("PR_TITLE"), { body: process.env.PR_BODY ?? "" });
    report(result);
    if (result.ok) process.stdout.write(`${JSON.stringify(result.labels)}\n`);
    return;
  }
  throw new Error("Usage: contribution-policy.mjs <pr|body|commits|hosted-commits|labels>");
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) {
  try {
    run();
  } catch (cause) {
    process.stderr.write(`[contribution-policy] ${cause.message}\n`);
    process.exitCode = 2;
  }
}
