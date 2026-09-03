#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, lstatSync, realpathSync, rmSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

function git(root, args, options = {}) {
  return execFileSync("git", ["-C", root, ...args], {
    encoding: "utf8",
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  }).trim();
}

function option(argv, name) {
  const index = argv.indexOf(name);
  return index === -1 ? undefined : argv[index + 1];
}

export function parseWorktrees(source) {
  return source.trim().split(/\n\n+/).filter(Boolean).map((record) => {
    const fields = {};
    for (const line of record.split("\n")) {
      const separator = line.indexOf(" ");
      const key = separator === -1 ? line : line.slice(0, separator);
      const value = separator === -1 ? true : line.slice(separator + 1);
      fields[key] = value;
    }
    return fields;
  });
}

function targetGiB(worktree) {
  const target = path.join(worktree, "target");
  if (!existsSync(target)) return 0;
  const result = spawnSync("du", ["-sk", target], { encoding: "utf8" });
  if (result.status !== 0) return null;
  const kib = Number(result.stdout.trim().split(/\s+/)[0]);
  return Number.isFinite(kib) ? kib / 1024 / 1024 : null;
}

function isAncestor(root, ancestor, descendant, run = spawnSync) {
  try {
    return run("git", ["-C", root, "merge-base", "--is-ancestor", ancestor, descendant], {
      stdio: "ignore",
    }).status === 0;
  } catch {
    return false;
  }
}

const SHA_PATTERN = /^[0-9a-f]{40}$/;

// `integration` was a temporary delivery branch promoted in full by PR #258
// and then retired under #264. Keep this immutable transition explicit so
// historical exact-head worktrees can be proven delivered without reviving a
// second permanent base branch.
const RETIRED_BRANCH_PROMOTIONS = Object.freeze([
  Object.freeze({ number: 258, source: "integration", target: "develop" }),
]);

export function githubRepositoryFromRemote(remote) {
  const match = remote.match(/^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+?)(?:\.git)?\/?$/);
  return match ? `${match[1]}/${match[2]}` : null;
}

function commandText(run, command, args, options = {}) {
  try {
    const result = run(command, args, { encoding: "utf8", timeout: 30_000, ...options });
    return result.status === 0 && typeof result.stdout === "string" ? result.stdout.trim() : null;
  } catch {
    return null;
  }
}

function remoteDevelopHead(root, run) {
  const output = commandText(run, "git", ["-C", root, "ls-remote", "--exit-code", "origin", "refs/heads/develop"]);
  if (output === null) return null;
  const [head, ref, ...extra] = output.split(/\s+/);
  if (extra.length > 0 || ref !== "refs/heads/develop" || !SHA_PATTERN.test(head ?? "")) return null;
  return head;
}

export function indexGithubMergeProofs(
  pulls,
  mergeCommitIsIntegrated,
  requestedHeads = null,
  {
    promotions = [],
    mergeCommitIsInPromotion = () => false,
    promotionPreservesTree = () => false,
  } = {},
) {
  if (!Array.isArray(pulls)) throw new Error("GitHub merge evidence must be an array");
  const candidates = new Map();
  for (const pull of pulls) {
    const head = pull?.headRefOid;
    const mergeCommit = pull?.mergeCommit?.oid;
    if (!Number.isSafeInteger(pull?.number) || pull.number < 1
      || pull?.state !== "MERGED"
      || typeof pull?.mergedAt !== "string" || pull.mergedAt.length === 0
      || !SHA_PATTERN.test(head ?? "") || !SHA_PATTERN.test(mergeCommit ?? "")
      || (requestedHeads !== null && !requestedHeads.has(head))) continue;
    const proofs = candidates.get(head) ?? new Set();
    if (pull.baseRefName === "develop" && mergeCommitIsIntegrated(mergeCommit)) {
      proofs.add(`github-pr:${pull.number}`);
    }
    for (const promotion of promotions) {
      const promotionHead = promotion?.headRefOid;
      const promotionMerge = promotion?.mergeCommit?.oid;
      const specification = RETIRED_BRANCH_PROMOTIONS.find((candidate) => (
        candidate.number === promotion?.number
      ));
      if (specification === undefined
        || promotion?.state !== "MERGED"
        || promotion?.baseRefName !== specification.target
        || promotion?.headRefName !== specification.source
        || pull.baseRefName !== specification.source
        || typeof promotion?.mergedAt !== "string" || promotion.mergedAt.length === 0
        || !SHA_PATTERN.test(promotionHead ?? "") || !SHA_PATTERN.test(promotionMerge ?? "")
        || !mergeCommitIsIntegrated(promotionMerge)
        || !promotionPreservesTree(promotionHead, promotionMerge)
        || !mergeCommitIsInPromotion(mergeCommit, promotionHead)) continue;
      proofs.add(`github-pr:${pull.number}:via-pr:${promotion.number}`);
    }
    if (proofs.size === 0) continue;
    candidates.set(head, proofs);
  }
  const proofs = new Map();
  const ambiguous = new Set();
  for (const [head, matches] of candidates) {
    if (matches.size === 1) proofs.set(head, [...matches][0]);
    else ambiguous.add(head);
  }
  return { proofs, ambiguous };
}

function unavailableEvidence(ancestry = new Set()) {
  return { status: "unavailable", ancestry, proofs: new Map(), ambiguous: new Set(), unavailableHeads: new Set() };
}

export function githubMergeQuery(heads) {
  if (!Array.isArray(heads) || heads.length === 0 || heads.some((head) => !SHA_PATTERN.test(head))) {
    throw new Error("GitHub merge query requires exact commit heads");
  }
  const selections = heads.map((head, index) => (
    `h${index}:object(oid:"${head}"){... on Commit{associatedPullRequests(first:10){nodes{number state mergedAt baseRefName headRefOid mergeCommit{oid}}pageInfo{hasNextPage}}}}`
  )).join("");
  const promotions = RETIRED_BRANCH_PROMOTIONS.map((promotion, index) => (
    `p${index}:pullRequest(number:${promotion.number}){number state mergedAt baseRefName headRefName headRefOid mergeCommit{oid}}`
  )).join("");
  return `query($owner:String!,$name:String!){repository(owner:$owner,name:$name){${selections}${promotions}}}`;
}

export function parseGithubMergeResponse(output, heads) {
  const payload = JSON.parse(output);
  if (payload?.errors !== undefined || typeof payload?.data?.repository !== "object" || payload.data.repository === null) {
    throw new Error("GitHub merge evidence response is incomplete");
  }
  const pulls = [];
  const promotions = [];
  const unavailableHeads = new Set();
  for (const [index, head] of heads.entries()) {
    const object = payload.data.repository[`h${index}`];
    if (object === null) continue;
    const connection = object?.associatedPullRequests;
    if (!Array.isArray(connection?.nodes) || typeof connection?.pageInfo?.hasNextPage !== "boolean") {
      throw new Error("GitHub merge evidence connection is malformed");
    }
    if (connection.pageInfo.hasNextPage) unavailableHeads.add(head);
    else pulls.push(...connection.nodes);
  }
  for (const [index] of RETIRED_BRANCH_PROMOTIONS.entries()) {
    const promotion = payload.data.repository[`p${index}`];
    if (promotion !== null && promotion !== undefined) promotions.push(promotion);
  }
  return { pulls, promotions, unavailableHeads };
}

function sameTree(root, left, right, run) {
  const leftTree = commandText(run, "git", ["-C", root, "rev-parse", `${left}^{tree}`]);
  const rightTree = commandText(run, "git", ["-C", root, "rev-parse", `${right}^{tree}`]);
  return SHA_PATTERN.test(leftTree ?? "") && leftTree === rightTree;
}

export function loadGithubMergeEvidence(root, heads, { run = spawnSync } = {}) {
  const uniqueHeads = [...new Set(heads)].filter((head) => SHA_PATTERN.test(head));
  const ancestry = new Set(uniqueHeads.filter((head) => isAncestor(root, head, "origin/develop", run)));
  const requested = uniqueHeads.filter((head) => !ancestry.has(head));
  if (requested.length === 0) {
    return { status: "available", ancestry, proofs: new Map(), ambiguous: new Set(), unavailableHeads: new Set() };
  }
  const trackedHead = commandText(run, "git", ["-C", root, "rev-parse", "origin/develop"]);
  const remote = commandText(run, "git", ["-C", root, "remote", "get-url", "origin"]);
  const repository = remote === null ? null : githubRepositoryFromRemote(remote);
  if (!SHA_PATTERN.test(trackedHead ?? "") || repository === null) return unavailableEvidence(ancestry);
  const before = remoteDevelopHead(root, run);
  if (before === null || before !== trackedHead) return unavailableEvidence(ancestry);
  const [owner, name] = repository.split("/");
  const output = commandText(run, "gh", [
    "api", "graphql",
    "-f", `query=${githubMergeQuery(requested)}`,
    "-f", `owner=${owner}`,
    "-f", `name=${name}`,
  ], { cwd: root });
  const after = remoteDevelopHead(root, run);
  if (output === null || after === null || after !== before) return unavailableEvidence(ancestry);
  try {
    const response = parseGithubMergeResponse(output, requested);
    const availableHeads = new Set(requested.filter((head) => !response.unavailableHeads.has(head)));
    const indexed = indexGithubMergeProofs(
      response.pulls,
      (mergeCommit) => isAncestor(root, mergeCommit, "origin/develop", run),
      availableHeads,
      {
        promotions: response.promotions,
        mergeCommitIsInPromotion: (mergeCommit, promotionHead) => (
          isAncestor(root, mergeCommit, promotionHead, run)
        ),
        promotionPreservesTree: (promotionHead, promotionMerge) => (
          sameTree(root, promotionHead, promotionMerge, run)
        ),
      },
    );
    return { status: "available", ancestry, ...indexed, unavailableHeads: response.unavailableHeads };
  } catch {
    return unavailableEvidence(ancestry);
  }
}

export function resolveMergeState(head, byAncestry, githubEvidence) {
  if (byAncestry) return { merged: true, mergeProof: "ancestry" };
  const githubProof = githubEvidence.proofs.get(head);
  if (githubProof) return { merged: true, mergeProof: githubProof };
  if (githubEvidence.ambiguous.has(head)) return { merged: false, mergeProof: "ambiguous" };
  if (githubEvidence.unavailableHeads?.has(head)) return { merged: false, mergeProof: "unavailable" };
  return { merged: false, mergeProof: githubEvidence.status === "available" ? "none" : "unavailable" };
}

function describe(root, entry, now = Date.now(), githubEvidence = unavailableEvidence()) {
  const worktree = realpathSync(entry.worktree);
  const head = git(worktree, ["rev-parse", "HEAD"]);
  const clean = git(worktree, ["status", "--porcelain=v1"]) === "";
  const committedAt = Number(git(worktree, ["log", "-1", "--format=%ct"]));
  const directory = statSync(worktree);
  const createdAtMs = directory.birthtimeMs > 0 ? directory.birthtimeMs : directory.mtimeMs;
  // A new worktree on an old commit is still new. Retention requires both the
  // selected revision and this checkout directory to be old enough.
  const ageDays = Math.min((now / 1000 - committedAt) / 86400, (now - createdAtMs) / 86400000);
  const { merged, mergeProof } = resolveMergeState(head, githubEvidence.ancestry.has(head), githubEvidence);
  return {
    worktree,
    branch: typeof entry.branch === "string" ? entry.branch.replace("refs/heads/", "") : "(detached)",
    head,
    clean,
    merged,
    mergeProof,
    ageDays,
    targetGiB: targetGiB(worktree),
  };
}

export function removalEligibility(item, { primary, olderThanDays = 7 }) {
  if (item.worktree === primary) return "primary checkout";
  if (!item.clean) return "worktree is dirty";
  if (!item.merged) return `head is not integrated into origin/develop (merge proof: ${item.mergeProof ?? "none"})`;
  if (item.ageDays < olderThanDays) return `last commit is newer than ${olderThanDays} days`;
  return null;
}

function audit(root, entries, githubEvidence, { json = false } = {}) {
  const items = entries.map((entry) => describe(root, entry, Date.now(), githubEvidence));
  const primary = realpathSync(entries[0].worktree);
  if (json) {
    process.stdout.write(`${JSON.stringify(items.map((item) => ({
      ...item,
      removableAfterSevenDays: removalEligibility(item, { primary }) === null,
    })), null, 2)}\n`);
    return;
  }
  process.stdout.write("GiB\tclean\tmerged\tage(d)\tbranch\tworktree\tproof\n");
  for (const item of items) {
    const size = item.targetGiB === null ? "?" : item.targetGiB.toFixed(1);
    process.stdout.write(`${size}\t${item.clean}\t${item.merged}\t${item.ageDays.toFixed(1)}\t${item.branch}\t${item.worktree}\t${item.mergeProof}\n`);
  }
}

function selectedItem(root, entries, argv, githubEvidence) {
  const requested = option(argv, "--path");
  const expectedHead = option(argv, "--expect-head");
  if (!requested || !expectedHead) throw new Error("--path and --expect-head are required");
  const resolved = realpathSync(requested);
  const entry = entries.find((candidate) => realpathSync(candidate.worktree) === resolved);
  if (!entry) throw new Error("--path is not a registered worktree");
  const item = describe(root, entry, Date.now(), githubEvidence);
  if (item.head !== expectedHead) throw new Error(`head changed: expected ${expectedHead}, found ${item.head}`);
  return item;
}

function requireExecute(argv) {
  if (!argv.includes("--execute")) throw new Error("refusing mutation without --execute");
}

function main(argv = process.argv.slice(2)) {
  const command = argv[0] ?? "audit";
  const root = git(process.cwd(), ["worktree", "list", "--porcelain"])
    .split("\n").find((line) => line.startsWith("worktree ")).slice("worktree ".length);
  const entries = parseWorktrees(git(root, ["worktree", "list", "--porcelain"]));
  const primary = realpathSync(entries[0].worktree);
  const githubEvidence = command === "audit" || command === "remove"
    ? loadGithubMergeEvidence(root, entries.map((entry) => entry.HEAD))
    : unavailableEvidence();
  if (command === "audit") return audit(root, entries, githubEvidence, { json: argv.includes("--json") });

  const item = selectedItem(root, entries, argv, githubEvidence);
  requireExecute(argv);
  if (command === "clean-target") {
    if (item.worktree === primary) throw new Error("refusing to clean the primary checkout target");
    if (!item.clean) throw new Error("refusing to clean a dirty worktree");
    const target = path.join(item.worktree, "target");
    if (!existsSync(target)) return;
    const targetMetadata = lstatSync(target);
    if (targetMetadata.isSymbolicLink() || !targetMetadata.isDirectory()) {
      throw new Error("target must be a real directory, not a symlink or file");
    }
    const resolvedTarget = realpathSync(target);
    if (resolvedTarget !== path.resolve(target) || path.dirname(resolvedTarget) !== item.worktree) {
      throw new Error("target is not a real directory directly beneath the selected worktree");
    }
    rmSync(resolvedTarget, { recursive: true });
    return;
  }
  if (command === "remove") {
    const olderThanDays = Number(option(argv, "--older-than-days") ?? "7");
    if (!Number.isFinite(olderThanDays) || olderThanDays < 1) throw new Error("--older-than-days must be at least 1");
    const reason = removalEligibility(item, { primary, olderThanDays });
    if (reason) throw new Error(`refusing removal: ${reason}`);
    execFileSync("git", ["-C", root, "worktree", "remove", "--", item.worktree], { stdio: "inherit" });
    return;
  }
  throw new Error(`unknown command: ${command}`);
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`[worktree-lifecycle] ${error.message}\n`);
    process.exitCode = 1;
  }
}
