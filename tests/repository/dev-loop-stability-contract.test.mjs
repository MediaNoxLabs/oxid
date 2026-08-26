// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmod, lstat, mkdtemp, mkdir, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
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
  assertMinimumGhVersion,
  bodyClosesIssue,
  normalizeTimelinePullRequests,
  parseGhVersion,
} from "../../scripts/github/resolve-issue-pr-links.mjs";
import { preflightGh } from "../../scripts/github/preflight-gh.mjs";
import {
  assertClaudeHelpCapabilities,
  buildClaudeInvocation,
  ClaudeReviewFindingsError,
  parseClaudeReviewResult,
  runClaudeCurrentHeadReview,
  verifyClaudeReviewEvidence,
} from "../../scripts/review/claude-current-head.mjs";
import registerDevLoopPreflight from "../../.pi/extensions/dev-loop-preflight.ts";

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
  await mkdir(path.join(root, ".pi", "agents"), { recursive: true });
  await writeFile(path.join(root, ".pi", "settings.json"), JSON.stringify({
    packages: ["npm:dev-loops@0.9.0"],
    subagents: { projectRootResolution: "git-root" },
  }));
  const packageRoot = path.join(root, ".pi", "npm", "node_modules", "dev-loops");
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await mkdir(path.join(packageRoot, "cli"));
  await writeFile(path.join(packageRoot, "cli", "index.mjs"), "");
  await writeFile(path.join(packageRoot, "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools: read, search, execute, bash, edit, write", "---", "fixture",
  ].join("\n"));
  await writeFile(path.join(root, ".pi", "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools:", "  - read", "  - grep", "  - find", "  - ls", "  - bash", "  - edit", "  - write", "---", "fixture",
  ].join("\n"));
  await symlink(path.join(repoRoot, ".pi", "npm", "node_modules", "yaml"), path.join(root, ".pi", "npm", "node_modules", "yaml"), "dir");
  const worktree = path.join(root, "tmp", "worktrees", "dev-loops", "issue-150");
  await mkdir(worktree, { recursive: true });
  await writeFile(path.join(worktree, ".git"), `gitdir: ${path.join(root, ".git", "worktrees", "fixture")}\n`);
  await mkdir(path.join(worktree, ".pi", "agents"), { recursive: true });
  await writeFile(path.join(worktree, ".pi", "settings.json"), await readFile(path.join(root, ".pi", "settings.json")));
  await writeFile(path.join(worktree, ".pi", "agents", "developer.agent.md"), await readFile(path.join(root, ".pi", "agents", "developer.agent.md")));
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

test("effective packaged agent allowlists use YAML project shadows, not ineffective settings overrides", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const settings = JSON.parse(await readFile(path.join(fixture.root, ".pi", "settings.json"), "utf8"));
  const clean = await checkAgentToolAllowlists({
    packageRoot: fixture.packageRoot,
    projectRoot: fixture.root,
    settings,
    availableTools: supportedTools,
  });
  assert.equal(clean.ok, true);
  assert.equal(clean.agents[0].source, "project");
  assert.deepEqual(clean.agents[0].tools, ["read", "grep", "find", "ls", "bash", "edit", "write"]);

  const shadow = path.join(fixture.root, ".pi", "agents", "developer.agent.md");
  const validShadow = await readFile(shadow);
  await writeFile(shadow, "---\nname: developer\ntools: [read\n---\n");
  await assert.rejects(
    checkAgentToolAllowlists({ packageRoot: fixture.packageRoot, projectRoot: fixture.root, settings, availableTools: supportedTools }),
    /invalid YAML frontmatter.*developer\.agent\.md/,
  );
  await writeFile(shadow, validShadow);
  await rm(shadow);
  const invalid = await checkAgentToolAllowlists({ packageRoot: fixture.packageRoot, projectRoot: fixture.root, settings, availableTools: supportedTools });
  assert.equal(invalid.ok, false);
  assert.deepEqual(invalid.agents[0].missingTools, ["search", "execute"]);

  settings.subagents.agentOverrides = { developer: { tools: ["read"] } };
  await assert.rejects(
    checkAgentToolAllowlists({ packageRoot: fixture.packageRoot, projectRoot: fixture.root, settings, availableTools: supportedTools }),
    /agentOverrides are forbidden.*frontmatter tools/,
  );
});

test("tracked extension blocks invalid tools at input and provider hooks before launch", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const settings = JSON.parse(await readFile(path.join(fixture.root, ".pi", "settings.json"), "utf8"));
  const handlers = new Map();
  const notifications = [];
  const pi = {
    getAllTools: () => [{ name: "read" }],
    on: (event, handler) => handlers.set(event, handler),
  };
  registerDevLoopPreflight(pi, {
    resolve: async () => ({ packageRoot: fixture.packageRoot, gitRoot: fixture.root, settings }),
    check: checkAgentToolAllowlists,
  });
  const ctx = {
    cwd: fixture.root,
    ui: { notify: (message) => notifications.push(message) },
    abortCalled: false,
    abort() { this.abortCalled = true; },
  };
  let providerLaunches = 0;
  const inputResult = await handlers.get("input")({}, ctx);
  if (inputResult.action === "continue") providerLaunches += 1;
  assert.deepEqual(inputResult, { action: "handled" });
  assert.equal(providerLaunches, 0);
  assert.match(notifications[0], /before model execution|unavailable agent tools/);
  await assert.rejects(handlers.get("before_provider_request")({}, ctx), /unavailable agent tools/);
  assert.equal(ctx.abortCalled, true);
});

test("tracked project agents shadow every incompatible packaged dev-loops manifest", async () => {
  const settings = JSON.parse(await read(".pi/settings.json"));
  assert.equal(settings.subagents.projectRootResolution, "git-root");
  assert.equal(settings.subagents.agentOverrides, undefined);
  for (const name of ["dev-loop", "developer", "docs", "fixer", "quality", "refiner", "review"]) {
    const source = await read(`.pi/agents/${name}.agent.md`);
    const toolsLine = source.split(/\r?\n/).find((line) => line.startsWith("tools:"));
    assert.ok(toolsLine, `${name} has a tracked project shadow`);
    const tools = toolsLine.slice("tools:".length).split(",").map((tool) => tool.trim());
    assert.equal(tools.some((tool) => legacyTools.has(tool)), false, `${name} has no legacy tool alias`);
  }
});

test("pinned pi-subagents runtime applies git-root project-agent precedence", async (t) => {
  const jitiPath = path.join(repoRoot, ".pi/npm/node_modules/jiti/lib/jiti.mjs");
  const agentsPath = path.join(repoRoot, ".pi/npm/node_modules/pi-subagents/src/agents/agents.ts");
  try {
    await readFile(jitiPath);
    await readFile(agentsPath);
  } catch {
    t.skip("project-local Pi packages are intentionally absent from public CI");
    return;
  }
  const program = `
    import { createJiti } from ${JSON.stringify(new URL(`file://${jitiPath}`).href)};
    const jiti = createJiti(import.meta.url, { interopDefault: true });
    const { discoverAgents } = await jiti.import(${JSON.stringify(agentsPath)});
    const result = discoverAgents(${JSON.stringify(repoRoot)}, "both");
    const names = ["dev-loop", "developer", "docs", "fixer", "quality", "refiner", "review"];
    process.stdout.write(JSON.stringify(names.map((name) => {
      const agent = result.agents.find((candidate) => candidate.name === name);
      return { name, source: agent?.source, tools: agent?.tools };
    })));
  `;
  const discovered = JSON.parse(execFileSync(process.execPath, ["--input-type=module", "-e", program], { encoding: "utf8" }));
  for (const agent of discovered) {
    assert.equal(agent.source, "project", `${agent.name} uses the tracked project shadow`);
    assert.equal(agent.tools.some((tool) => legacyTools.has(tool)), false, `${agent.name} has no legacy tool`);
  }
});

test("repository wrappers force only the public PR-creation and managed-worktree routes", () => {
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "create", "--head", "topic"]), ["pr", "create", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--silent", "pr", "create-draft", "--head", "topic"]), ["--silent", "pr", "create-draft", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "ready-for-review", "--pr", "153"]), ["pr", "ready-for-review", "--pr", "153"]);
  assert.deepEqual(normalizeDevLoopsArgs(["queue", "add", "--title", "pr", "create"]), ["queue", "add", "--title", "pr", "create"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--jq", ".ok", "pr", "create"]), ["--jq", ".ok", "pr", "create", "--base", "integration"]);
  assert.throws(() => normalizeDevLoopsArgs(["--silent", "pr", "create", "--base", "main"]), /must target integration/);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create-draft", "--base=develop"]), /must target integration/);
  assert.deepEqual(normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150"]), ["--repo-root", "/repo", "--issue", "150", "--base", "origin/integration"]);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--base", "origin/main"]), /must use origin\/integration/);
});

test("GitHub compatibility enforces the supported CLI floor and REST capabilities", async (t) => {
  assert.deepEqual(parseGhVersion("gh version 2.97.0 (2026-08-14)"), [2, 97, 0]);
  assert.deepEqual(assertMinimumGhVersion([2, 67, 0]), [2, 67, 0]);
  assert.throws(() => assertMinimumGhVersion([2, 66, 9]), /unsupported; require >= 2\.67\.0/);
  assert.throws(() => parseGhVersion("not gh"), /could not parse/);
  const links = normalizeTimelinePullRequests([
    { event: "cross-referenced", source: { issue: { number: 71, html_url: "https://github.com/MediaNoxLabs/oxid/pull/71", pull_request: { url: "https://api.github.com/repos/MediaNoxLabs/oxid/pulls/71" } } } },
  ], "MediaNoxLabs/oxid");
  assert.deepEqual(links.map((link) => link.number), [71]);
  assert.equal(bodyClosesIssue("Closes #150", 150), true);
  assert.equal(bodyClosesIssue("Related to #150", 150), false);
  assert.equal(bodyClosesIssue("Closes other/repo#150", 150), false);

  const fixtureRoot = await mkdtemp(path.join(os.tmpdir(), "oxid-gh-preflight-"));
  t.after(() => rm(fixtureRoot, { recursive: true, force: true }));
  const fakeGh = path.join(fixtureRoot, "gh");
  await writeFile(fakeGh, `#!/usr/bin/env node
const args = process.argv.slice(2);
if (args[0] === "--version") process.stdout.write("gh version 2.67.0 (fixture)\\n");
else if (args.at(-1).endsWith("/timeline")) process.stdout.write(JSON.stringify([[{ event: "cross-referenced" }]]));
else if (args.at(-1).endsWith("/issues/150")) process.stdout.write(JSON.stringify({ number: 150 }));
else process.exit(9);
`);
  await chmod(fakeGh, 0o755);
  const probe = preflightGh({ repository: "MediaNoxLabs/oxid", issue: 150, ghCommand: fakeGh });
  assert.deepEqual(probe.version, [2, 67, 0]);
  assert.equal(probe.timelinePages, 1);

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

test("Claude invocation preflights every relied-on flag and requires structured output", () => {
  const invocation = buildClaudeInvocation({ schema: { type: "object" }, maxBudgetUsd: 10 });
  assert.ok(invocation.args.includes("--safe-mode"));
  const toolsIndex = invocation.args.indexOf("--tools");
  assert.ok(toolsIndex >= 0);
  assert.equal(invocation.args[toolsIndex + 1], "");
  assert.ok(invocation.args.includes("--no-session-persistence"));
  assert.equal(assertClaudeHelpCapabilities(invocation.args.filter((arg) => String(arg).startsWith("--")).join("\n")), true);
  assert.throws(() => assertClaudeHelpCapabilities("--safe-mode --tools"), /required review flags/);
  const parsed = parseClaudeReviewResult(JSON.stringify({ structured_output: { verdict: "clean", findings: [], summary: "No findings" }, session_id: "session-1" }));
  assert.equal(parsed.review.verdict, "clean");
  assert.equal(parsed.observedSessionId, "session-1");
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
  process.stdout.write("--print --output-format --json-schema --max-budget-usd --safe-mode --tools --no-session-persistence --permission-mode --system-prompt\\n");
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
  assert.equal(result.evidence.schemaVersion, 2);
  assert.equal(result.evidence.evidenceKind, "local-attestation");
  assert.equal(result.evidence.claude.observedSessionId, "fixture-session");
  assert.equal(result.evidence.claude.tools.length, 0);
  assert.match(result.evidence.limitations.join(" "), /do not authenticate reviewer identity/);
  assert.equal(path.isAbsolute(result.evidence.diff.path), false);
  assert.equal(path.isAbsolute(result.evidence.rawResponse.path), false);
  assert.equal((await verifyClaudeReviewEvidence({ evidencePath: result.evidencePath, repoRoot: repository, fetchBase: false })).ok, true);
  assert.equal((await lstat(evidenceDir)).mode & 0o777, 0o700);
  for (const file of [result.evidencePath, path.join(evidenceDir, result.evidence.diff.path), path.join(evidenceDir, result.evidence.rawResponse.path)]) {
    assert.equal((await lstat(file)).mode & 0o777, 0o600);
  }
  const exactGitDiff = execFileSync("git", ["diff", "--binary", "--full-index", "--no-ext-diff", baseSha, headSha, "--"], { cwd: repository });
  assert.deepEqual(await readFile(path.join(evidenceDir, result.evidence.diff.path)), exactGitDiff);

  await writeFile(fakeClaude, (await readFile(fakeClaude, "utf8")).replace(
    'structured_output: { verdict: "clean", findings: [], summary: "No findings" }',
    'structured_output: { verdict: "findings", findings: [{ severity: "major", message: "Fixture finding" }], summary: "Has findings" }',
  ));
  let findingsError;
  try {
    await runClaudeCurrentHeadReview({
      issue: 150,
      repoRoot: repository,
      evidenceDir,
      expectedHead: headSha,
      claudeCommand: fakeClaude,
      issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
      fetchBase: false,
    });
  } catch (error) {
    findingsError = error;
  }
  assert.ok(findingsError instanceof ClaudeReviewFindingsError);
  assert.equal(JSON.parse(await readFile(findingsError.evidencePath, "utf8")).verdict, "findings");

  const symlinkTarget = path.join(fixtureRoot, "symlink-target");
  const symlinkEvidence = path.join(fixtureRoot, "symlink-evidence");
  await mkdir(symlinkTarget, { mode: 0o700 });
  await symlink(symlinkTarget, symlinkEvidence, "dir");
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir: symlinkEvidence,
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /real directory|final symlink/);

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
