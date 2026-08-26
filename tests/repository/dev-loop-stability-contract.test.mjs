// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmod, mkdtemp, mkdir, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  checkAgentToolAllowlists,
  resolveDevLoopsPackageRoot,
} from "../../scripts/lib/dev-loop-runtime.mjs";
import { normalizeDevLoopsArgs } from "../../scripts/dev-loops.mjs";
import { normalizeWorktreeArgs } from "../../scripts/loop/ensure-worktree.mjs";
import {
  bodyClosesIssue,
  normalizeTimelinePullRequests,
  parseGhVersion,
} from "../../scripts/github/resolve-issue-pr-links.mjs";
import {
  buildClaudeInvocation,
  parseClaudeReviewResult,
  runClaudeCurrentHeadReview,
  verifyClaudeReviewEvidence,
} from "../../scripts/review/claude-current-head.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");
const legacyTools = new Set(["search", "execute", "agent", "todo"]);
const supportedTools = [
  "read", "grep", "find", "ls", "bash", "edit", "write", "subagent",
  "labels_bootstrap", "pr_approve_dep_upgrade", "pr_expedite", "pr_request_review",
  "pr_stabilize", "pr_watch", "review_claim", "review_complete", "review_create",
  "review_enrich", "review_list",
];

async function makeFixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), "oxid-dev-loop-root-"));
  await mkdir(path.join(root, ".git", "worktrees", "fixture"), { recursive: true });
  await mkdir(path.join(root, ".pi", "npm", "node_modules", "dev-loops", "agents"), { recursive: true });
  await writeFile(path.join(root, ".pi", "settings.json"), JSON.stringify({
    packages: ["npm:dev-loops@0.9.0"],
    subagents: {
      projectRootResolution: "git-root",
      agentOverrides: {
        developer: { tools: ["read", "grep", "find", "ls", "bash", "edit", "write"] },
      },
    },
  }));
  const packageRoot = path.join(root, ".pi", "npm", "node_modules", "dev-loops");
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await mkdir(path.join(packageRoot, "cli"));
  await writeFile(path.join(packageRoot, "cli", "index.mjs"), "");
  await writeFile(path.join(packageRoot, "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools: read, search, execute, bash, edit, write", "---", "fixture",
  ].join("\n"));
  const worktree = path.join(root, "tmp", "worktrees", "dev-loops", "issue-150");
  await mkdir(worktree, { recursive: true });
  await writeFile(path.join(worktree, ".git"), `gitdir: ${path.join(root, ".git", "worktrees", "fixture")}\n`);
  await mkdir(path.join(worktree, ".pi"));
  await writeFile(path.join(worktree, ".pi", "settings.json"), await readFile(path.join(root, ".pi", "settings.json")));
  return { root, worktree, packageRoot };
}

test("project-local dev-loops resolution is exact from roots and linked worktrees", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));

  for (const cwd of [fixture.root, fixture.worktree]) {
    const resolved = await resolveDevLoopsPackageRoot({ cwd });
    assert.equal(await realpath(resolved.packageRoot), await realpath(fixture.packageRoot));
    assert.equal(resolved.version, "0.9.0");
    assert.equal(resolved.source, cwd === fixture.root ? "git-root" : "git-common-root");
  }
});

test("package resolution rejects mismatched identities and symlink escapes", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const manifest = path.join(fixture.packageRoot, "package.json");
  await writeFile(manifest, JSON.stringify({ name: "dev-loops", version: "9.9.9" }));
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: fixture.root }), /expected dev-loops@0\.9\.0/);

  await rm(fixture.packageRoot, { recursive: true, force: true });
  const outside = await mkdtemp(path.join(os.tmpdir(), "oxid-dev-loop-outside-"));
  t.after(() => rm(outside, { recursive: true, force: true }));
  await mkdir(path.join(outside, "cli"));
  await writeFile(path.join(outside, "cli", "index.mjs"), "");
  await writeFile(path.join(outside, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await symlink(outside, fixture.packageRoot, "dir");
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: fixture.root }), /escapes allowed project roots/);
});

test("effective packaged agent allowlists are checked against actual Pi tools", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const settings = JSON.parse(await readFile(path.join(fixture.root, ".pi", "settings.json"), "utf8"));
  const clean = await checkAgentToolAllowlists({
    packageRoot: fixture.packageRoot,
    settings,
    availableTools: supportedTools,
  });
  assert.equal(clean.ok, true);
  assert.deepEqual(clean.agents[0].tools, ["read", "grep", "find", "ls", "bash", "edit", "write"]);

  delete settings.subagents.agentOverrides.developer;
  const invalid = await checkAgentToolAllowlists({ packageRoot: fixture.packageRoot, settings, availableTools: supportedTools });
  assert.equal(invalid.ok, false);
  assert.deepEqual(invalid.agents[0].missingTools, ["search", "execute"]);
});

test("tracked project settings replace legacy names and choose the git root", async () => {
  const settings = JSON.parse(await read(".pi/settings.json"));
  assert.equal(settings.subagents.projectRootResolution, "git-root");
  for (const name of ["dev-loop", "developer", "docs", "fixer", "quality", "refiner", "review"]) {
    const tools = settings.subagents.agentOverrides[name]?.tools;
    assert.ok(Array.isArray(tools), `${name} has a tracked tool override`);
    assert.equal(tools.some((tool) => legacyTools.has(tool)), false, `${name} has no legacy tool alias`);
  }
});

test("repository wrappers force integration bases", () => {
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "create", "--head", "topic"]), ["pr", "create", "--head", "topic", "--base", "integration"]);
  assert.throws(() => normalizeDevLoopsArgs(["--silent", "pr", "create", "--base", "main"]), /must target integration/);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create", "--base", "main"]), /must target integration/);
  assert.deepEqual(normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150"]), ["--repo-root", "/repo", "--issue", "150", "--base", "origin/integration"]);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--base", "origin/main"]), /must use origin\/integration/);
});

test("GitHub compatibility uses REST timeline facts without deprecated fields", async () => {
  assert.deepEqual(parseGhVersion("gh version 2.97.0 (2026-08-14)"), [2, 97, 0]);
  assert.throws(() => parseGhVersion("not gh"), /could not parse/);
  const links = normalizeTimelinePullRequests([
    { event: "cross-referenced", source: { issue: { number: 71, html_url: "https://github.com/MediaNoxLabs/oxid/pull/71", pull_request: { url: "https://api.github.com/repos/MediaNoxLabs/oxid/pulls/71" } } } },
  ], "MediaNoxLabs/oxid");
  assert.deepEqual(links.map((link) => link.number), [71]);
  assert.equal(bodyClosesIssue("Closes #150", 150), true);
  assert.equal(bodyClosesIssue("Related to #150", 150), false);

  for (const file of [
    "scripts/github/resolve-issue-pr-links.mjs",
    "scripts/github/preflight-gh.mjs",
    "scripts/lib/dev-loop-runtime.mjs",
  ]) {
    const source = await read(file);
    assert.doesNotMatch(source, /projectCards|closingIssuesReferences/, file);
  }
  assert.match(await read("nix/devshells/default.nix"), /^\s+gh$/m);
});

test("Claude invocation is tools-disabled and parser requires an explicit clean verdict", () => {
  const invocation = buildClaudeInvocation({ schema: { type: "object" }, maxBudgetUsd: 10 });
  assert.ok(invocation.args.includes("--safe-mode"));
  const toolsIndex = invocation.args.indexOf("--tools");
  assert.ok(toolsIndex >= 0);
  assert.equal(invocation.args[toolsIndex + 1], "");
  assert.ok(invocation.args.includes("--no-session-persistence"));
  assert.equal(parseClaudeReviewResult(JSON.stringify({ structured_output: { verdict: "clean", findings: [], summary: "No findings" }, session_id: "session-1" })).review.verdict, "clean");
  assert.throws(() => parseClaudeReviewResult(JSON.stringify({ result: "No findings" })), /structured review result/);
  assert.throws(() => parseClaudeReviewResult(JSON.stringify({ structured_output: { verdict: "clean", findings: [{ severity: "blocker", message: "bad" }] } })), /clean verdict cannot contain findings/);
});

test("Claude runner binds clean exact-head evidence and rejects stale worktrees", async (t) => {
  const fixtureRoot = await mkdtemp(path.join(os.tmpdir(), "oxid-claude-runner-"));
  const repository = path.join(fixtureRoot, "repo");
  const evidenceDir = path.join(fixtureRoot, "evidence");
  const fakeClaude = path.join(fixtureRoot, "claude");
  t.after(() => rm(fixtureRoot, { recursive: true, force: true }));
  await mkdir(repository);

  const git = (...args) => execFileSync("git", args, { cwd: repository, encoding: "utf8" }).trim();
  git("init", "--quiet");
  git("config", "user.name", "Fixture");
  git("config", "user.email", "fixture@example.invalid");
  git("config", "commit.gpgsign", "false");
  await writeFile(path.join(repository, "contract.txt"), "base\n");
  git("add", "contract.txt");
  git("commit", "--quiet", "-m", "base");
  const baseSha = git("rev-parse", "HEAD");
  git("update-ref", "refs/remotes/origin/integration", baseSha);
  await writeFile(path.join(repository, "contract.txt"), "base\nhead\n");
  git("add", "contract.txt");
  git("commit", "--quiet", "-m", "head");
  const headSha = git("rev-parse", "HEAD");

  await writeFile(fakeClaude, `#!/usr/bin/env node
const fs = require("node:fs");
if (process.argv.includes("--version")) {
  process.stdout.write("claude fixture 1.0\\n");
} else if (process.argv.includes("--help")) {
  process.stdout.write("--safe-mode --tools --json-schema --no-session-persistence\\n");
} else if (process.argv[2] === "auth" && process.argv[3] === "status") {
  process.stdout.write(JSON.stringify({ loggedIn: true, authMethod: "fixture", apiProvider: "fixture" }));
} else {
  const prompt = fs.readFileSync(0, "utf8");
  if (!prompt.includes("${headSha}") || !prompt.includes("${baseSha}")) process.exit(9);
  process.stdout.write(JSON.stringify({
    session_id: "fixture-session",
    structured_output: { verdict: "clean", findings: [], summary: "No findings" },
  }));
}
`);
  await chmod(fakeClaude, 0o755);

  const result = await runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  });
  assert.equal(result.evidence.headSha, headSha);
  assert.equal(result.evidence.baseSha, baseSha);
  assert.equal(result.evidence.claude.sessionId, "fixture-session");
  assert.equal(result.evidence.claude.tools.length, 0);
  assert.equal(path.isAbsolute(result.evidence.diff.path), false);
  assert.equal(path.isAbsolute(result.evidence.rawResponse.path), false);
  assert.equal((await verifyClaudeReviewEvidence({ evidencePath: result.evidencePath, repoRoot: repository, fetchBase: false })).ok, true);

  await writeFile(path.join(repository, "dirty.txt"), "dirty\n");
  await assert.rejects(
    verifyClaudeReviewEvidence({ evidencePath: result.evidencePath, repoRoot: repository, fetchBase: false }),
    /clean worktree/,
  );
});

test("manual Claude review and zero Copilot rounds are mandatory gate facts", async () => {
  const config = await read(".devloops");
  assert.match(config, /^\s+maxCopilotRounds: 0$/m);
  for (const block of [
    config.slice(config.indexOf("  draft:"), config.indexOf("  preApproval:")),
    config.slice(config.indexOf("  preApproval:"), config.indexOf("  requireFanoutEvidence:")),
  ]) {
    const mandatory = block.slice(block.indexOf("    mandatoryAngles:"));
    assert.match(mandatory, /^\s+- external-review$/m);
  }
});

test("upstream-only gaps are linked and speculative local patches are forbidden", async () => {
  const doc = await read("docs/dev-loop-stability.md");
  for (const link of [
    "nicobailon/pi-subagents/issues/985",
    "nicobailon/pi-subagents/issues/1460",
    "nicobailon/pi-subagents/issues/1434",
    "blob/v0.42.1/src/runs/background/async-resume.ts",
  ]) assert.match(doc, new RegExp(link.replaceAll("/", "\\/")));
  assert.match(doc, /usageBudget/);
  assert.match(doc, /upstream-only/i);
  assert.match(doc, /do not (?:patch|modify|vendor)/i);
});

test("repository verification runs this stability contract", async () => {
  const run = await read("run.sh");
  assert.match(run, /node --test tests\/repository\/dev-loop-stability-contract\.test\.mjs/);
});
