#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  validateBranchName,
  validateCommitMessage,
  validateCommitRange,
} from "../ci/contribution-policy.mjs";
import { parseDeliveryTarget } from "../lib/delivery-target.mjs";

const ZERO_OID = /^(?:0{40}|0{64})$/u;
const REMOTE_NAME = /^[A-Za-z0-9._-]+$/u;

function git(repository, args) {
  return execFileSync("git", args, {
    cwd: repository,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function config(repository, key, { local = false } = {}) {
  try {
    return git(repository, ["config", ...(local ? ["--local"] : []), "--get", key]);
  } catch {
    return "";
  }
}

export function inspectSigningConfiguration(repository) {
  const errors = [];
  if (config(repository, "commit.gpgSign", { local: true }) !== "true") {
    errors.push("repository-local commit.gpgSign must be true");
  }
  if (config(repository, "gpg.format", { local: true }) !== "openpgp") {
    errors.push("repository-local gpg.format must be openpgp");
  }
  if (!config(repository, "user.signingkey")) {
    errors.push("user.signingkey must identify an available OpenPGP signing key");
  }
  if (!config(repository, "user.name") || !config(repository, "user.email")) {
    errors.push("Git user.name and user.email must be configured for exact DCO identity");
  }
  return { ok: errors.length === 0, errors };
}

export function authorIdentity(repository) {
  const ident = git(repository, ["var", "GIT_AUTHOR_IDENT"]);
  const match = ident.match(/^(.*) <([^<>]+)> [0-9]+ [+-][0-9]{4}$/u);
  if (!match || !match[1] || !match[2]) throw new Error("Git did not provide a parseable author identity");
  return { name: match[1], email: match[2] };
}

export function validateMessageFile(repository, messageFile) {
  const resolved = path.resolve(messageFile);
  const info = statSync(resolved);
  if (!info.isFile() || info.size > 1024 * 1024) {
    throw new Error("commit message must be a regular file no larger than 1 MiB");
  }
  const author = authorIdentity(repository);
  return validateCommitMessage({
    message: readFileSync(resolved, "utf8"),
    authorName: author.name,
    authorEmail: author.email,
  });
}

export function parsePushUpdates(input) {
  const updates = [];
  for (const [index, line] of input.split(/\r?\n/u).entries()) {
    if (!line.trim()) continue;
    const fields = line.trim().split(/\s+/u);
    if (fields.length !== 4) throw new Error(`pre-push update ${index + 1} must contain four fields`);
    const [localRef, localSha, remoteRef, remoteSha] = fields;
    if (!/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u.test(localSha)
      || !/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u.test(remoteSha)) {
      throw new Error(`pre-push update ${index + 1} contains an invalid object ID`);
    }
    updates.push({ localRef, localSha, remoteRef, remoteSha, deletion: ZERO_OID.test(localSha) });
  }
  return updates;
}

function configuredDeliveryRef(repository, remoteName, branch, gitRunner) {
  let configured;
  try {
    configured = gitRunner(repository, ["config", "--local", "--get", `branch.${branch}.oxidDeliveryBase`]);
  } catch {
    throw new Error(`${branch} has no recorded delivery base; create it with the managed worktree wrapper or configure branch.${branch}.oxidDeliveryBase`);
  }
  const target = parseDeliveryTarget(configured);
  return `refs/remotes/${remoteName}/${target.branch}`;
}

export function planPushRanges({ repository, remoteName, input, gitRunner = git, deliveryBaseResolver = configuredDeliveryRef }) {
  if (!REMOTE_NAME.test(remoteName)) throw new Error("pre-push remote name is invalid");
  const updates = parsePushUpdates(input);
  const plans = [];
  for (const update of updates) {
    if (update.deletion) continue;
    let localRef = update.localRef;
    if (localRef === "HEAD") {
      try {
        localRef = gitRunner(repository, ["symbolic-ref", "--quiet", "HEAD"]);
      } catch {
        throw new Error("HEAD pushes require a named issue branch");
      }
    }
    if (!localRef.startsWith("refs/heads/")) {
      throw new Error(`local development hooks permit branch pushes only; received ${localRef}`);
    }
    const branch = localRef.slice("refs/heads/".length);
    const branchResult = validateBranchName(branch);
    if (!branchResult.ok) throw new Error(branchResult.errors.join("; "));
    if (update.remoteRef !== localRef) {
      throw new Error(`issue branch ${localRef} may only push to the same remote ref, not ${update.remoteRef}`);
    }
    const deliveryRef = deliveryBaseResolver(repository, remoteName, branch, gitRunner);
    try {
      gitRunner(repository, ["rev-parse", "--verify", `${deliveryRef}^{commit}`]);
      gitRunner(repository, ["rev-parse", "--verify", `${update.localSha}^{commit}`]);
    } catch {
      throw new Error(`fetch ${deliveryRef} before pushing so the complete candidate range can be verified`);
    }
    let base;
    try {
      base = gitRunner(repository, ["merge-base", deliveryRef, update.localSha]);
    } catch {
      throw new Error(`${branch} has no locally resolvable ${deliveryRef} merge base`);
    }
    if (!base) throw new Error(`${branch} has an empty ${deliveryRef} merge base`);
    plans.push({
      branch,
      deliveryRef,
      deliveryBranch: deliveryRef.slice(`refs/remotes/${remoteName}/`.length),
      base,
      head: update.localSha,
      remoteRef: update.remoteRef,
    });
  }
  return plans;
}

export function validatePrePush({
  repository,
  remoteName,
  input,
  gitRunner = git,
  deliveryBaseResolver = configuredDeliveryRef,
  rangeValidator = validateCommitRange,
}) {
  const plans = planPushRanges({ repository, remoteName, input, gitRunner, deliveryBaseResolver });
  const results = plans.map((plan) => ({
    ...plan,
    result: rangeValidator({
      repository,
      base: plan.base,
      head: plan.head,
      baseRef: plan.deliveryBranch,
      headRef: plan.branch,
      verifyOpenPgp: true,
    }),
  }));
  return {
    ok: results.every((entry) => entry.result.ok),
    results,
  };
}

function reportErrors(errors) {
  for (const problem of errors) process.stderr.write(`[local-contribution-policy] ${problem}\n`);
}

function repositoryRoot() {
  return git(process.cwd(), ["rev-parse", "--show-toplevel"]);
}

function main(argv = process.argv.slice(2)) {
  const command = argv[0];
  const repository = repositoryRoot();
  if (command === "pre-commit") {
    const result = inspectSigningConfiguration(repository);
    reportErrors(result.errors);
    if (!result.ok) process.exitCode = 1;
    return;
  }
  if (command === "commit-msg") {
    if (argv.length !== 2) throw new Error("commit-msg requires the Git message-file argument");
    const result = validateMessageFile(repository, argv[1]);
    reportErrors(result.errors);
    if (!result.ok) process.exitCode = 1;
    return;
  }
  if (command === "pre-push") {
    if (argv.length !== 3) throw new Error("pre-push requires Git's remote-name and remote-url arguments");
    const result = validatePrePush({
      repository,
      remoteName: argv[1],
      input: readFileSync(0, "utf8"),
    });
    for (const entry of result.results) {
      reportErrors(entry.result.errors);
      for (const candidate of entry.result.commits) {
        reportErrors(candidate.errors.map((problem) => `${candidate.commit}: ${problem}`));
      }
    }
    if (!result.ok) process.exitCode = 1;
    else process.stdout.write(`Local contribution policy passed for ${result.results.length} outgoing branch range(s).\n`);
    return;
  }
  throw new Error("Usage: local-policy.mjs <pre-commit|commit-msg FILE|pre-push REMOTE URL>");
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`[local-contribution-policy] ${error.message}\n`);
    process.exitCode = 2;
  }
}
