// SPDX-License-Identifier: Apache-2.0

import { execFile } from "node:child_process";
import { lstat, realpath } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const WORKTREE_NAMESPACE_SEGMENTS = ["tmp", "worktrees", "dev-loops"];
const SHA_PATTERN = /^[0-9a-f]{40}$/;
const ISSUE_BRANCH_PATTERN = /^(?:build|chore|ci|docs|feat|fix|perf|refactor|revert|style|test)\/issue-([1-9]\d*)$/;

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

function countWorktreeNamespaces(candidate) {
  const segments = path.normalize(candidate).split(path.sep).filter(Boolean);
  let count = 0;
  for (let index = 0; index <= segments.length - WORKTREE_NAMESPACE_SEGMENTS.length; index += 1) {
    if (WORKTREE_NAMESPACE_SEGMENTS.every((segment, offset) => segments[index + offset] === segment)) count += 1;
  }
  return count;
}

async function pathState(candidate) {
  let entry;
  try {
    entry = await lstat(candidate);
  } catch (error) {
    if (error?.code === "ENOENT") return { exists: false, entry: null, real: null };
    if (error?.code === "ENOTDIR") {
      throw new Error(`handoff envelope topology contains a non-directory ancestor: ${candidate}`, { cause: error });
    }
    throw error;
  }
  return { exists: true, entry, real: await realpath(candidate) };
}

async function listedWorktrees(commonRoot) {
  const { stdout } = await execFileAsync(
    "git",
    ["-C", commonRoot, "worktree", "list", "--porcelain", "-z"],
    { encoding: "utf8", maxBuffer: 1024 * 1024 },
  );
  return stdout.split("\0")
    .filter((field) => field.startsWith("worktree "))
    .map((field) => path.resolve(field.slice("worktree ".length)));
}

async function commandText(command, args) {
  const { stdout } = await execFileAsync(command, args, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    timeout: 30_000,
  });
  return stdout.trim();
}

async function resolveHostedPrHead(target) {
  const output = await commandText("gh", [
    "pr",
    "view",
    String(target.pr),
    "--repo",
    target.repo,
    "--json",
    "state,headRefName,headRefOid",
  ]);
  const result = JSON.parse(output);
  if (result?.state !== "OPEN"
    || typeof result.headRefName !== "string" || result.headRefName.length === 0
    || !SHA_PATTERN.test(result.headRefOid ?? "")) {
    throw new Error(`hosted PR #${target.pr} has no exact open head`);
  }
  return result;
}

async function authorizePrIssueWorktree(target, candidate, {
  commonRoot,
  core,
  worktrees,
  resolvePrHead,
}) {
  if (target?.kind !== "pr" || !worktrees.includes(candidate)) return false;
  let branch;
  try {
    branch = await commandText("git", ["-C", candidate, "branch", "--show-current"]);
  } catch {
    return false;
  }
  const match = branch.match(ISSUE_BRANCH_PATTERN);
  if (match === null) return false;
  const issue = Number(match[1]);
  if (candidate !== path.resolve(core.resolveWorktreePath({ repoRoot: commonRoot, kind: "issue", number: issue }))) {
    return false;
  }
  let deliveryBase;
  try {
    deliveryBase = await commandText("git", [
      "-C",
      commonRoot,
      "config",
      "--get",
      `branch.${branch}.oxidDeliveryBase`,
    ]);
  } catch {
    return false;
  }
  if (!/^origin\/(?:develop|milestone-\d+\.\d+\.\d+)$/.test(deliveryBase)) return false;
  const [head, hosted] = await Promise.all([
    commandText("git", ["-C", candidate, "rev-parse", "HEAD"]),
    resolvePrHead(target),
  ]);
  if (head !== hosted.headRefOid || branch !== hosted.headRefName) {
    throw new Error(`managed issue worktree does not match hosted PR #${target.pr} head`);
  }
  return true;
}

function canonicalTargets(target, commonRoot, core) {
  if (target?.kind === "issue") {
    return [core.resolveWorktreePath({ repoRoot: commonRoot, kind: "issue", number: target.issue })];
  }
  if (target?.kind === "pr") {
    return [core.resolveWorktreePath({ repoRoot: commonRoot, kind: "pr", number: target.pr })];
  }
  if (target?.kind === "local_branch") {
    const slug = core.buildWorktreeSlug(target, target.kind);
    if (!slug) throw new Error("handoff envelope local_branch target has no canonical worktree identity");
    return [path.join(commonRoot, core.WORKTREE_NAMESPACE, slug)];
  }
  if (target?.kind === "local_phase") {
    const slug = core.buildWorktreeSlug(target, target.kind);
    if (!slug) throw new Error("handoff envelope local_phase target has no canonical worktree identity");
    const targets = [path.join(commonRoot, core.WORKTREE_NAMESPACE, slug)];
    if (Number.isInteger(target.issue) && target.issue > 0) {
      targets.push(core.resolveWorktreePath({ repoRoot: commonRoot, kind: "issue", number: target.issue }));
    }
    return [...new Set(targets.map((candidate) => path.resolve(candidate)))];
  }
  throw new Error(`handoff envelope target kind '${target?.kind ?? "missing"}' has no authorized managed worktree`);
}

async function assertProspectiveAncestors(candidate, commonRoot) {
  let current = path.dirname(candidate);
  while (isContained(commonRoot, current)) {
    const state = await pathState(current);
    if (state.exists) {
      if (state.entry.isSymbolicLink() || state.real !== current) {
        throw new Error(`refusing prospective handoff envelope cwd beneath symlinked topology: ${candidate}`);
      }
      if (!state.entry.isDirectory()) {
        throw new Error(`refusing prospective handoff envelope cwd beneath non-directory topology: ${candidate}`);
      }
    }
    if (current === commonRoot) return;
    current = path.dirname(current);
  }
  throw new Error(`prospective handoff envelope cwd escapes the common checkout: ${candidate}`);
}

async function assertOwnedOrProspective(candidate, { commonRoot, canonical, worktrees }) {
  if (!isContained(commonRoot, candidate) || countWorktreeNamespaces(candidate) !== 1) {
    throw new Error(`refusing non-canonical handoff envelope cwd: ${candidate}`);
  }
  if (!canonical.includes(candidate)) {
    throw new Error(`handoff envelope cwd does not match the resolver target: ${candidate}`);
  }

  const state = await pathState(candidate);
  if (!state.exists) {
    await assertProspectiveAncestors(candidate, commonRoot);
    return;
  }
  if (state.entry.isSymbolicLink() || state.real !== candidate) {
    throw new Error(`refusing symlinked or realpath-mismatched handoff envelope cwd: ${candidate}`);
  }
  if (!state.entry.isDirectory()) {
    throw new Error(`refusing non-directory handoff envelope cwd: ${candidate}`);
  }
  if (!worktrees.includes(candidate)) {
    throw new Error(`refusing foreign existing handoff envelope cwd: ${candidate}`);
  }
}

export async function normalizeHandoffEnvelopeCwd(envelope, resolved, core, {
  resolvePrHead = resolveHostedPrHead,
} = {}) {
  const commonRoot = path.resolve(resolved.commonRoot);
  const gitRoot = path.resolve(resolved.gitRoot);
  const [commonState, gitState] = await Promise.all([pathState(commonRoot), pathState(gitRoot)]);
  if (!commonState.exists || !gitState.exists || commonState.entry.isSymbolicLink() || gitState.entry.isSymbolicLink()) {
    throw new Error("handoff envelope checkout topology is absent or symlinked");
  }
  if (commonState.real !== commonRoot || gitState.real !== gitRoot) {
    throw new Error("handoff envelope checkout path does not match its realpath");
  }

  const isMainCheckout = gitRoot === commonRoot;
  if (isMainCheckout && (typeof envelope.cwd !== "string" || envelope.cwd.trim() === "")) {
    throw new Error("handoff envelope cwd is required for a main-checkout invocation");
  }
  if (isMainCheckout && !path.isAbsolute(envelope.cwd)) {
    throw new Error(`handoff envelope cwd must be absolute: ${envelope.cwd}`);
  }

  const worktrees = await listedWorktrees(commonRoot);
  let canonical = canonicalTargets(envelope.target, commonRoot, core);
  let cwd;
  const candidate = isMainCheckout ? path.resolve(envelope.cwd) : gitRoot;
  if (!canonical.includes(candidate) && await authorizePrIssueWorktree(envelope.target, candidate, {
    commonRoot,
    core,
    worktrees,
    resolvePrHead,
  })) {
    canonical = [...canonical, candidate];
  }
  if (isMainCheckout) {
    cwd = candidate;
    await assertOwnedOrProspective(cwd, { commonRoot, canonical, worktrees });
  } else {
    if (!worktrees.includes(gitRoot)) throw new Error(`invocation checkout is not an owned Git worktree: ${gitRoot}`);
    if (!canonical.includes(gitRoot)) {
      throw new Error(`invocation worktree disagrees with resolver target: ${gitRoot}`);
    }
    await assertOwnedOrProspective(gitRoot, { commonRoot, canonical, worktrees });
    cwd = gitRoot;
  }

  const normalized = { ...envelope, cwd };
  const validation = core.validateHandoffEnvelope(normalized);
  if (!validation.ok) {
    throw new Error(`normalized handoff envelope failed core validation: ${JSON.stringify(validation.errors)}`);
  }
  return normalized;
}
