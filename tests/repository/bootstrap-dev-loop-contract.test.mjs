// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { parseBootstrapDevLoopInvocation, resolveBootstrapDevLoopCwd } from "../../scripts/loop/bootstrap-dev-loop.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const bootstrapSource = await readFile(path.join(repoRoot, "bootstrap.sh"), "utf8");
const main = "/fixture/oxid";
const canonical = `${main}/tmp/worktrees/dev-loops/issue-305`;
const issue = JSON.stringify({
  title: "feat(wallet): enter the canonical worktree",
  body: "## Delivery target\n\ndevelop\n",
});

function gitFixture(current) {
  return (program, args) => {
    if (program === "gh") return issue;
    assert.equal(program, "git");
    const repository = args[1];
    const gitArgs = args.slice(2);
    if (gitArgs.join(" ") === "rev-parse --show-toplevel") return `${current}\n`;
    if (gitArgs.join(" ") === "worktree list --porcelain") return `worktree ${main}\n\nworktree ${canonical}\n\n`;
    if (repository === canonical && gitArgs.join(" ") === "branch --show-current") return "feat/issue-305\n";
    throw new Error(`unexpected command: ${program} ${args.join(" ")}`);
  };
}

test("primary exact /dev-loop print enters the canonical issue worktree before Pi", async () => {
  const calls = [];
  const recorded = [];
  const cwd = await resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: main,
    run: gitFixture(main),
    ensureWorktree: async (args, options) => { calls.push({ args, options }); return 0; },
    recordDeliveryBase: (...args) => recorded.push(args),
  });
  assert.equal(cwd, canonical);
  assert.deepEqual(calls, [{
    args: ["--silent", "--repo-root", main, "--issue", "305", "--branch", "feat/issue-305", "--delivery-base", "origin/develop"],
    options: { cwd: main },
  }]);
  assert.deepEqual(recorded, [[main, "feat/issue-305", "origin/develop"]]);
});

test("primary bootstrap delegates to the canonical flake before starting its devshell", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-bootstrap-dev-loop-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const primary = path.join(root, "primary");
  const worktree = path.join(primary, "tmp", "worktrees", "dev-loops", "issue-305");
  const bin = path.join(root, "bin");
  const pins = path.join(root, "nix-pins");
  await Promise.all([
    mkdir(worktree, { recursive: true }),
    mkdir(bin, { recursive: true }),
  ]);
  await Promise.all([
    writeFile(path.join(primary, "bootstrap.sh"), bootstrapSource, { mode: 0o755 }),
    writeFile(path.join(worktree, "bootstrap.sh"), bootstrapSource, { mode: 0o755 }),
    writeFile(path.join(primary, ".fixture-flake-pin"), "primary-stale\n"),
    writeFile(path.join(worktree, ".fixture-flake-pin"), "canonical-current\n"),
    writeFile(path.join(bin, "node"), `#!/bin/bash
printf '%s\\n' "$CANONICAL_WORKTREE"
`, { mode: 0o755 }),
    writeFile(path.join(bin, "nix"), `#!/bin/bash
[ "$1" = develop ] || exit 90
[[ " $* " == *" /nix/var/nix/profiles/default/bin "* ]] || exit 91
printf '%s\\n' "$(cat .fixture-flake-pin)" >> "$NIX_PINS"
`, { mode: 0o755 }),
  ]);
  await Promise.all([chmod(path.join(primary, "bootstrap.sh"), 0o755), chmod(path.join(worktree, "bootstrap.sh"), 0o755)]);

  const result = spawnSync(path.join(primary, "bootstrap.sh"), ["--pi", "--print", "/dev-loop production-ready issue 305"], {
    cwd: primary,
    encoding: "utf8",
    env: {
      PATH: `${bin}:/usr/bin:/bin`,
      CANONICAL_WORKTREE: worktree,
      NIX_PINS: pins,
      OXID_BOOTSTRAP_NIX_PROFILE_BIN: "/nix/var/nix/profiles/default/bin",
    },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(pins, "utf8"), "canonical-current\n");
});

test("ordinary Pi startup remains Nix-only and does not require a host Node binary", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-bootstrap-ordinary-pi-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const primary = path.join(root, "primary");
  const bin = path.join(root, "bin");
  const pins = path.join(root, "nix-pins");
  await Promise.all([mkdir(primary, { recursive: true }), mkdir(bin, { recursive: true })]);
  await Promise.all([
    writeFile(path.join(primary, "bootstrap.sh"), bootstrapSource, { mode: 0o755 }),
    writeFile(path.join(primary, ".fixture-flake-pin"), "primary-current\n"),
    writeFile(path.join(bin, "nix"), `#!/bin/bash
[ "$1" = develop ] || exit 90
printf '%s\\n' "$(cat .fixture-flake-pin)" >> "$NIX_PINS"
`, { mode: 0o755 }),
  ]);
  await chmod(path.join(primary, "bootstrap.sh"), 0o755);

  const result = spawnSync(path.join(primary, "bootstrap.sh"), ["--pi", "--print", "explain this checkout"], {
    cwd: primary,
    encoding: "utf8",
    env: {
      PATH: `${bin}:/usr/bin:/bin`,
      NIX_PINS: pins,
    },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await readFile(pins, "utf8"), "primary-current\n");
});

test("linked canonical /dev-loop print stays in that worktree", async () => {
  let ensured = false;
  const cwd = await resolveBootstrapDevLoopCwd(["--print=/dev-loop prototype issue 305"], {
    repoRoot: canonical,
    run: gitFixture(canonical),
    ensureWorktree: async () => { ensured = true; return 0; },
    recordDeliveryBase: () => {},
  });
  assert.equal(cwd, canonical);
  assert.equal(ensured, false);
});

test("ordinary Pi prompts are unchanged and malformed or ambiguous dev-loop commands never dispatch", async () => {
  assert.equal(await resolveBootstrapDevLoopCwd(["--print", "explain this diff"], {
    repoRoot: main,
    run: () => { throw new Error("ordinary prompt must not inspect GitHub or Git"); },
  }), main);
  for (const args of [
    ["--print", "/dev-loop production-ready issue nope"],
    ["--print", "/dev-loop production-ready issue 305", "--print", "other"],
  ]) {
    await assert.rejects(resolveBootstrapDevLoopCwd(args, {
      repoRoot: main,
      run: () => { throw new Error("malformed command must not dispatch"); },
    }), /bootstrap accepts/);
  }
  let dispatched = false;
  await assert.rejects(resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: main,
    run: (program, args) => program === "gh"
      ? JSON.stringify({ title: "feat(wallet): ambiguous target", body: "## Delivery target\n\ndevelop\nmilestone-1.2.3\n" })
      : gitFixture(main)(program, args),
    ensureWorktree: async () => { dispatched = true; return 0; },
    recordDeliveryBase: () => { dispatched = true; },
  }), /exactly one branch name/);
  assert.equal(dispatched, false);
  assert.deepEqual(parseBootstrapDevLoopInvocation(["--print", "/dev-loop prototype issue 305"]), { profile: "prototype", issue: 305 });
});

test("dev-loop conductor disables managed subagent worktree wrapping", async () => {
  const agent = await readFile(path.join(repoRoot, ".pi", "agents", "dev-loop.agent.md"), "utf8");
  assert.match(agent, /^worktree: false$/m);
});
