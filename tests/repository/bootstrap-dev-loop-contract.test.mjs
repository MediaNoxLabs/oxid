// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { parseBootstrapDevLoopInvocation, resolveBootstrapDevLoopCwd } from "../../scripts/loop/bootstrap-dev-loop.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
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
