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
const codexRoot = "/fixture/.codex/worktrees";
const codex = `${codexRoot}/issue-305-wallet/oxid`;
const issue = JSON.stringify({
  title: "feat(wallet): enter the canonical worktree",
  body: "## Delivery target\n\ndevelop\n",
});

function gitFixture(current, {
  includeCodex = false,
  dirtyCodex = false,
  codexDeliveryBase = "origin/develop",
  codexBaseIsAncestor = true,
} = {}) {
  return (program, args) => {
    if (program === "gh") return issue;
    assert.equal(program, "git");
    const repository = args[1];
    const gitArgs = args.slice(2);
    if (gitArgs.join(" ") === "rev-parse --show-toplevel") return `${current}\n`;
    if (gitArgs.join(" ") === "remote get-url origin") return "https://github.com/MediaNoxLabs/oxid.git\n";
    if (gitArgs.join(" ") === "worktree list --porcelain") {
      return `worktree ${main}\n\nworktree ${canonical}\n\n${includeCodex ? `worktree ${codex}\n\n` : ""}`;
    }
    if ((repository === canonical || repository === codex) && gitArgs.join(" ") === "branch --show-current") return "feat/issue-305\n";
    if (repository === codex && gitArgs.join(" ") === "status --porcelain") return dirtyCodex ? "?? dirty\n" : "";
    if (repository === main && gitArgs.join(" ") === "config --get branch.feat/issue-305.oxidDeliveryBase") return `${codexDeliveryBase}\n`;
    if (repository === codex && gitArgs.join(" ") === `merge-base --is-ancestor ${codexDeliveryBase} HEAD`) {
      if (!codexBaseIsAncestor) throw new Error("not an ancestor");
      return "";
    }
    throw new Error(`unexpected command: ${program} ${args.join(" ")}`);
  };
}

test("primary exact /dev-loop print enters the canonical issue worktree before Pi", async () => {
  const calls = [];
  const recorded = [];
  const admissions = [];
  const cwd = await resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: main,
    run: gitFixture(main),
    ensureWorktree: async (args, options) => { calls.push({ args, options }); return 0; },
    recordDeliveryBase: (...args) => recorded.push(args),
    recordAdmission: (record) => admissions.push(record),
  });
  assert.equal(cwd, canonical);
  assert.deepEqual(calls, [{
    args: ["--silent", "--repo-root", main, "--issue", "305", "--branch", "feat/issue-305", "--delivery-base", "origin/develop"],
    options: { cwd: main },
  }]);
  assert.deepEqual(recorded, [[main, "feat/issue-305", "origin/develop"]]);
  assert.deepEqual(admissions, [{
    schema: "oxid-dev-loop-admission-v1", issue: 305, repository: "MediaNoxLabs/oxid",
    branch: "feat/issue-305", deliveryBase: "origin/develop",
    calls: { commands: 7, ensureWorktree: 1, recordDeliveryBase: 1 },
  }]);
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

test("verified Codex Desktop /dev-loop print stays in its registered issue worktree", async () => {
  let ensured = false;
  const recorded = [];
  const cwd = await resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: codex,
    run: gitFixture(codex, { includeCodex: true }),
    ensureWorktree: async () => { ensured = true; return 0; },
    recordDeliveryBase: (...args) => recorded.push(args),
    codexWorktreesRoot: codexRoot,
  });
  assert.equal(cwd, codex);
  assert.equal(ensured, false);
  assert.deepEqual(recorded, [[main, "feat/issue-305", "origin/develop"]]);
});

test("dirty Codex Desktop worktrees remain outside dev-loop admission", async () => {
  await assert.rejects(resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: codex,
    run: gitFixture(codex, { includeCodex: true, dirtyCodex: true }),
    recordDeliveryBase: () => {},
    codexWorktreesRoot: codexRoot,
  }), /refusing \/dev-loop dispatch from non-canonical linked worktree/);
});

test("stale or mismatched Codex Desktop delivery bases remain outside admission", async () => {
  for (const options of [
    { includeCodex: true, codexDeliveryBase: "origin/milestone-9.9.9" },
    { includeCodex: true, codexBaseIsAncestor: false },
  ]) {
    await assert.rejects(resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
      repoRoot: codex,
      run: gitFixture(codex, options),
      recordDeliveryBase: () => {},
      codexWorktreesRoot: codexRoot,
    }), /refusing \/dev-loop dispatch from non-canonical linked worktree/);
  }
});

test("bootstrap reads issue metadata from the exact origin repository", async () => {
  let issueRepo;
  await resolveBootstrapDevLoopCwd(["--print", "/dev-loop production-ready issue 305"], {
    repoRoot: canonical,
    run: (program, args) => {
      if (program === "gh") {
        issueRepo = args[args.indexOf("--repo") + 1];
        return issue;
      }
      return gitFixture(canonical)(program, args);
    },
    recordDeliveryBase: () => {},
  });
  assert.equal(issueRepo, "MediaNoxLabs/oxid");
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
  const bootstrap = await readFile(path.join(repoRoot, "bootstrap.sh"), "utf8");
  assert.match(bootstrap, /node "\$repo_root\/scripts\/loop\/prepare-dev-loop-admission\.mjs" prepare -- "\$@"/u);
});
