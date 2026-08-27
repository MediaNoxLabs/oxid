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

function isAncestor(root, ancestor, descendant) {
  return spawnSync("git", ["-C", root, "merge-base", "--is-ancestor", ancestor, descendant], {
    stdio: "ignore",
  }).status === 0;
}

function describe(root, entry, now = Date.now()) {
  const worktree = realpathSync(entry.worktree);
  const head = git(worktree, ["rev-parse", "HEAD"]);
  const clean = git(worktree, ["status", "--porcelain=v1"]) === "";
  const committedAt = Number(git(worktree, ["log", "-1", "--format=%ct"]));
  const directory = statSync(worktree);
  const createdAtMs = directory.birthtimeMs > 0 ? directory.birthtimeMs : directory.mtimeMs;
  // A new worktree on an old commit is still new. Retention requires both the
  // selected revision and this checkout directory to be old enough.
  const ageDays = Math.min((now / 1000 - committedAt) / 86400, (now - createdAtMs) / 86400000);
  const merged = isAncestor(root, head, "origin/integration");
  return {
    worktree,
    branch: typeof entry.branch === "string" ? entry.branch.replace("refs/heads/", "") : "(detached)",
    head,
    clean,
    merged,
    ageDays,
    targetGiB: targetGiB(worktree),
  };
}

export function removalEligibility(item, { primary, olderThanDays = 7 }) {
  if (item.worktree === primary) return "primary checkout";
  if (!item.clean) return "worktree is dirty";
  if (!item.merged) return "head is not merged into origin/integration";
  if (item.ageDays < olderThanDays) return `last commit is newer than ${olderThanDays} days`;
  return null;
}

function audit(root, entries, { json = false } = {}) {
  const items = entries.map((entry) => describe(root, entry));
  const primary = realpathSync(entries[0].worktree);
  if (json) {
    process.stdout.write(`${JSON.stringify(items.map((item) => ({
      ...item,
      removableAfterSevenDays: removalEligibility(item, { primary }) === null,
    })), null, 2)}\n`);
    return;
  }
  process.stdout.write("GiB\tclean\tmerged\tage(d)\tbranch\tworktree\n");
  for (const item of items) {
    const size = item.targetGiB === null ? "?" : item.targetGiB.toFixed(1);
    process.stdout.write(`${size}\t${item.clean}\t${item.merged}\t${item.ageDays.toFixed(1)}\t${item.branch}\t${item.worktree}\n`);
  }
}

function selectedItem(root, entries, argv) {
  const requested = option(argv, "--path");
  const expectedHead = option(argv, "--expect-head");
  if (!requested || !expectedHead) throw new Error("--path and --expect-head are required");
  const resolved = realpathSync(requested);
  const entry = entries.find((candidate) => realpathSync(candidate.worktree) === resolved);
  if (!entry) throw new Error("--path is not a registered worktree");
  const item = describe(root, entry);
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
  if (command === "audit") return audit(root, entries, { json: argv.includes("--json") });

  const item = selectedItem(root, entries, argv);
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
