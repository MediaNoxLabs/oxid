// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, lstat, mkdtemp, mkdir, readFile, readdir, realpath, rm, stat, symlink, utimes, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  checkAgentToolAllowlists,
  devLoopPreflightCacheKey,
  ensureSharedPiPackageStore,
  parseAgentFrontmatter,
  resolveDevLoopsPackageRoot,
} from "../../scripts/lib/dev-loop-runtime.mjs";
import { normalizeHandoffEnvelopeCwd } from "../../scripts/lib/handoff-envelope-cwd.mjs";
import { normalizeDevLoopsArgs, resolvePinnedCoreModulePath, runDevLoops } from "../../scripts/dev-loops.mjs";
import { assertNoPreflightBypass, inferSubagentAvailability, runPreFlightGate, runRepositoryPreflight } from "../../scripts/loop/pre-flight-gate.mjs";
import { enforceFactoryAdmissionForCreation, normalizeLinkedWorktreeContext, normalizeWorktreeArgs, resolveRepositoryWorktreePath, runEnsureWorktree } from "../../scripts/loop/ensure-worktree.mjs";
import { assertReviewedWorktreePin, oxidConsumerProvision } from "../../scripts/loop/ensure-worktree-consumer.mjs";
import {
  assertMinimumGhVersion,
  assertTimelinePages,
  bodyReferencesIssue,
  normalizeTimelinePullRequests,
  parseGhVersion,
  resolveIssuePullRequestLinks,
} from "../../scripts/github/resolve-issue-pr-links.mjs";
import { preflightGh } from "../../scripts/github/preflight-gh.mjs";
import { GH_REST_MAX_BUFFER_BYTES, runGhCommand } from "../../scripts/github/rest-client.mjs";
import {
  assertClaudeAuthHelpCapabilities,
  assertAttestedReviewEffort,
  assertClaudeEffortCapability,
  assertClaudeHelpCapabilities,
  assertMinimumClaudeVersion,
  assertClaudeReviewMaxBudgetUsd,
  CLAUDE_REVIEW_EFFORTS,
  DEFAULT_CLAUDE_REVIEW_EFFORT,
  MAXIMUM_EXCLUSIVE_CLAUDE_VERSION,
  MAXIMUM_CLAUDE_REVIEW_BUDGET_USD,
  buildClaudeInvocation,
  claudeReviewCliFailure,
  ClaudeReviewEvidenceVersionError,
  ClaudeReviewFindingsError,
  MAX_CLAUDE_REVIEW_TIMEOUT_MS,
  MAX_REVIEW_DIFF_BYTES,
  parseClaudeReviewResult,
  parseClaudeVersion,
  probeClaudeCliCapabilities,
  runCli as runClaudeReviewCli,
  runClaudeCurrentHeadReview,
  verifyClaudeReviewEvidence,
} from "../../scripts/review/claude-current-head.mjs";
import registerDevLoopPreflight, { runDevLoopPreflight } from "../../scripts/lib/dev-loop-preflight-core.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");
const legacyTools = new Set(["search", "execute", "agent", "todo"]);
const supportedTools = [
  "read", "grep", "find", "ls", "bash", "edit", "write", "subagent",
  "labels_bootstrap", "pr_approve_dep_upgrade", "pr_expedite", "pr_request_review",
  "pr_stabilize", "pr_watch", "review_claim", "review_complete", "review_create",
  "review_enrich", "review_list",
];
const handoffCore = {
  WORKTREE_NAMESPACE: path.join("tmp", "worktrees", "dev-loops"),
  resolveWorktreePath: ({ repoRoot: root, kind, number }) => path.join(root, "tmp", "worktrees", "dev-loops", `${kind}-${number}`),
  buildWorktreeSlug: (target) => target.kind === "local_branch"
    ? target.branch.replaceAll("/", "-")
    : `phase-${target.issue}-${target.phase}`,
  validateHandoffEnvelope: (envelope) => ({ ok: typeof envelope.cwd === "string", errors: [] }),
};

const fixtureClaudeHelp = [
  "  --print",
  "  --output-format <format>",
  "  --json-schema <schema>",
  "  --max-budget-usd <amount>",
  "  --effort <level> (low, medium, high, xhigh, max)",
  "  --safe-mode",
  '  --tools <tools...> Specify tools. Use "" to disable all tools.',
  "  --no-session-persistence",
  '  --permission-mode <mode> (choices: "acceptEdits", "dontAsk", "plan")',
  "  --system-prompt <prompt>",
].join("\n");
// Captured verbatim from the installed Claude Code 2.1.228 general help.
const capturedClaudeEffortEntry = [
  "  --effort <level>                      Effort level for the current session",
  "                                        (low, medium, high, xhigh, max)",
].join("\n");
const fixtureClaudeAuthHelp = "Usage: claude auth status [options]\n  --json Output as JSON (default)\n";
const fixtureClaudeCliEfforts = ["low", "medium", "high", "xhigh", "max"];

async function realMkdtemp(prefix) {
  return realpath(await mkdtemp(path.join(os.tmpdir(), prefix)));
}

async function installedPiRoot(t) {
  try {
    const expectedVersion = execFileSync("pi", ["--version"], { encoding: "utf8" }).trim();
    const executable = (execFileSync("which", ["pi"], { encoding: "utf8" })).trim();
    const root = path.dirname(path.dirname(await realpath(executable)));
    const packageRoot = path.join(root, "lib", "node_modules", "pi-monorepo");
    const manifest = JSON.parse(await readFile(path.join(packageRoot, "package.json"), "utf8"));
    if (manifest.version !== expectedVersion) throw new Error(`expected Pi ${expectedVersion}, found ${manifest.version}`);
    return packageRoot;
  } catch (error) {
    t.skip(`Nix-pinned Pi runner fixture unavailable: ${error.message}`);
    return null;
  }
}

async function makeFixture() {
  const root = await realMkdtemp("oxid-dev-loop-root-");
  await mkdir(path.join(root, ".pi", "agents"), { recursive: true });
  await writeFile(path.join(root, ".gitignore"), "/.pi/npm\n/tmp/\n");
  await writeFile(path.join(root, ".devloops"), "version: 1\n");
  await writeFile(path.join(root, ".pi", "settings.json"), JSON.stringify({
    packages: ["npm:dev-loops@0.9.0"],
    subagents: { projectRootResolution: "git-root" },
  }));
  await writeFile(path.join(root, ".pi", "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools:", "  - read", "  - grep", "  - find", "  - ls", "  - bash", "  - edit", "  - write", "---", "fixture",
  ].join("\n"));
  execFileSync("git", ["init", "--initial-branch", "integration"], { cwd: root, stdio: "ignore" });
  execFileSync("git", ["add", ".devloops", ".gitignore", ".pi/settings.json", ".pi/agents/developer.agent.md"], { cwd: root, stdio: "ignore" });
  execFileSync("git", ["-c", "user.name=Oxid Test", "-c", "user.email=oxid-test@example.invalid", "commit", "-m", "fixture"], {
    cwd: root,
    stdio: "ignore",
  });
  const worktree = path.join(root, "tmp", "worktrees", "dev-loops", "issue-150");
  await mkdir(path.dirname(worktree), { recursive: true });
  execFileSync("git", ["worktree", "add", "-b", "issue-150", worktree, "HEAD"], { cwd: root, stdio: "ignore" });

  await mkdir(path.join(root, ".pi", "npm", "node_modules", "dev-loops", "agents"), { recursive: true });
  const packageRoot = path.join(root, ".pi", "npm", "node_modules", "dev-loops");
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await mkdir(path.join(packageRoot, "cli"));
  await writeFile(path.join(packageRoot, "cli", "index.mjs"), 'process.stdout.write("dev-loop-out\\n"); process.stderr.write("dev-loop-err\\n");\n');
  await mkdir(path.join(packageRoot, "scripts", "loop"), { recursive: true });
  await writeFile(path.join(packageRoot, "scripts", "loop", "ensure-worktree.mjs"), [
    'import { mkdir } from "node:fs/promises";',
    'import path from "node:path";',
    'function value(argv, name) { const index = argv.indexOf(name); if (index >= 0) return argv[index + 1]; const prefix = `${name}=`; return argv.find((arg) => arg.startsWith(prefix))?.slice(prefix.length); }',
    'async function genericProvision({ worktreePath }) { process.stderr.write("[provision-worktree] WARN fixture generic provisioning ran\\n"); await mkdir(path.join(worktreePath, "node_modules", "@dev-loops"), { recursive: true }); return { ok: true, actions: [{ mode: "link" }], summary: { copied: 0, linked: 1, skipped: 0, rejected: 0, warnings: 1 } }; }',
    'export function parseEnsureWorktreeCliArgs(argv) { const help = argv.includes("--help") || argv.includes("-h"); const repoRoot = value(argv, "--repo-root"); if (!help && !repoRoot) throw new Error("Missing required --repo-root"); return { help, repoRoot, issue: Number(value(argv, "--issue")), pr: Number(value(argv, "--pr")), branch: value(argv, "--branch"), base: value(argv, "--base"), jq: value(argv, "--jq"), silent: argv.includes("--silent") || argv.includes("-s") }; }',
    'export async function ensureWorktree(options, { provision = genericProvision } = {}) { if (options.branch === "conflict") throw new Error("fixture branch conflict"); const kind = Number.isInteger(options.issue) && options.issue > 0 ? "issue" : "pr"; const number = kind === "issue" ? options.issue : options.pr; const worktreePath = path.join(options.repoRoot, "tmp", "worktrees", "dev-loops", `${kind}-${number}`); const provisionResult = await provision({ worktreePath, repoRoot: options.repoRoot }); if (options.branch === "trailing") await new Promise((resolve) => setTimeout(() => { process.stdout.write("worktree-out\\n"); process.stderr.write("worktree-err\\n"); resolve(); }, 10)); return { ok: true, path: worktreePath, created: false, reused: true, provision: provisionResult }; }',
    'export async function runCli(_argv, { stdout }) { stdout.write("fixture help\\n"); }',
  ].join("\n"));
  await mkdir(path.join(packageRoot, "scripts", "lib"), { recursive: true });
  await writeFile(path.join(packageRoot, "scripts", "lib", "jq-output.mjs"), [
    'export function emitResult(result, { jq, silent, stdout }) {',
    '  if (silent) return result.ok ? 0 : 1;',
    '  const value = jq === undefined ? result : jq === ".ok" ? result.ok : result;',
    '  stdout.write(`${JSON.stringify(value)}\\n`);',
    '  return 0;',
    '}',
  ].join("\n"));
  await writeFile(path.join(packageRoot, "scripts", "_core-helpers.mjs"), 'export function formatCliError(error) { return error.message; }\n');
  await writeFile(path.join(packageRoot, "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools: read, search, execute, bash, edit, write", "---", "fixture",
  ].join("\n"));
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

test("registered linked worktrees use one fail-closed Pi package store", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));

  assert.equal((await ensureSharedPiPackageStore({ cwd: fixture.root })).mode, "primary");
  const linked = await ensureSharedPiPackageStore({ cwd: fixture.worktree });
  assert.equal(linked.mode, "linked");
  assert.equal((await lstat(path.join(fixture.worktree, ".pi", "npm"))).isSymbolicLink(), true);
  assert.equal(await realpath(path.join(fixture.worktree, ".pi", "npm")), await realpath(path.join(fixture.root, ".pi", "npm")));
  assert.equal((await ensureSharedPiPackageStore({ cwd: fixture.worktree })).mode, "linked");
  assert.equal(execFileSync("git", ["status", "--porcelain", "--", ".pi/npm"], { cwd: fixture.worktree, encoding: "utf8" }), "");
  assert.equal((await resolveDevLoopsPackageRoot({ cwd: fixture.worktree })).source, "git-common-root");

  await rm(path.join(fixture.worktree, ".pi", "npm"));
  await mkdir(path.join(fixture.worktree, ".pi", "npm"));
  await assert.rejects(ensureSharedPiPackageStore({ cwd: fixture.worktree }), /absent or a managed symlink/);
  await rm(path.join(fixture.worktree, ".pi", "npm"), { recursive: true });
  const outside = await realMkdtemp("oxid-pi-store-outside-");
  t.after(() => rm(outside, { recursive: true, force: true }));
  await symlink(outside, path.join(fixture.worktree, ".pi", "npm"), "dir");
  await assert.rejects(ensureSharedPiPackageStore({ cwd: fixture.worktree }), /outside the registered common checkout/);
});

test("Pi smoke resolution reuses every exact common-checkout package from a linked worktree", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));

  const pins = [
    ["pi-subagents", "0.42.1"],
    ["@input-output-hk/agent-review-pi", "0.5.0"],
  ];
  const settings = {
    packages: ["npm:dev-loops@0.9.0", ...pins.map(([name, version]) => `npm:${name}@${version}`)],
    subagents: { projectRootResolution: "git-root" },
  };
  for (const settingsRoot of [fixture.root, fixture.worktree]) {
    await writeFile(path.join(settingsRoot, ".pi", "settings.json"), JSON.stringify(settings));
  }
  for (const [name, version] of pins) {
    const packageRoot = path.join(fixture.root, ".pi", "npm", "node_modules", ...name.split("/"));
    await mkdir(packageRoot, { recursive: true });
    await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name, version }));
  }

  const resolved = await resolveDevLoopsPackageRoot({
    cwd: fixture.worktree,
    includeAllPinnedPackages: true,
  });
  assert.deepEqual(resolved.packageRoots.map(({ name, source }) => [name, source]), [
    ["dev-loops", "git-common-root"],
    ["pi-subagents", "git-common-root"],
    ["@input-output-hk/agent-review-pi", "git-common-root"],
  ]);
  assert.equal(await lstat(path.join(fixture.worktree, ".pi", "npm")).catch(() => null), null);
});

test("Pi devshell smoke delegates package authority to the bounded exact-pin resolver", async () => {
  const smoke = await read("scripts/check-pi-devshell.sh");
  const devshell = await read("nix/devshells/default.nix");
  assert.match(smoke, /resolveDevLoopsPackageRoot/);
  assert.match(smoke, /includeAllPinnedPackages:\s*true/);
  assert.doesNotMatch(smoke, /review_package_root=["']\.pi\/npm/);
  assert.doesNotMatch(smoke, /(?:HOME|global|node_modules\/\.\.\/)/);
  assert.match(devshell, /ensureSharedPiPackageStore/);
  assert.match(devshell, /export PI_OFFLINE=.*PI_OFFLINE:-1/);
});

test("package resolution rejects mismatched identities and symlink escapes", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const manifest = path.join(fixture.packageRoot, "package.json");
  await writeFile(manifest, JSON.stringify({ name: "dev-loops", version: "9.9.9" }));
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: fixture.root }), /expected dev-loops@0\.9\.0/);

  await rm(fixture.packageRoot, { recursive: true, force: true });
  const outside = await realMkdtemp("oxid-dev-loop-outside-");
  t.after(() => rm(outside, { recursive: true, force: true }));
  await mkdir(path.join(outside, "cli"));
  await writeFile(path.join(outside, "cli", "index.mjs"), "");
  await writeFile(path.join(outside, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await symlink(outside, fixture.packageRoot, "dir");
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: fixture.root }), /escapes allowed project roots/);
});

test("package resolution rejects an unregistered path with borrowed worktree metadata", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const foreign = path.join(fixture.root, "tmp", "unregistered");
  await mkdir(foreign, { recursive: true });
  await writeFile(path.join(foreign, ".git"), await readFile(path.join(fixture.worktree, ".git")));
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: foreign }), /does not contain requested path|not registered/);
});

test("effective packaged agent allowlists use self-contained project shadows, not ineffective settings overrides", async (t) => {
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
    /unmodelled YAML frontmatter.*developer\.agent\.md/,
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

test("bounded frontmatter parser ignores unrelated YAML and strips inline comments", () => {
  const parsed = parseAgentFrontmatter([
    "---",
    "name: reviewer # effective name",
    "keywords:",
    "  - one",
    "description: >",
    "  folded content",
    "  - that is not a tools item",
    "tools: read, grep # keep bash out",
    "---",
  ].join("\n"), "fixture.agent.md");
  assert.equal(parsed.name, "reviewer");
  assert.deepEqual(parsed.tools, ["read", "grep"]);
  assert.throws(() => parseAgentFrontmatter("---\nname: reviewer\ntools: >\n  read, grep\n---\n", "unknown.agent.md"), /unmodelled YAML frontmatter/);
  assert.deepEqual(parseAgentFrontmatter("---\nname: inherited\nkeywords:\n  - one\n---\n", "inherited.agent.md"), {
    name: "inherited",
    tools: null,
  });
  assert.deepEqual(parseAgentFrontmatter("---\nname: none\ntools: []\n---\n", "none.agent.md"), {
    name: "none",
    tools: [],
  });
  assert.deepEqual(parseAgentFrontmatter('---\nname: quoted\ntools: "read, grep"\n---\n', "quoted.agent.md"), {
    name: "quoted",
    tools: ["read", "grep"],
  });
});

test("preflight scans all installed pinned package agents and content-invalidates its session key", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const settingsPath = path.join(fixture.root, ".pi", "settings.json");
  const settings = JSON.parse(await readFile(settingsPath, "utf8"));
  settings.packages.push("npm:pi-subagents@0.42.1", "npm:@input-output-hk/agent-review-pi@0.5.0");
  await writeFile(settingsPath, JSON.stringify(settings));
  const piSubagents = path.join(fixture.root, ".pi", "npm", "node_modules", "pi-subagents");
  const reviewPackage = path.join(fixture.root, ".pi", "npm", "node_modules", "@input-output-hk", "agent-review-pi");
  for (const [root, name, version] of [
    [piSubagents, "pi-subagents", "0.42.1"],
    [reviewPackage, "@input-output-hk/agent-review-pi", "0.5.0"],
  ]) {
    await mkdir(path.join(root, "agents"), { recursive: true });
    await writeFile(path.join(root, "package.json"), JSON.stringify({ name, version }));
  }
  await writeFile(path.join(piSubagents, "agents", "README.md"), "not an agent manifest\n");
  await writeFile(path.join(piSubagents, "agents", "developer.agent.md"), "---\nname: developer\ntools: [read, unavailable-shadowed-tool]\n---\n");
  await writeFile(path.join(piSubagents, "agents", "auditor.agent.md"), "---\nname: auditor\ntools: [read, unavailable-pi-tool]\n---\n");
  await writeFile(path.join(reviewPackage, "agents", "auditor.agent.md"), "---\nname: auditor\ntools: [read, unavailable-review-tool]\n---\n");

  const resolved = await resolveDevLoopsPackageRoot({ cwd: fixture.root, includeAllPinnedPackages: true });
  assert.deepEqual(resolved.packageRoots.map(({ name }) => name).sort(), [
    "@input-output-hk/agent-review-pi", "dev-loops", "pi-subagents",
  ]);
  const result = await checkAgentToolAllowlists({
    packageRoots: resolved.packageRoots,
    projectRoot: fixture.root,
    settings: resolved.settings,
    availableTools: supportedTools,
  });
  assert.equal(result.ok, false);
  const auditors = result.agents.filter(({ name }) => name === "auditor");
  assert.equal(auditors.length, 2, "every duplicate package manifest is validated when no project shadow exists");
  assert.deepEqual(auditors.map(({ missingTools }) => missingTools[0]).sort(), ["unavailable-pi-tool", "unavailable-review-tool"]);
  const developers = result.agents.filter(({ name }) => name === "developer");
  assert.equal(developers.length, 1, "one project shadow replaces every package manifest of the same name");
  assert.equal(developers[0].source, "project");
  assert.equal(developers[0].shadowsPackages, true);
  assert.deepEqual(developers[0].missingTools, []);

  const auditorManifest = path.join(reviewPackage, "agents", "auditor.agent.md");
  const originalInfo = await stat(auditorManifest);
  const originalSource = await readFile(auditorManifest, "utf8");
  const firstKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  const rewrittenSource = originalSource.replace("unavailable-review-tool", "unavailable-review-toom");
  assert.equal(Buffer.byteLength(rewrittenSource), Buffer.byteLength(originalSource));
  await writeFile(auditorManifest, rewrittenSource);
  await utimes(auditorManifest, originalInfo.atime, originalInfo.mtime);
  const rewrittenInfo = await stat(auditorManifest);
  assert.equal(rewrittenInfo.size, originalInfo.size);
  const secondKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  assert.notEqual(firstKey, secondKey, "same-size manifest rewrites with restored mtimes invalidate the session cache");

  const packageManifest = path.join(reviewPackage, "package.json");
  const packageInfo = await stat(packageManifest);
  const packageSource = await readFile(packageManifest, "utf8");
  const packageKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  const rewrittenPackage = packageSource.replace("0.5.0", "0.5.1");
  assert.equal(Buffer.byteLength(rewrittenPackage), Buffer.byteLength(packageSource));
  await writeFile(packageManifest, rewrittenPackage);
  await utimes(packageManifest, packageInfo.atime, packageInfo.mtime);
  assert.notEqual(await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools }), packageKey);

  const settingsInfo = await stat(settingsPath);
  const settingsSource = await readFile(settingsPath, "utf8");
  const settingsKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  const rewrittenSettings = settingsSource.replace("git-root", "git-roof");
  assert.equal(Buffer.byteLength(rewrittenSettings), Buffer.byteLength(settingsSource));
  await writeFile(settingsPath, rewrittenSettings);
  await utimes(settingsPath, settingsInfo.atime, settingsInfo.mtime);
  assert.notEqual(await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools }), settingsKey);

  await rm(reviewPackage, { recursive: true, force: true });
  const cliOnly = await resolveDevLoopsPackageRoot({ cwd: fixture.root });
  assert.equal(await realpath(cliOnly.packageRoot), await realpath(fixture.packageRoot), "CLI wrapper resolution does not couple to unrelated pins");
  await assert.rejects(
    resolveDevLoopsPackageRoot({ cwd: fixture.root, includeAllPinnedPackages: true }),
    /missing exact @input-output-hk\/agent-review-pi@0\.5\.0/,
  );
});

test("selected dev-loop hook validates current provider tools without edit/write false positives", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const handlers = new Map();
  const observedActiveTools = [];
  let activeTools = ["read", "grep", "find", "ls", "bash", "subagent"];
  const pi = {
    getAllTools: () => activeTools.map((name) => ({ name })),
    getActiveTools: () => [...activeTools],
    on: (event, handler) => handlers.set(event, handler),
  };
  registerDevLoopPreflight(pi, {
    env: { PI_SUBAGENT_CHILD_AGENT: "dev-loop" },
    resolve: async () => ({ packageRoot: fixture.packageRoot, gitRoot: fixture.root, settings: { subagents: { projectRootResolution: "git-root" } } }),
    check: async ({ activeTools: current }) => {
      observedActiveTools.push([...current]);
      return { ok: true, agents: [] };
    },
    cacheKey: async ({ activeTools: current }) => JSON.stringify(current),
  });
  const ctx = { cwd: fixture.root, ui: { notify: assert.fail }, abort: assert.fail };

  await handlers.get("before_provider_request")({}, ctx);
  activeTools = ["read", "bash"];
  await handlers.get("before_provider_request")({}, ctx);
  activeTools = ["read", "bash", "subagent"];
  await handlers.get("before_provider_request")({}, ctx);
  await handlers.get("before_agent_start")({ systemPromptOptions: { selectedTools: ["read", "bash"] } }, ctx);

  assert.deepEqual(observedActiveTools, [
    ["read", "grep", "find", "ls", "bash", "subagent"],
    ["read", "bash"],
    ["read", "bash", "subagent"],
    ["read", "bash"],
  ]);
});

test("tracked extension is idempotent and truthfully advisory on invalid allowlists", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const handlers = new Map();
  const notifications = [];
  const pi = {
    getAllTools: () => [{ name: "read" }],
    getActiveTools: () => ["read"],
    on: (event, handler) => handlers.set(event, [...(handlers.get(event) ?? []), handler]),
  };
  const runtime = {
    resolve: async () => { throw new Error("missing exact dev-loops@0.9.0"); },
  };
  registerDevLoopPreflight(pi, runtime);
  registerDevLoopPreflight(pi, runtime);
  for (const event of ["input", "before_agent_start", "before_provider_request"]) {
    assert.equal(handlers.get(event).length, 1, `${event} is registered once`);
  }
  const ctx = {
    cwd: fixture.root,
    ui: { notify: (message, level) => notifications.push({ message, level }) },
    abort: assert.fail,
  };
  assert.deepEqual(await handlers.get("input")[0]({}, ctx), { action: "continue" });
  await handlers.get("before_agent_start")[0]({}, ctx);
  await handlers.get("before_provider_request")[0]({}, ctx);
  assert.equal(notifications.length, 3);
  assert.match(notifications[0].message, /hooks cannot cancel agent or provider execution/);
  assert.match(notifications[1].message, /Advisory only.*no cancellation result/);
  assert.match(notifications[2].message, /Advisory only.*errors are swallowed/);
});

test("Nix-pinned Pi runner cannot hard-cancel a local fake provider through these hooks", async (t) => {
  const piRoot = await installedPiRoot(t);
  if (!piRoot) return;
  const [{ Agent }, { createAssistantMessageEventStream }, { fauxAssistantMessage }, extensions, { createEventBus }] = await Promise.all([
    import(new URL("./node_modules/@earendil-works/pi-agent-core/dist/agent.js", `file://${piRoot}/`)),
    import(new URL("./node_modules/@earendil-works/pi-ai/dist/utils/event-stream.js", `file://${piRoot}/`)),
    import(new URL("./node_modules/@earendil-works/pi-ai/dist/providers/faux.js", `file://${piRoot}/`)),
    import(new URL("./dist/core/extensions/index.js", `file://${piRoot}/`)),
    import(new URL("./dist/core/event-bus.js", `file://${piRoot}/`)),
  ]);
  const runtime = extensions.createExtensionRuntime();
  const extension = await extensions.loadExtensionFromFactory((pi) => {
    pi.on("before_agent_start", (_event, ctx) => ctx.abort());
    pi.on("before_provider_request", (_event, ctx) => {
      ctx.abort();
      throw new Error("attempted hard gate");
    });
  }, repoRoot, createEventBus(), runtime, "<hard-gate-fixture>");
  const runner = new extensions.ExtensionRunner([extension], runtime, repoRoot, {}, {});
  let activeTools = ["read"];
  let agent;
  runner.bindCore({
    getActiveTools: () => [...activeTools],
    getAllTools: () => [{ name: "read" }],
  }, {
    abort: () => agent?.abort(),
  });
  const runnerErrors = [];
  runner.onError((error) => runnerErrors.push(error));
  await runner.emitBeforeAgentStart("fixture", undefined, "system", { cwd: repoRoot, selectedTools: activeTools });

  let providerInvocations = 0;
  const streamFn = (model, _context, options) => {
    const stream = createAssistantMessageEventStream();
    queueMicrotask(async () => {
      await options.onPayload?.({ fixture: true }, model);
      providerInvocations += 1;
      const message = fauxAssistantMessage("local fake response");
      stream.push({ type: "done", reason: "stop", message });
      stream.end(message);
    });
    return stream;
  };
  agent = new Agent({
    initialState: {
      model: { id: "fake", name: "fake", api: "fake", provider: "local", baseUrl: "http://localhost.invalid", reasoning: false, input: ["text"], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 1024, maxTokens: 32 },
    },
    streamFn,
    onPayload: (payload) => runner.emitBeforeProviderRequest(payload),
  });
  await agent.prompt("fixture");
  assert.equal(providerInvocations, 1, "the real runner swallowed the hook throw and invoked the signal-ignoring fake provider");
  assert.equal(runnerErrors.some(({ event }) => event === "before_provider_request"), true);
  activeTools = [];
  assert.deepEqual(runner.getActiveTools(), []);
});

test("tracked project agents shadow every incompatible packaged dev-loops manifest", async () => {
  const upstreamDigests = {
    "dev-loop": "6a58bbcb79aaa27f037f5f15438afded916d66379bf7e21ba09913f89cb0a1f5",
    developer: "aaecd8859df4b561fbd46f5c05fe893b37f249e7ea52abd631dfe20de5b1fa90",
    docs: "eefeace5309224ef13fd271321b6137330a29bd2e973eb9a575bd4e4bc375912",
    fixer: "be0b42b4c280fac6912c13a066250280b746ecbb047f5adcfbe4c2b6f187cbe3",
    quality: "d52480ced74b3c695eb15f8d04da292d18300ed5f4eb29bab4f4011b82de28ec",
    refiner: "8563349bbf77d799b8c2db78696124799262ac3ceff8b14784002ccea6daae11",
    review: "2d3b46334b9fd5731f6ba0f081b5472b580e541d2d2ba56cf2b9ed2f90714acd",
  };
  const extensionFiles = (await readdir(path.join(repoRoot, ".pi", "extensions"))).filter((file) => file.startsWith("dev-loop-preflight"));
  assert.deepEqual(extensionFiles, ["dev-loop-preflight.ts"], "only the thin Pi registrar is auto-loaded");
  const settings = JSON.parse(await read(".pi/settings.json"));
  assert.equal(settings.packages.includes("npm:dev-loops@0.9.0"), true);
  assert.equal(settings.subagents.projectRootResolution, "git-root");
  assert.equal(settings.subagents.agentOverrides, undefined);
  for (const name of ["dev-loop", "developer", "docs", "fixer", "quality", "refiner", "review"]) {
    const source = await read(`.pi/agents/${name}.agent.md`);
    const toolsLine = source.split(/\r?\n/).find((line) => line.startsWith("tools:"));
    assert.ok(toolsLine, `${name} has a tracked project shadow`);
    const tools = toolsLine.slice("tools:".length).split(",").map((tool) => tool.trim());
    assert.equal(tools.some((tool) => legacyTools.has(tool)), false, `${name} has no legacy tool alias`);
    assert.match(source, /SPDX-License-Identifier: MIT/, `${name} preserves the upstream derived-content license`);
    assert.match(source, new RegExp(`Derived from dev-loops@0\\.9\\.0 agents/${name}\\.agent\\.md`), `${name} binds its source pin`);
    assert.match(source, new RegExp(`Upstream-SHA256: ${upstreamDigests[name]}`), `${name} binds exact upstream source bytes`);
    assert.doesNotMatch(source, /\]\(\.\.\/npm\/node_modules\//, `${name} has no link into an untracked package tree`);
    try {
      const upstream = await readFile(path.join(repoRoot, ".pi", "npm", "node_modules", "dev-loops", "agents", `${name}.agent.md`));
      assert.equal(createHash("sha256").update(upstream).digest("hex"), upstreamDigests[name], `${name} source digest matches installed pin`);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  const reviewTools = (await read(".pi/agents/review.agent.md")).match(/^tools:\s*(.+)$/m)?.[1] ?? "";
  assert.doesNotMatch(reviewTools, /\b(?:bash|edit|write)\b/, "review shadow exposes only read-only inspection tools");
  const devLoop = await read(".pi/agents/dev-loop.agent.md");
  assert.match(devLoop, /scripts\/dev-loops\.mjs/);
  assert.match(devLoop, /pre-flight-gate\.mjs --check-subagents.*before each later delegation or routed action/s);
  assert.match(devLoop, /gate coordination is authoritative for gate progression/);
  assert.match(devLoop, /run_draft_gate[\s\S]*requireCi: false/);
  assert.match(devLoop, /stop on every other contradiction/);
  assert.doesNotMatch(devLoop, /review-routing\.mjs|~\/.pi|npm root -g|require\.resolve\(['"]dev-loops|<dev-loops-package-root>\/cli\/index\.mjs/);
  const review = await read(".pi/agents/review.agent.md");
  assert.doesNotMatch(review, /\bgh api\b|\bgit (?:diff|log)\b/);
  assert.match(review, /gate-context artifact/i);
  for (const removedShadow of [
    "scripts/github/watch-ci.mjs",
    "scripts/lib/ci-check-selection.mjs",
    "scripts/lib/gate-evidence-repair.mjs",
    "scripts/lib/review-routing.mjs",
    "scripts/review/repair-gate-evidence.mjs",
  ]) {
    await assert.rejects(readFile(path.join(repoRoot, removedShadow)), { code: "ENOENT" });
  }
  const notices = await read("THIRD_PARTY_NOTICES.md");
  assert.match(notices, /dev-loops agent compatibility shadows/);
  assert.match(notices, /Copyright \(c\) 2026 mfittko/);
  assert.match(notices, /Permission is hereby granted, free of charge/);
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

test("pinned core resolution accepts bounded hoisted and nested package layouts", async (t) => {
  const root = await realMkdtemp("oxid-core-layout-");
  t.after(() => rm(root, { recursive: true, force: true }));
  const packageRoot = path.join(root, "node_modules", "dev-loops");
  await mkdir(packageRoot, { recursive: true });
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));

  async function installCore(coreRoot, version = "0.9.0") {
    const moduleRoot = path.join(coreRoot, "src", "loop");
    await mkdir(moduleRoot, { recursive: true });
    await writeFile(path.join(coreRoot, "package.json"), JSON.stringify({ name: "@dev-loops/core", version }));
    await writeFile(path.join(moduleRoot, "handoff-envelope.mjs"), "export const fixture = true;\n");
    return path.join(moduleRoot, "handoff-envelope.mjs");
  }

  const hoistedRoot = path.join(root, "node_modules", "@dev-loops", "core");
  const hoistedModule = await installCore(hoistedRoot);
  assert.equal(await resolvePinnedCoreModulePath(packageRoot), hoistedModule);

  await rm(hoistedRoot, { recursive: true, force: true });
  const nestedRoot = path.join(packageRoot, "node_modules", "@dev-loops", "core");
  const nestedModule = await installCore(nestedRoot);
  assert.equal(await resolvePinnedCoreModulePath(packageRoot), nestedModule);

  await writeFile(path.join(nestedRoot, "package.json"), JSON.stringify({ name: "@dev-loops/core", version: "0.9.1" }));
  await assert.rejects(resolvePinnedCoreModulePath(packageRoot), /expected @dev-loops\/core@0\.9\.0/);
});

test("repository wrappers force only the public PR-creation and managed-worktree routes", () => {
  assert.deepEqual(normalizeDevLoopsArgs(["--help"]), ["help"]);
  assert.deepEqual(normalizeDevLoopsArgs(["-h"]), ["help"]);
  assert.throws(() => normalizeDevLoopsArgs(["--help", "pr", "create"]), /unsupported leading/);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create", "--head", "topic"]), /--delivery-base is required/);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "create", "--head", "topic", "--delivery-base", "milestone-0.4.0"]), ["pr", "create", "--head", "topic", "--base", "milestone-0.4.0"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--silent", "pr", "create-draft", "--head", "topic", "--delivery-base=origin/milestone-0.5.0"]), ["--silent", "pr", "create-draft", "--head", "topic", "--base", "milestone-0.5.0"]);
  assert.deepEqual(normalizeDevLoopsArgs(["-s", "pr", "create", "--head", "topic", "--delivery-base", "develop"]), ["-s", "pr", "create", "--head", "topic", "--base", "develop"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--repo", "MediaNoxLabs/oxid", "pr", "create", "--head", "topic", "--delivery-base", "origin/develop"]), ["--repo", "MediaNoxLabs/oxid", "pr", "create", "--head", "topic", "--base", "develop"]);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "create", "--head", "feat/issue-280", "--base", "docs/issue-279", "--delivery-base", "develop"]), ["pr", "create", "--head", "feat/issue-280", "--base", "docs/issue-279"]);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create", "--head", "feat/issue-280", "--base", "main", "--delivery-base", "develop"]), /delivery target develop or a conventional issue branch/);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "ready-for-review", "--pr", "153"]), ["pr", "ready-for-review", "--pr", "153"]);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "edit", "--pr", "153", "--base", "milestone-0.4.0", "--delivery-base", "milestone-0.4.0"]), ["pr", "edit", "--pr", "153", "--base", "milestone-0.4.0"]);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "edit", "--pr", "153", "--base", "main", "--delivery-base", "develop"]), /must use develop/);
  assert.deepEqual(normalizeDevLoopsArgs(["queue", "add", "--title", "pr", "create"]), ["queue", "add", "--title", "pr", "create"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--jq", ".ok", "pr", "create", "--delivery-base", "milestone-1.0.0"]), ["--jq", ".ok", "pr", "create", "--base", "milestone-1.0.0"]);
  assert.throws(() => normalizeDevLoopsArgs(["--silent", "pr", "create", "--base", "main", "--delivery-base", "develop"]), /delivery target develop or a conventional issue branch/);
  assert.throws(() => normalizeDevLoopsArgs(["--future-global", "pr", "create", "--base", "main"]), /unsupported leading dev-loops@0\.9\.0 option/);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create-draft", "--base=integration", "--delivery-base", "milestone-0.4.0"]), /delivery target milestone-0\.4\.0 or a conventional issue branch/);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150"]), /--delivery-base is required/);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--delivery-base", "milestone-0.4.0"]), /--branch is required/);
  assert.deepEqual(normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--branch", "feat/issue-150", "--delivery-base", "milestone-0.4.0"]), ["--repo-root", "/repo", "--issue", "150", "--branch", "feat/issue-150", "--base", "origin/milestone-0.4.0"]);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--branch", "feat/issue-150", "--base", "origin/main", "--delivery-base", "develop"]), /must use origin\/develop/);
  assert.doesNotThrow(() => assertReviewedWorktreePin("0.9.0"));
  assert.throws(() => assertReviewedWorktreePin("0.9.1"), /supports only reviewed dev-loops@0\.9\.0/);
  assert.notStrictEqual(oxidConsumerProvision(), oxidConsumerProvision());
});

test("repository recovery path stays aligned with the real pinned dev-loops core", async (t) => {
  let resolved;
  try {
    resolved = await resolveDevLoopsPackageRoot({ cwd: repoRoot });
  } catch (error) {
    if (/missing exact dev-loops@/u.test(error.message)) {
      t.skip("project-local Pi packages are intentionally absent from public CI");
      return;
    }
    throw error;
  }
  const corePath = await resolvePinnedCoreModulePath(resolved.packageRoot);
  const core = await import(pathToFileURL(corePath).href);
  const packagePreflight = await readFile(path.join(resolved.packageRoot, "scripts", "loop", "pre-flight-gate.mjs"), "utf8");
  assert.match(packagePreflight, /\(creates\+provisions tmp\/worktrees\/dev-loops\/<kind>-<n> from origin\/main\)/u);
  for (const target of [
    { args: ["--issue", "194"], kind: "issue", number: 194 },
    { args: ["--pr", "204"], kind: "pr", number: 204 },
  ]) {
    assert.equal(
      resolveRepositoryWorktreePath(repoRoot, target.args),
      core.resolveWorktreePath({ repoRoot, kind: target.kind, number: target.number }),
    );
  }
});

test("linked worktree context handles issue/PR selectors, equals forms, main roots, and spaces", async (t) => {
  const root = await realMkdtemp("oxid-worktree-context-");
  const unusual = path.join(root, "checkout with spaces");
  const issueTarget = path.join(unusual, "tmp", "worktrees", "dev-loops", "issue-150");
  const prTarget = path.join(unusual, "tmp", "worktrees", "dev-loops", "pr-153");
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(unusual, { recursive: true });
  execFileSync("git", ["init", "--quiet"], { cwd: unusual });
  execFileSync("git", ["config", "user.name", "Fixture"], { cwd: unusual });
  execFileSync("git", ["config", "user.email", "fixture@example.invalid"], { cwd: unusual });
  await writeFile(path.join(unusual, "tracked"), "base\n");
  execFileSync("git", ["add", "tracked"], { cwd: unusual });
  execFileSync("git", ["commit", "--quiet", "-m", "base"], { cwd: unusual });
  execFileSync("git", ["worktree", "add", "--quiet", "-b", "issue-150", issueTarget], { cwd: unusual });
  execFileSync("git", ["worktree", "add", "--quiet", "-b", "pr-153", prTarget], { cwd: unusual });

  const issueRewritten = normalizeLinkedWorktreeContext([`--repo-root=${issueTarget}`, "--issue=150"]);
  assert.equal(optionAfter(issueRewritten, "--repo-root"), await realpath(unusual));
  assert.equal(optionAfter(issueRewritten, "--issue"), "150");
  const prRewritten = normalizeLinkedWorktreeContext(["--repo-root", prTarget, "--pr", "153"]);
  assert.equal(optionAfter(prRewritten, "--repo-root"), await realpath(unusual));
  assert.equal(optionAfter(prRewritten, "--pr"), "153");
  const mainRewritten = normalizeLinkedWorktreeContext(["--repo-root", unusual, "--issue", "150"]);
  assert.equal(optionAfter(mainRewritten, "--repo-root"), await realpath(unusual));
  assert.throws(
    () => normalizeLinkedWorktreeContext(["--repo-root", issueTarget, "--issue", "151"]),
    /refusing nested worktree creation.*canonical target/s,
  );
  assert.throws(
    () => normalizeLinkedWorktreeContext(["--repo-root", issueTarget, "--issue", "150", "--pr", "153"]),
    /exactly one --issue or --pr/,
  );
  assert.throws(() => normalizeLinkedWorktreeContext(["--repo-root", issueTarget, "--issue", "zero"]), /positive integer/);
  assert.throws(() => normalizeLinkedWorktreeContext(["--repo-root", issueTarget, "--issue"]), /--issue requires a value/);
  assert.throws(() => normalizeLinkedWorktreeContext(["--repo-root", "--issue", "150"]), /--repo-root requires a value/);
});

test("Oxid worktree creation applies zero consumer provisioning despite a dirty invalid primary config", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  await writeFile(path.join(fixture.root, ".devloops"), "invalid: [\n");
  assert.notEqual(execFileSync("git", ["status", "--porcelain", "--", ".devloops"], { cwd: fixture.root, encoding: "utf8" }), "");
  assert.equal(await readFile(path.join(fixture.worktree, ".devloops"), "utf8"), "version: 1\n");

  const stdout = [];
  const stderr = [];
  const stdoutSink = new Writable({ write(chunk, _encoding, callback) { stdout.push(chunk.toString()); callback(); } });
  const stderrSink = new Writable({ write(chunk, _encoding, callback) { stderr.push(chunk.toString()); callback(); } });
  assert.equal(await runEnsureWorktree(["--repo-root", fixture.root, "--issue", "150", "--branch", "issue-150", "--delivery-base", "develop"], {
    cwd: fixture.worktree,
    stdout: stdoutSink,
    stderr: stderrSink,
  }), 0);
  const result = JSON.parse(stdout.join("").trim().split("\n").at(-1));
  assert.deepEqual(result.provision, {
    ok: true,
    actions: [],
    summary: { copied: 0, linked: 0, skipped: 0, rejected: 0, warnings: 0 },
  });
  assert.doesNotMatch(stderr.join(""), /provision-worktree|packages\/core|WARN/);
  assert.equal(await lstat(path.join(fixture.worktree, "node_modules")).catch(() => null), null);
});

test("new managed worktrees gate only on host capacity admission", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const existing = await enforceFactoryAdmissionForCreation([
    "--repo-root", fixture.root, "--issue", "150", "--base", "origin/develop",
  ], {
    admissionAudit: async () => { throw new Error("audit must not run for an existing canonical worktree"); },
  });
  assert.equal(existing.reused, true);

  await mkdir(path.join(fixture.root, "tmp", "worktrees", "dev-loops", "issue-151"));
  let auditOptions;
  const admitted = await enforceFactoryAdmissionForCreation([
    "--repo-root", fixture.root, "--issue", "151", "--base", "origin/develop",
  ], {
    admissionAudit: async (options) => {
      auditOptions = options;
      return { admissionReady: true, checks: [{ id: "worktree-admission", status: "pass" }] };
    },
  });
  assert.equal(admitted.reused, false);
  assert.deepEqual(auditOptions, { repoRoot: await realpath(fixture.root) });

  await assert.rejects(enforceFactoryAdmissionForCreation([
    "--repo-root", fixture.root, "--issue", "152", "--base", "origin/develop",
  ], {
    admissionAudit: async () => ({
      admissionReady: false,
      checks: [
        { id: "worktree-admission", status: "fail" },
      ],
    }),
  }), /factory admission is blocked \(worktree-admission\).*--audit-pi/su);
});

test("Oxid worktree consumer preserves help, parse-error, conflict, jq, and silent output contracts", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const invoke = async (args) => {
    const stdout = [];
    const stderr = [];
    const stdoutSink = new Writable({ write(chunk, _encoding, callback) { stdout.push(chunk.toString()); callback(); } });
    const stderrSink = new Writable({ write(chunk, _encoding, callback) { stderr.push(chunk.toString()); callback(); } });
    const code = await runEnsureWorktree(args, { cwd: fixture.root, stdout: stdoutSink, stderr: stderrSink });
    return { code, stdout: stdout.join(""), stderr: stderr.join("") };
  };

  assert.deepEqual(await invoke(["--help"]), { code: 0, stdout: "fixture help\n", stderr: "" });
  assert.deepEqual(await invoke(["--issue", "150", "--branch", "issue-150", "--delivery-base", "develop"]), { code: 1, stdout: "", stderr: "Missing required --repo-root\n" });
  assert.deepEqual(await invoke(["--repo-root", fixture.root, "--issue", "150", "--branch", "conflict", "--delivery-base", "develop"]), { code: 1, stdout: "", stderr: "fixture branch conflict\n" });
  assert.deepEqual(await invoke(["--repo-root", fixture.root, "--issue", "150", "--branch", "issue-150", "--jq", ".ok", "--delivery-base", "develop"]), { code: 0, stdout: "true\n", stderr: "" });
  assert.deepEqual(await invoke(["--repo-root", fixture.root, "--issue", "150", "--branch", "issue-150", "--silent", "--delivery-base", "develop"]), { code: 0, stdout: "", stderr: "" });

  const helperLink = path.join(fixture.root, "tmp", "ensure-worktree-consumer-link.mjs");
  await symlink(path.join(repoRoot, "scripts", "loop", "ensure-worktree-consumer.mjs"), helperLink);
  assert.equal(execFileSync(process.execPath, [helperLink, "--help"], { cwd: fixture.root, encoding: "utf8" }), "fixture help\n");
});

function optionAfter(args, option) {
  const index = args.indexOf(option);
  return index >= 0 ? args[index + 1] : args.find((arg) => arg.startsWith(`${option}=`))?.slice(option.length + 1);
}

function validEnvelope(target, cwd) {
  return {
    handoffVersion: 1,
    target,
    nextAction: "fixture action",
    requiredReads: ["fixture.md"],
    acceptance: { criteria: [{ id: "fixture", must: "pass", severity: "required" }] },
    stopRules: [],
    executionMode: "bounded_handoff",
    asyncStartMode: "required",
    asyncStartEffective: "required",
    cwd,
  };
}

async function makeEnvelopeGitFixture(t) {
  const parent = await realMkdtemp("oxid-envelope-topology-");
  const root = path.join(parent, "checkout with spaces");
  const namespace = path.join(root, "tmp", "worktrees", "dev-loops");
  t.after(() => rm(parent, { recursive: true, force: true }));
  await mkdir(root, { recursive: true });
  execFileSync("git", ["init", "--quiet"], { cwd: root });
  execFileSync("git", ["config", "user.name", "Fixture"], { cwd: root });
  execFileSync("git", ["config", "user.email", "fixture@example.invalid"], { cwd: root });
  await writeFile(path.join(root, "tracked"), "base\n");
  execFileSync("git", ["add", "tracked"], { cwd: root });
  execFileSync("git", ["commit", "--quiet", "-m", "base"], { cwd: root });
  const worktrees = {
    issue150: path.join(namespace, "issue-150"),
    issue151: path.join(namespace, "issue-151"),
    pr153: path.join(namespace, "pr-153"),
    phase150: path.join(namespace, "phase-150-issue-150"),
    phase151: path.join(namespace, "phase-151-other"),
  };
  for (const [branch, target] of Object.entries(worktrees)) {
    execFileSync("git", ["worktree", "add", "--quiet", "-b", `fixture-${branch}`, target], { cwd: root });
  }
  return { parent, root: await realpath(root), namespace, worktrees };
}

test("handoff envelope cwd normalization uses owned canonical Git topology", async (t) => {
  const fixture = await makeEnvelopeGitFixture(t);
  const resolve = (gitRoot) => ({ gitRoot, commonRoot: fixture.root });
  const issue = { kind: "issue", repo: "owner/repo", issue: 150 };
  const pr = { kind: "pr", repo: "owner/repo", pr: 153 };
  const phase = { kind: "local_phase", repo: "owner/repo", issue: 150, phase: "issue-150" };

  const prospectiveIssueCwd = handoffCore.resolveWorktreePath({ repoRoot: fixture.root, kind: "issue", number: 999 });
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope({ ...issue, issue: 999 }, prospectiveIssueCwd), resolve(fixture.root), handoffCore,
  )).cwd, prospectiveIssueCwd);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(pr, fixture.worktrees.pr153), resolve(fixture.root), handoffCore,
  )).cwd, fixture.worktrees.pr153);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(phase, fixture.worktrees.phase150), resolve(fixture.root), handoffCore,
  )).cwd, fixture.worktrees.phase150);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(issue, `${fixture.worktrees.issue150}/tmp/worktrees/dev-loops/issue-150`),
    resolve(fixture.worktrees.issue150), handoffCore,
  )).cwd, fixture.worktrees.issue150);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(pr, "ignored"), resolve(fixture.worktrees.pr153), handoffCore,
  )).cwd, fixture.worktrees.pr153);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(phase, "ignored"), resolve(fixture.worktrees.issue150), handoffCore,
  )).cwd, fixture.worktrees.issue150);
  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope(phase, "ignored"), resolve(fixture.worktrees.phase150), handoffCore,
  )).cwd, fixture.worktrees.phase150);

  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope({ ...issue, issue: 151 }, "ignored"), resolve(fixture.worktrees.issue150), handoffCore),
    /disagrees with resolver target/,
  );
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope(pr, "ignored"), resolve(fixture.worktrees.issue150), handoffCore),
    /disagrees with resolver target/,
  );
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope({ ...phase, issue: 151, phase: "other" }, "ignored"), resolve(fixture.worktrees.phase150), handoffCore),
    /disagrees with resolver target/,
  );
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope(issue, "ignored"), resolve(path.join(fixture.namespace, "issue-999")), handoffCore),
    /topology is absent/,
  );

  const alias = path.join(fixture.parent, "issue-150-alias");
  await symlink(fixture.worktrees.issue150, alias, "dir");
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope(issue, "ignored"), resolve(alias), handoffCore),
    /symlinked|realpath/,
  );
  const symlinkedTarget = path.join(fixture.namespace, "issue-997");
  await symlink(fixture.worktrees.issue150, symlinkedTarget, "dir");
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope({ ...issue, issue: 997 }, symlinkedTarget), resolve(fixture.root), handoffCore),
    /symlinked or realpath-mismatched/,
  );
  const foreign = path.join(fixture.namespace, "issue-998");
  await mkdir(foreign, { recursive: true });
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(validEnvelope({ ...issue, issue: 998 }, foreign), resolve(fixture.root), handoffCore),
    /foreign existing/,
  );
  await assert.rejects(
    normalizeHandoffEnvelopeCwd(
      validEnvelope(issue, path.join(fixture.worktrees.issue150, "tmp", "worktrees", "dev-loops", "issue-150")),
      resolve(fixture.root), handoffCore,
    ),
    /non-canonical handoff envelope cwd/,
  );
});

test("main-checkout handoff envelopes require target-derived authorization", async (t) => {
  const fixture = await makeEnvelopeGitFixture(t);
  const resolve = { gitRoot: fixture.root, commonRoot: fixture.root };
  const issue = { kind: "issue", repo: "owner/repo", issue: 150 };
  const prospectiveIssueCwd = handoffCore.resolveWorktreePath({ repoRoot: fixture.root, kind: "issue", number: 999 });
  const localBranchCwd = path.join(fixture.namespace, "fixture-topic");

  assert.equal((await normalizeHandoffEnvelopeCwd(
    validEnvelope({ kind: "local_branch", repo: "owner/repo", branch: "fixture/topic" }, localBranchCwd),
    resolve,
    handoffCore,
  )).cwd, localBranchCwd);

  const cases = [
    { name: "missing target", envelope: validEnvelope(undefined, prospectiveIssueCwd), error: /target kind 'missing'/ },
    { name: "absent cwd", envelope: validEnvelope(issue, undefined), error: /cwd is required/ },
    { name: "empty cwd", envelope: validEnvelope(issue, ""), error: /cwd is required/ },
    { name: "whitespace cwd", envelope: validEnvelope(issue, " \t "), error: /cwd is required/ },
    { name: "relative cwd", envelope: validEnvelope(issue, "tmp/worktrees/dev-loops/issue-150"), error: /cwd must be absolute/ },
    {
      name: "target disagreement",
      envelope: validEnvelope({ ...issue, issue: 999 }, fixture.worktrees.issue150),
      error: /does not match the resolver target/,
    },
    {
      name: "outside common root",
      envelope: validEnvelope(issue, path.join(fixture.parent, "outside-common-root")),
      error: /non-canonical handoff envelope cwd/,
    },
    {
      name: "unmodelled target",
      envelope: validEnvelope({ kind: "future_kind", repo: "owner/repo" }, path.join(fixture.namespace, "future")),
      error: /target kind 'future_kind'/,
    },
  ];
  for (const fixtureCase of cases) {
    await t.test(fixtureCase.name, async () => {
      await assert.rejects(
        normalizeHandoffEnvelopeCwd(fixtureCase.envelope, resolve, handoffCore),
        fixtureCase.error,
      );
    });
  }
});

async function installPinnedEnvelopeFixture(root, { nestedCore = false } = {}) {
  const packageRoot = path.join(root, ".pi", "npm", "node_modules", "dev-loops");
  const corePackageRoot = nestedCore
    ? path.join(packageRoot, "node_modules", "@dev-loops", "core")
    : path.join(root, ".pi", "npm", "node_modules", "@dev-loops", "core");
  const coreRoot = path.join(corePackageRoot, "src", "loop");
  await mkdir(path.join(packageRoot, "cli"), { recursive: true });
  await mkdir(path.join(packageRoot, "scripts", "loop"), { recursive: true });
  await mkdir(path.join(packageRoot, "scripts", "lib"), { recursive: true });
  await mkdir(coreRoot, { recursive: true });
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await writeFile(path.join(packageRoot, "cli", "index.mjs"), "");
  await writeFile(path.join(packageRoot, "scripts", "loop", "build-handoff-envelope.mjs"), [
    'import { readFile } from "node:fs/promises";',
    'import path from "node:path";',
    'const USAGE = "Usage: build-handoff-envelope.mjs --input <path>";',
    'function parseJsonText(source) {',
    '  try { return JSON.parse(source); } catch (error) { throw new Error(`Invalid JSON: ${error.message}`); }',
    '}',
    'function readOption(argv, index, name) {',
    '  const argument = argv[index];',
    '  if (argument === name) {',
    '    if (typeof argv[index + 1] !== "string") throw new Error(`${name} requires a value`);',
    '    return { value: argv[index + 1], consumed: 2 };',
    '  }',
    '  if (argument.startsWith(`${name}=`)) return { value: argument.slice(name.length + 1), consumed: 1 };',
    '  return null;',
    '}',
    'export function parseBuildHandoffEnvelopeCliArgs(argv) {',
    '  const options = { help: false, inputPath: undefined, gateState: undefined, overrides: undefined, repo: undefined, jq: undefined, silent: false };',
    '  for (let index = 0; index < argv.length;) {',
    '    const argument = argv[index];',
    '    if (argument === "--help" || argument === "-h") { options.help = true; return options; }',
    '    if (argument === "--silent" || argument === "-s") { options.silent = true; index += 1; continue; }',
    '    let matched = false;',
    '    for (const [flag, field] of [["--input", "inputPath"], ["--gate-state", "gateState"], ["--overrides", "overrides"], ["--repo", "repo"], ["--jq", "jq"]]) {',
    '      const option = readOption(argv, index, flag);',
    '      if (option) { options[field] = option.value; index += option.consumed; matched = true; break; }',
    '    }',
    '    if (!matched) throw new Error(`Unknown argument: ${argument}`);',
    '  }',
    '  if (!options.inputPath) throw new Error("--input <path> is required");',
    '  return options;',
    '}',
    'export async function buildHandoffEnvelopeCli(options, { adapter }) {',
    '  const cwd = adapter.getCwd();',
    '  const repoRoot = adapter.getRepoRoot();',
    '  const resolverOutput = parseJsonText(await readFile(path.resolve(cwd, options.inputPath), "utf8"));',
    '  const bundle = resolverOutput.bundle ?? {};',
    '  const artifact = bundle.activeArtifact ?? {};',
    '  const repo = options.repo ?? bundle.repoSlug ?? bundle.repo;',
    '  if (!repo) throw new Error("Repository slug could not be resolved");',
    '  const target = { ...artifact, repo };',
    '  let envelopeCwd = cwd;',
    '  if (target.kind === "issue") envelopeCwd = path.join(repoRoot, "tmp", "worktrees", "dev-loops", `issue-${target.issue}`);',
    '  if (target.kind === "pr") envelopeCwd = path.join(repoRoot, "tmp", "worktrees", "dev-loops", `pr-${target.pr}`);',
    '  let maxCopilotRounds;',
    '  try {',
    '    const config = await readFile(path.join(repoRoot, ".devloops"), "utf8");',
    '    const match = config.match(/^\\s*maxCopilotRounds:\\s*(\\d+)\\s*$/m);',
    '    if (match) maxCopilotRounds = Number(match[1]);',
    '  } catch (error) { if (error?.code !== "ENOENT") throw error; }',
    '  const gateState = options.gateState ? parseJsonText(options.gateState) : {};',
    '  const overrides = options.overrides ? parseJsonText(options.overrides) : undefined;',
    '  return {',
    '    handoffVersion: 1, target, nextAction: bundle.nextAction, requiredReads: [],',
    '    acceptance: { criteria: [] }, stopRules: [], executionMode: bundle.executionMode,',
    '    asyncStartMode: "required", asyncStartEffective: "required", cwd: envelopeCwd,',
    '    ...gateState, overrides, maxCopilotRounds,',
    '    sanctionedCommands: { createPr: "scripts/dev-loops.mjs pr create" },',
    '  };',
    '}',
    'export async function runCli(argv, { stdout, stderr, adapter }) {',
    '  try {',
    '    const options = parseBuildHandoffEnvelopeCliArgs(argv);',
    '    if (options.help) { stdout.write(`${USAGE}\\n`); return; }',
    '    const result = await buildHandoffEnvelopeCli(options, { adapter });',
    '    if (!options.silent) stdout.write(`${JSON.stringify(result)}\\n`);',
    '  } catch (error) { stderr.write(`${error.message}\\n`); process.exitCode = 1; }',
    '}',
  ].join("\n"));
  await writeFile(path.join(packageRoot, "scripts", "lib", "jq-output.mjs"), [
    'export function emitResult(result, { jq, silent, stdout, stderr }) {',
    '  let output = result;',
    '  if (jq !== undefined) {',
    '    if (jq !== ".cwd") { stderr.write(`${JSON.stringify({ ok: false, error: `--jq: unsupported filter ${jq}` })}\\n`); return 2; }',
    '    output = result.cwd;',
    '  }',
    '  if (silent) return 0;',
    '  stdout.write(`${typeof output === "string" ? output : JSON.stringify(output)}\\n`);',
    '  return 0;',
    '}',
  ].join("\n"));
  await writeFile(path.join(packageRoot, "scripts", "_core-helpers.mjs"), [
    'export function formatCliError(error) { return error instanceof Error ? error.message : String(error); }',
  ].join("\n"));
  await writeFile(path.join(corePackageRoot, "package.json"), JSON.stringify({ name: "@dev-loops/core", version: "0.9.0" }));
  await writeFile(path.join(coreRoot, "handoff-envelope.mjs"), [
    'import path from "node:path";',
    'export const WORKTREE_NAMESPACE = path.join("tmp", "worktrees", "dev-loops");',
    'export const resolveWorktreePath = ({ repoRoot, kind, number }) => path.join(repoRoot, WORKTREE_NAMESPACE, `${kind}-${number}`);',
    'export const buildWorktreeSlug = (target) => target.kind === "local_branch" ? target.branch.replaceAll("/", "-") : `phase-${target.issue}-${target.phase}`;',
    'export const validateHandoffEnvelope = (envelope) => ({ ok: typeof envelope.cwd === "string", errors: [] });',
  ].join("\n"));
  return { packageRoot, corePackageRoot };
}

function captureSink(chunks) {
  return new Writable({ write(chunk, _encoding, callback) { chunks.push(chunk.toString()); callback(); } });
}

async function writeEnvelopeDeliveryProfiles(root) {
  await writeFile(path.join(root, ".pi", "delivery-profiles.json"), JSON.stringify({
    defaultProfile: "production-ready",
    profiles: {
      prototype: {
        sloSeconds: { firstFeedback: 180, focusedIteration: 600 },
        closeoutFields: ["hypothesis", "result", "knownGaps"],
      },
      "production-ready": {},
    },
  }));
}

async function makeProspectiveEnvelopeRouteFixture(t, blockedAncestor) {
  const parent = await realMkdtemp("oxid-envelope-prospective-");
  const root = path.join(parent, "candidate checkout");
  const input = "resolver.json";
  t.after(() => rm(parent, { recursive: true, force: true }));
  await mkdir(path.join(root, ".pi"), { recursive: true });
  await writeFile(path.join(root, ".pi", "settings.json"), JSON.stringify({ packages: ["npm:dev-loops@0.9.0"] }));
  await writeEnvelopeDeliveryProfiles(root);
  await writeFile(path.join(root, ".devloops"), "version: 1\n");
  execFileSync("git", ["init", "--quiet"], { cwd: root });
  execFileSync("git", ["config", "user.name", "Fixture"], { cwd: root });
  execFileSync("git", ["config", "user.email", "fixture@example.invalid"], { cwd: root });
  execFileSync("git", ["add", ".pi/settings.json", ".pi/delivery-profiles.json", ".devloops"], { cwd: root });
  execFileSync("git", ["commit", "--quiet", "-m", "base"], { cwd: root });
  await installPinnedEnvelopeFixture(root);
  await writeFile(path.join(root, input), JSON.stringify({
    bundle: {
      repoSlug: "owner/repo",
      selectedStrategy: "local_implementation",
      executionMode: "bounded_handoff",
      nextAction: "fixture action",
      activeArtifact: { kind: "issue", issue: 999 },
    },
  }));
  if (blockedAncestor === "tmp") {
    await writeFile(path.join(root, "tmp"), "not a directory\n");
  } else if (blockedAncestor === "namespace") {
    await mkdir(path.join(root, "tmp", "worktrees"), { recursive: true });
    await writeFile(path.join(root, "tmp", "worktrees", "dev-loops"), "not a directory\n");
  }
  return { root: await realpath(root), input, target: path.join(root, "tmp", "worktrees", "dev-loops", "issue-999") };
}

test("tracked build-envelope route rejects non-directory prospective topology before emission", async (t) => {
  for (const fixtureCase of [
    { name: "absent prospective path", blockedAncestor: null, accepted: true },
    { name: "common-root tmp file", blockedAncestor: "tmp", accepted: false },
    { name: "managed namespace file", blockedAncestor: "namespace", accepted: false },
  ]) {
    await t.test(fixtureCase.name, async (subtest) => {
      const fixture = await makeProspectiveEnvelopeRouteFixture(subtest, fixtureCase.blockedAncestor);
      const out = [];
      const err = [];
      const code = await runDevLoops(["loop", "build-envelope", "--input", fixture.input, "--delivery-base", "milestone-0.4.0"], {
        cwd: fixture.root,
        stdout: captureSink(out),
        stderr: captureSink(err),
      });
      if (fixtureCase.accepted) {
        assert.equal(code, 0, err.join(""));
        assert.equal(JSON.parse(out.join("")).cwd, fixture.target);
        assert.equal(err.join(""), "");
      } else {
        assert.equal(code, 1);
        assert.equal(out.join(""), "");
        assert.match(err.join(""), /non-directory ancestor/);
      }
    });
  }
});

test("tracked build-envelope route preserves pinned parser, config, and output contracts", async (t) => {
  const parent = await realMkdtemp("oxid-envelope-cli-");
  const root = path.join(parent, "candidate checkout with spaces");
  const issueTarget = path.join(root, "tmp", "worktrees", "dev-loops", "issue-150");
  t.after(() => rm(parent, { recursive: true, force: true }));
  await mkdir(path.join(root, ".pi"), { recursive: true });
  await writeFile(path.join(root, ".pi", "settings.json"), JSON.stringify({ packages: ["npm:dev-loops@0.9.0"] }));
  await writeEnvelopeDeliveryProfiles(root);
  await writeFile(path.join(root, ".devloops"), "version: 1\nrefinement:\n  maxCopilotRounds: 9\n");
  execFileSync("git", ["init", "--quiet"], { cwd: root });
  execFileSync("git", ["config", "user.name", "Fixture"], { cwd: root });
  execFileSync("git", ["config", "user.email", "fixture@example.invalid"], { cwd: root });
  execFileSync("git", ["add", ".pi/settings.json", ".pi/delivery-profiles.json", ".devloops"], { cwd: root });
  execFileSync("git", ["commit", "--quiet", "-m", "base"], { cwd: root });
  execFileSync("git", ["worktree", "add", "--quiet", "-b", "fixture-issue-150", issueTarget], { cwd: root });
  await writeFile(path.join(issueTarget, ".devloops"), "version: 1\nrefinement:\n  maxCopilotRounds: 2\n");
  await installPinnedEnvelopeFixture(root);
  const resolver = {
    bundle: {
      repoSlug: "owner/repo",
      selectedStrategy: "local_implementation",
      executionMode: "bounded_handoff",
      nextAction: "fixture action",
      activeArtifact: { kind: "local_phase", issue: 150, phase: "issue-150" },
    },
  };
  const input = "resolver output with spaces.json";
  await writeFile(path.join(issueTarget, input), JSON.stringify(resolver));

  async function run(args) {
    const out = [];
    const err = [];
    const code = await runDevLoops(args, { cwd: issueTarget, stdout: captureSink(out), stderr: captureSink(err) });
    return { code, out: out.join(""), err: err.join("") };
  }

  const split = await run([
    "--json", "loop", "build-envelope", "--input", input,
    "--delivery-base", "origin/milestone-0.4.0",
    "--gate-state", '{"currentHeadSha":"fixture-head","ciStatus":"success","unresolvedThreadCount":3}',
    "--overrides", '{"preferLocal":true}',
  ]);
  assert.equal(split.code, 0, split.err);
  const envelope = JSON.parse(split.out);
  assert.equal(envelope.cwd, issueTarget);
  assert.equal(envelope.currentHeadSha, "fixture-head");
  assert.equal(envelope.ciStatus, "success");
  assert.equal(envelope.unresolvedThreadCount, 3);
  assert.equal(envelope.target.repo, "owner/repo");
  assert.equal(envelope.deliveryProfile, "production-ready");
  assert.equal(envelope.deliveryBase, "origin/milestone-0.4.0");
  assert.equal(envelope.deliveryTargetKind, "milestone");
  assert.equal(envelope.requiredReads.includes(".pi/delivery-profiles.json"), true);
  assert.deepEqual(envelope.overrides, { preferLocal: true });
  assert.equal(envelope.maxCopilotRounds, 2);
  assert.ok(envelope.sanctionedCommands);

  const prototypeResult = await run([
    "loop", "build-envelope", `--input=${input}`, "--delivery-profile=prototype", "--delivery-base=develop",
  ]);
  assert.equal(prototypeResult.code, 0, prototypeResult.err);
  const prototype = JSON.parse(prototypeResult.out);
  assert.equal(prototype.deliveryProfile, "prototype");
  assert.equal(prototype.deliveryBase, "origin/develop");
  assert.equal(prototype.executionMode, "bounded_handoff");
  assert.equal(prototype.nextAction.includes("prototype hypothesis locally"), true);
  assert.deepEqual(prototype.stopRules, ["remote-mutation", "hosted-ci", "merge-readiness", "merge"]);
  assert.equal(Object.hasOwn(prototype, "gateConfig"), false);

  const equals = await run(["--jq=.cwd", "loop", "build-envelope", `--input=${input}`, "--repo=owner/repo", "--delivery-base=milestone-0.4.0"]);
  assert.deepEqual(equals, { code: 0, out: `${issueTarget}\n`, err: "" });
  const silent = await run(["-s", "loop", "build-envelope", `--input=${input}`, "--delivery-base=milestone-0.4.0"]);
  assert.deepEqual(silent, { code: 0, out: "", err: "" });
  assert.deepEqual(
    await run(["--silent", "loop", "build-envelope", `--input=${input}`, "--delivery-base=milestone-0.4.0"]),
    { code: 0, out: "", err: "" },
  );
  const help = await run(["loop", "build-envelope", "--help"]);
  assert.equal(help.code, 0);
  assert.match(help.out, /Usage: build-handoff-envelope/);
  assert.match(help.out, /--delivery-base <origin\/develop\|origin\/milestone-x\.y\.z>/u);
  assert.match(help.out, /--delivery-profile <prototype\|production-ready>/u);
  const badJq = await run(["loop", "build-envelope", `--input=${input}`, "--jq", "unsupported", "--delivery-base", "milestone-0.4.0"]);
  assert.equal(badJq.code, 2);
  assert.match(badJq.err, /--jq/);
  const unknownProfile = await run(["loop", "build-envelope", `--input=${input}`, "--delivery-profile", "fast-ish", "--delivery-base", "milestone-0.4.0"]);
  assert.equal(unknownProfile.code, 1);
  assert.match(unknownProfile.err, /unknown delivery profile/u);
  await writeFile(path.join(issueTarget, "malformed.json"), "{");
  const malformed = await run(["loop", "build-envelope", "--input", "malformed.json", "--delivery-base", "milestone-0.4.0"]);
  assert.equal(malformed.code, 1);
  assert.match(malformed.err, /Invalid JSON/);
});

test("tracked pre-flight wrapper reports Pi child dispatch availability deterministically", async (t) => {
  assert.equal(inferSubagentAvailability({ PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" }), "1");
  assert.equal(inferSubagentAvailability({ PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "2", PI_SUBAGENT_MAX_DEPTH: "2" }), "0");
  assert.equal(inferSubagentAvailability({ DEVLOOPS_SUBAGENT_AVAILABLE: "1", PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "2", PI_SUBAGENT_MAX_DEPTH: "2" }), "0");
  assert.equal(inferSubagentAvailability({ DEVLOOPS_SUBAGENT_AVAILABLE: "0", PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" }), "1");
  assert.equal(inferSubagentAvailability({ DEVLOOPS_SUBAGENT_AVAILABLE: "1" }), "1");
  assert.equal(inferSubagentAvailability({ DEVLOOPS_SUBAGENT_AVAILABLE: "invalid" }), "0");
  for (const value of ["1", "0", "true", "false", "yes", " \t0 "]) {
    assert.throws(() => assertNoPreflightBypass({ DEVLOOPS_PREFLIGHT_BYPASS: value }), /BYPASS is not permitted/);
  }
  for (const value of [undefined, "", " \t "]) {
    assert.doesNotThrow(() => assertNoPreflightBypass({ DEVLOOPS_PREFLIGHT_BYPASS: value }));
  }

  await assert.rejects(
    runRepositoryPreflight("/unused", { DEVLOOPS_PREFLIGHT_BYPASS: "true" }),
    /BYPASS is not permitted/,
  );

  const injectedEnv = {
    PI_SUBAGENT_CHILD_AGENT: "injected",
    PI_SUBAGENT_CHILD: "1",
    PI_SUBAGENT_DEPTH: "0",
    PI_SUBAGENT_MAX_DEPTH: "1",
  };
  let observedScopes;
  const injectedCheck = await runDevLoopPreflight({
    getAllTools: () => ["read"],
    getActiveTools: () => ["read"],
  }, "/unused", {
    env: injectedEnv,
    resolve: async () => ({ packageRoot: "/unused", packageRoots: ["/unused"], gitRoot: "/unused", settings: {} }),
    check: async (scopes) => {
      observedScopes = scopes;
      return { ok: true };
    },
  });
  assert.equal(injectedCheck.ok, true);
  assert.equal(observedScopes.activeAgent, "injected");
  assert.ok(observedScopes.futureTools.includes("subagent"));

  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const script = path.join(fixture.packageRoot, "scripts", "loop", "pre-flight-gate.mjs");
  await writeFile(script, "process.stdout.write(JSON.stringify({available:process.env.DEVLOOPS_SUBAGENT_AVAILABLE,bypass:Object.hasOwn(process.env,'DEVLOOPS_PREFLIGHT_BYPASS')}) + '\\n');\n");
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  assert.equal(await runPreFlightGate(["--check-subagents"], {
    cwd: fixture.root,
    env: { ...process.env, DEVLOOPS_PREFLIGHT_BYPASS: " \t ", PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" },
    stdout: sink,
    stderr: sink,
  }), 0);
  assert.deepEqual(JSON.parse(output.join("")), { available: "1", bypass: false });
  await assert.rejects(runPreFlightGate([], {
    cwd: fixture.root,
    env: { ...process.env, DEVLOOPS_PREFLIGHT_BYPASS: "0" },
    stdout: sink,
    stderr: sink,
  }), /BYPASS is not permitted/);
  const trackedMode = execFileSync("git", ["ls-files", "--stage", "--", "scripts/loop/pre-flight-gate.mjs"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim().split(/\s+/, 1)[0];
  assert.equal(trackedMode, "100755");
  const repositoryCheck = await runRepositoryPreflight(repoRoot);
  if (repositoryCheck.ok) {
    assert.match(repositoryCheck.resolved.source, /^git-(?:root|common-root)$/);
  } else {
    assert.match(repositoryCheck.message, /missing exact dev-loops@0\.9\.0; checked only/);
  }
});

test("repository wrapper executes conventional help and delegates watch-ci unchanged", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const cli = path.join(fixture.packageRoot, "cli", "index.mjs");
  await writeFile(cli, "process.stdout.write(JSON.stringify(process.argv.slice(2)) + '\\n');\n");
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  assert.equal(await runDevLoops(["--help"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
  assert.equal(await runDevLoops(["--silent", "loop", "watch-ci", "--pr", "7"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
  assert.deepEqual(output.join("").trim().split("\n").map((line) => JSON.parse(line)), [
    ["help"],
    ["--silent", "loop", "watch-ci", "--pr", "7"],
  ]);
});

test("repository wrappers await child close and preserve trailing output", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  assert.equal(await runDevLoops(["gates"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
  assert.equal(await runEnsureWorktree(["--repo-root", fixture.root, "--issue", "150", "--branch", "trailing", "--delivery-base", "develop"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
  assert.equal(execFileSync("git", ["config", "--local", "--get", "branch.trailing.oxidDeliveryBase"], {
    cwd: fixture.root, encoding: "utf8",
  }).trim(), "origin/develop");
  assert.match(output.join(""), /dev-loop-out/);
  assert.match(output.join(""), /dev-loop-err/);
  assert.match(output.join(""), /worktree-out/);
  assert.match(output.join(""), /worktree-err/);
});

test("GitHub compatibility enforces the supported CLI floor and REST capabilities", async (t) => {
  assert.deepEqual(parseGhVersion("gh version 2.97.0 (2026-08-14)"), [2, 97, 0]);
  assert.deepEqual(assertMinimumGhVersion([2, 97, 0]), [2, 97, 0]);
  assert.throws(() => assertMinimumGhVersion([2, 96, 9]), /unsupported; require >= 2\.97\.0.*nix develop/);
  assert.throws(() => parseGhVersion("not gh"), /could not parse/);
  const links = normalizeTimelinePullRequests([
    { event: "cross-referenced", source: { issue: { number: 71, html_url: "https://github.com/MediaNoxLabs/oxid/pull/71", pull_request: { url: "https://api.github.com/repos/MediaNoxLabs/oxid/pulls/71" } } } },
  ], "MediaNoxLabs/oxid");
  assert.deepEqual(links.map((link) => link.number), [71]);
  assert.throws(() => assertTimelinePages([{ event: "cross-referenced" }]), /did not return paginated arrays/);
  assert.equal(bodyReferencesIssue("Closes #150", 150, "MediaNoxLabs/oxid"), true);
  assert.equal(bodyReferencesIssue("Fixes GH-150", 150, "MediaNoxLabs/oxid"), true);
  assert.equal(bodyReferencesIssue("Resolves MediaNoxLabs/oxid#150", 150, "MediaNoxLabs/oxid"), true);
  assert.equal(bodyReferencesIssue("Closes https://github.com/MediaNoxLabs/oxid/issues/150", 150, "MediaNoxLabs/oxid"), true);
  assert.equal(bodyReferencesIssue("Related to #150", 150, "MediaNoxLabs/oxid"), false);
  assert.equal(bodyReferencesIssue("Refs #150", 150, "MediaNoxLabs/oxid"), true);
  assert.equal(bodyReferencesIssue("Refs other/repo#150", 150, "MediaNoxLabs/oxid"), false);

  const fixtureRoot = await realMkdtemp("oxid-gh-preflight-");
  t.after(() => rm(fixtureRoot, { recursive: true, force: true }));
  const fakeGh = path.join(fixtureRoot, "gh");
  await writeFile(fakeGh, `#!/usr/bin/env node
const args = process.argv.slice(2);
if (args[0] === "--version") process.stdout.write("gh version 2.97.0 (fixture)\\n");
else if (args.at(-1).endsWith("/timeline")) process.stdout.write(JSON.stringify([[{ event: "cross-referenced" }]]));
else if (args.at(-1).endsWith("/issues/150")) process.stdout.write(JSON.stringify({ number: 150 }));
else process.exit(9);
`);
  await chmod(fakeGh, 0o755);
  const probe = preflightGh({ repository: "MediaNoxLabs/oxid", issue: 150, ghCommand: fakeGh });
  assert.deepEqual(probe.version, [2, 97, 0]);
  assert.equal(probe.timelinePages, 1);
  await writeFile(fakeGh, (await readFile(fakeGh, "utf8")).replace(
    'JSON.stringify([[{ event: "cross-referenced" }]])',
    'JSON.stringify([{ event: "cross-referenced" }])',
  ));
  assert.throws(
    () => preflightGh({ repository: "MediaNoxLabs/oxid", issue: 150, ghCommand: fakeGh }),
    /did not return paginated arrays/,
  );
  assert.throws(
    () => resolveIssuePullRequestLinks({ repository: "MediaNoxLabs/oxid", issue: 150, ghCommand: fakeGh }),
    /did not return paginated arrays/,
  );

  const largeGh = path.join(fixtureRoot, "gh-large");
  const payloadBytes = 2 * 1024 * 1024;
  await writeFile(largeGh, `#!/usr/bin/env node\nprocess.stdout.write("x".repeat(${payloadBytes}));\n`);
  await chmod(largeGh, 0o755);
  const largeOutput = runGhCommand(largeGh, []);
  assert.equal(largeOutput.length, payloadBytes);
  assert.ok(GH_REST_MAX_BUFFER_BYTES > payloadBytes);

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

test("Claude invocation requires documented empty-tool semantics and structured output", () => {
  const invocation = buildClaudeInvocation({ schema: { type: "object" }, maxBudgetUsd: 10 });
  assert.ok(invocation.args.includes("--safe-mode"));
  const toolsIndex = invocation.args.indexOf("--tools");
  assert.ok(toolsIndex >= 0);
  assert.equal(invocation.args[toolsIndex + 1], "");
  assert.ok(invocation.args.includes("--no-session-persistence"));
  const effortIndex = invocation.args.indexOf("--effort");
  assert.ok(effortIndex >= 0);
  assert.equal(invocation.args[effortIndex + 1], DEFAULT_CLAUDE_REVIEW_EFFORT);
  assert.deepEqual(CLAUDE_REVIEW_EFFORTS, ["medium", "high", "xhigh", "max"]);
  assert.equal(assertAttestedReviewEffort("medium"), "medium");
  assert.throws(() => assertAttestedReviewEffort("low"), /must be one of: medium, high, xhigh, max/);
  assert.throws(() => buildClaudeInvocation({ effort: "unbounded" }), /must be one of/);
  assert.throws(() => buildClaudeInvocation({ effort: "low" }), /must be one of/);
  assert.match(new ClaudeReviewEvidenceVersionError(4).message, /upgrade the review wrapper/);
  assert.equal(assertClaudeReviewMaxBudgetUsd(0.01), 0.01);
  assert.equal(assertClaudeReviewMaxBudgetUsd("10"), 10);
  assert.equal(assertClaudeReviewMaxBudgetUsd(MAXIMUM_CLAUDE_REVIEW_BUDGET_USD), 10);
  assert.throws(() => assertClaudeReviewMaxBudgetUsd(0), /positive and no more than 10 USD/);
  assert.throws(() => assertClaudeReviewMaxBudgetUsd(11), /positive and no more than 10 USD/);
  assert.throws(() => assertClaudeReviewMaxBudgetUsd(Number.POSITIVE_INFINITY), /positive and no more than 10 USD/);
  const stringBudgetInvocation = buildClaudeInvocation({ maxBudgetUsd: "10" });
  assert.equal(stringBudgetInvocation.args[stringBudgetInvocation.args.indexOf("--max-budget-usd") + 1], "10");
  assert.deepEqual(parseClaudeVersion("2.1.228 (Claude Code)"), [2, 1, 228]);
  assert.deepEqual(assertMinimumClaudeVersion([2, 1, 228]), [2, 1, 228]);
  assert.throws(() => assertMinimumClaudeVersion([2, 1, 227]), /unsupported; require >= 2\.1\.228 and < 2\.2\.0/);
  assert.throws(() => assertMinimumClaudeVersion(MAXIMUM_EXCLUSIVE_CLAUDE_VERSION), /unsupported.*< 2\.2\.0/);
  const capabilities = assertClaudeHelpCapabilities(fixtureClaudeHelp, [2, 1, 228]);
  assert.equal(capabilities.emptyToolsDisabled, true);
  assert.equal(capabilities.emptyToolsBasis, "captured-help-and-bounded-version-contract");
  assert.equal(capabilities.permissionMode, "dontAsk");
  assert.equal(assertClaudeAuthHelpCapabilities(fixtureClaudeAuthHelp).jsonOutput, true);
  assert.throws(() => assertClaudeHelpCapabilities("  --safe-mode\n  --toolsfoo\n", [2, 1, 228]), /required review flags/);
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace(/^\s*--effort.*\n/m, ""), [2, 1, 228]),
    /required review flags: --effort/,
  );
  const duplicateEffortHelp = fixtureClaudeHelp.replace(
    "  --effort <level> (low, medium, high, xhigh, max)",
    "  --effort <level> (low, high)\n  --effort <level> (low, medium, high, xhigh, max)",
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(duplicateEffortHelp, [2, 1, 228]),
    /multiple --effort option blocks/,
  );
  const splitAliasHelp = fixtureClaudeHelp.replace("  --effort", "  -E,\n  --effort");
  assert.deepEqual(
    assertClaudeHelpCapabilities(splitAliasHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace("  --safe-mode", "  -s, --safe-mode"), [2, 1, 228]),
    /required review flags: --safe-mode/,
  );
  const crlfIndentedHelp = fixtureClaudeHelp
    .replace("  --safe-mode", "    --safe-mode")
    .replaceAll("\n", "\r\n");
  assert.equal(assertClaudeHelpCapabilities(crlfIndentedHelp, [2, 1, 228]).emptyToolsDisabled, true);
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace("--tools", "--TOOLS"), [2, 1, 228]),
    /required review flags: --tools/,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace('Use "" to disable all tools.', "Use defaults."), [2, 1, 228]),
    /no-tools form/,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace('"dontAsk", ', ""), [2, 1, 228]),
    /dontAsk permission mode/,
  );
  const reducedEfforts = assertClaudeHelpCapabilities(fixtureClaudeHelp.replace(", max", ""), [2, 1, 228]);
  assert.deepEqual(reducedEfforts.effortLevels, ["low", "medium", "high", "xhigh"]);
  assert.equal(assertClaudeEffortCapability("medium", reducedEfforts.effortLevels), "medium");
  assert.throws(
    () => assertClaudeEffortCapability("max", reducedEfforts.effortLevels),
    /does not document the selected review effort: max/,
  );
  const noDefaultEfforts = assertClaudeHelpCapabilities(
    fixtureClaudeHelp.replace("(low, medium, high, xhigh, max)", "(low, high)"),
    [2, 1, 228],
  );
  assert.deepEqual(noDefaultEfforts.effortLevels, ["low", "high"]);
  assert.equal(assertClaudeEffortCapability("high", noDefaultEfforts.effortLevels), "high");
  assert.throws(
    () => assertClaudeEffortCapability("medium", noDefaultEfforts.effortLevels),
    /does not document the selected review effort: medium/,
  );
  const reorderedEfforts = assertClaudeHelpCapabilities(
    fixtureClaudeHelp.replace(
      "(low, medium, high, xhigh, max)",
      '(default: medium) (choices: "max", "low", "xhigh", "medium", "high", "none")',
    ),
    [2, 1, 228],
  );
  assert.deepEqual(reorderedEfforts.effortLevels, ["max", "low", "xhigh", "medium", "high"]);
  const futureEffortHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    "(low, medium, high, xhigh, max, ultra)",
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(futureEffortHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  const describedEffortHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    "(low, medium, high, xhigh, max) Effort level for the session",
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(describedEffortHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  const aliasedMixedHelp = fixtureClaudeHelp.replace(
    "  --effort <level> (low, medium, high, xhigh, max)",
    '  -E, --effort <level> (choices: "low", "medium", "high", "xhigh", "max", default: "medium")',
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(aliasedMixedHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  const capturedEntryHelp = fixtureClaudeHelp.replace(
    "  --effort <level> (low, medium, high, xhigh, max)",
    capturedClaudeEffortEntry,
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(capturedEntryHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  assert.equal(
    assertClaudeHelpCapabilities(capturedEntryHelp, [2, 1, 228]).effortHelpEntry,
    capturedClaudeEffortEntry,
  );
  const wrappedChoicesHelp = fixtureClaudeHelp.replace(
    "  --effort <level> (low, medium, high, xhigh, max)",
    '  --effort <level> (choices: "low", "medium",\n      "high", "xhigh", "max")',
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(wrappedChoicesHelp, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  const followingAliasHelp = fixtureClaudeHelp.replace(
    "\n  --safe-mode",
    "\n  -ef, --environment <id> (foreign, modes)\n  --safe-mode",
  );
  const followingAliasCapabilities = assertClaudeHelpCapabilities(followingAliasHelp, [2, 1, 228]);
  assert.deepEqual(followingAliasCapabilities.effortLevels, fixtureClaudeCliEfforts);
  assert.doesNotMatch(followingAliasCapabilities.effortHelpEntry, /--environment/);
  const followingShortOnlyHelp = fixtureClaudeHelp
    .replace("(low, medium, high, xhigh, max)", "levels follow")
    .replace("\n  --safe-mode", "\n  -v <mode> (low, medium, high, xhigh, max)\n  --safe-mode");
  assert.throws(
    () => assertClaudeHelpCapabilities(followingShortOnlyHelp, [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  const unrelatedLatencyHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    "Effort profile (low, high) latency",
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(unrelatedLatencyHelp, [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  const commaProseHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    "(level for the session, see docs)",
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(commaProseHelp, [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  const enumerationBeforeDefault = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    '(low, medium, high, xhigh, max) (default: "medium")',
  );
  assert.deepEqual(
    assertClaudeHelpCapabilities(enumerationBeforeDefault, [2, 1, 228]).effortLevels,
    fixtureClaudeCliEfforts,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace("low", "Low"), [2, 1, 228]),
    /unsupported casing/,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace("(low, medium, high, xhigh, max)", "with a bounded level"), [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace("(low, medium, high, xhigh, max)", "(medium)"), [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  const effortLastHelp = [
    ...fixtureClaudeHelp.split("\n").filter((line) => !line.includes("--effort")),
    "  --effort <level> (default: medium)",
    "",
    "Examples: unrelated modes (low, medium, high, xhigh, max)",
  ].join("\n");
  assert.throws(
    () => assertClaudeHelpCapabilities(effortLastHelp, [2, 1, 228]),
    /recognizable review effort choice list/,
  );
  const conflictingEffortHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    "(low, medium, high) (low, medium, xhigh, max)",
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(conflictingEffortHelp, [2, 1, 228]),
    /multiple conflicting review effort choice lists/,
  );
  const explicitConflictHelp = fixtureClaudeHelp.replace(
    "(low, medium, high, xhigh, max)",
    '(choices: "low", "medium", "high", "xhigh", "max") (low, medium)',
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(explicitConflictHelp, [2, 1, 228]),
    /multiple conflicting review effort choice lists/,
  );
  assert.throws(() => assertClaudeAuthHelpCapabilities("Usage: claude auth status\n"), /default JSON output/);
  const calls = [];
  const capabilityProbe = probeClaudeCliCapabilities({
    claudeCommand: "fixture-claude",
    runner: (_command, args) => {
      calls.push(args);
      if (args[0] === "--version") return { status: 0, stdout: "2.1.228 (Claude Code)\n", stderr: "" };
      if (args[0] === "--help") return { status: 0, stdout: fixtureClaudeHelp, stderr: "" };
      if (args.at(-1) === "--help") return { status: 0, stdout: fixtureClaudeAuthHelp, stderr: "" };
      if (args.at(-1) === "--json") return { status: 0, stdout: JSON.stringify({ loggedIn: true }), stderr: "" };
      throw new Error(`unexpected fixture argv: ${args.join(" ")}`);
    },
  });
  assert.deepEqual(calls, [["--version"], ["--help"], ["auth", "status", "--help"], ["auth", "status", "--json"]]);
  assert.equal(capabilityProbe.authCapabilities.jsonOutput, true);
  assert.equal(capabilityProbe.outputSmoke, null, "capability contract probe does not invoke a model");
  const permissionIndex = invocation.args.indexOf("--permission-mode");
  assert.equal(invocation.args[permissionIndex + 1], "dontAsk");
  const parsed = parseClaudeReviewResult(JSON.stringify({ structured_output: { verdict: "clean", findings: [], summary: "No findings" }, session_id: "session-1" }));
  assert.equal(parsed.review.verdict, "clean");
  assert.equal(parsed.observedSessionId, "session-1");
  assert.throws(() => parseClaudeReviewResult(JSON.stringify({ result: "No findings" })), /structured review result/);
  assert.throws(() => parseClaudeReviewResult(JSON.stringify({ structured_output: { verdict: "clean", findings: [{ severity: "blocker", message: "bad" }] } })), /clean verdict cannot contain findings/);
});

test("Claude review CLI rejects invalid resource arguments before model execution", async () => {
  let help = "";
  await runClaudeReviewCli(["--help"], { stdout: { write(chunk) { help += chunk; } } });
  assert.match(help, /--timeout-ms INTEGER/);
  assert.match(help, /--timeout-ms 300000 \(five minutes\)/);
  assert.match(help, /Attested effort levels: medium, high, xhigh, max/);
  assert.match(help, /--max-budget-usd NUMBER/);
  assert.match(help, /--max-budget-usd 10/);
  assert.match(help, /Budget must be positive and no more than 10 USD/);

  await assert.rejects(
    runClaudeReviewCli([
      "--effort", "unbounded",
    ], { stdout: { write() {} } }),
    /effort must be one of/,
  );
  await assert.rejects(
    runClaudeReviewCli(["--effort", "low"]),
    /exact-head attestation effort must be one of/,
  );
  for (const invalidTimeout of ["", "-1", "0", "1.5", "1e3"]) {
    await assert.rejects(
      runClaudeReviewCli([`--timeout-ms=${invalidTimeout}`]),
      /review timeout must use positive base-10 integer syntax/,
    );
  }
  await assert.rejects(
    runClaudeReviewCli(["--timeout-ms=300001"]),
    /review timeout must be an integer between 1 and 300000 milliseconds/,
  );
  await assert.rejects(
    runClaudeReviewCli(["--timeout-ms", "1"]),
    /--issue-contract-file is required/,
  );
  for (const invalidBudget of ["", "-1", "0", "NaN", "Infinity", "0x10", "1e6", " 10 "]) {
    await assert.rejects(
      runClaudeReviewCli([`--max-budget-usd=${invalidBudget}`]),
      /review max budget must use positive base-10 decimal syntax|review max budget must be positive and no more than 10 USD/,
    );
  }
  await assert.rejects(
    runClaudeReviewCli(["--max-budget-usd=10.01"]),
    /review max budget must be positive and no more than 10 USD/,
  );
  await assert.rejects(
    runClaudeReviewCli(["--max-budget-usd=0.50"]),
    /--issue-contract-file is required/,
  );
});

test("review migration note remains inside the Unreleased changelog section", async () => {
  const changelog = await read("CHANGELOG.md");
  const unreleased = changelog.indexOf("## [Unreleased]");
  const nextRelease = changelog.indexOf("\n## [", unreleased + 1);
  const unreleasedEnd = nextRelease < 0 ? changelog.length : nextRelease;
  const reviewMigration = changelog.indexOf("Exact-head Claude reviews now select and attest a bounded reasoning effort");
  assert.ok(unreleased >= 0 && reviewMigration > unreleased && reviewMigration < unreleasedEnd);
});

test("installed real Claude CLI smoke is explicit opt-in and never a default API dependency", async (t) => {
  if (process.env.OXID_CLAUDE_LIVE_SMOKE !== "1") {
    t.skip("set OXID_CLAUDE_LIVE_SMOKE=1 to opt into the authenticated billed capability smoke");
    return;
  }
  const cwd = await realMkdtemp("oxid-claude-capability-");
  t.after(() => rm(cwd, { recursive: true, force: true }));
  let probe;
  try {
    probe = probeClaudeCliCapabilities({ cwd, performOutputSmoke: true });
  } catch (error) {
    t.skip(`opt-in Claude smoke unavailable: ${error.message}`);
    return;
  }
  assert.equal(probe.accountStatus.loggedIn, true);
  assert.equal(probe.capabilities.emptyToolsDisabled, true);
  assert.equal(probe.outputSmoke.review.verdict, "clean");
  assert.ok(probe.outputSmoke.observedSessionId);
});

test("Claude runner binds clean exact-head evidence and rejects stale worktrees", async (t) => {
  const fixtureRoot = await realMkdtemp("oxid-claude-runner-");
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
  git("config", `branch.${git("branch", "--show-current")}.oxidDeliveryBase`, "origin/develop");
  await writeFile(path.join(repository, "contract.txt"), "base\n");
  git("add", "contract.txt");
  git("commit", "--quiet", "-m", "base");
  const baseSha = git("rev-parse", "HEAD");
  const origin = path.join(fixtureRoot, "origin.git");
  execFileSync("git", ["init", "--bare", "--quiet", origin]);
  git("remote", "add", "origin", origin);
  git("push", "--quiet", "origin", "HEAD:refs/heads/develop");
  git("update-ref", "refs/remotes/origin/develop", baseSha);
  await writeFile(path.join(repository, "contract.txt"), "base\nhead\n");
  git("add", "contract.txt");
  git("commit", "--quiet", "-m", "head");
  const headSha = git("rev-parse", "HEAD");

  const writeFakeClaude = async (mode) => {
    await writeFile(fakeClaude, `#!/usr/bin/env node
const fs = require("node:fs");
const { execFileSync } = require("node:child_process");
const mode = ${JSON.stringify(mode)};
if (process.argv.includes("--version")) {
  process.stdout.write("2.1.228 (Claude Code)\\n");
} else if (process.argv[2] === "auth" && process.argv[3] === "status" && process.argv.includes("--help")) {
  process.stdout.write(${JSON.stringify(fixtureClaudeAuthHelp)});
} else if (process.argv.includes("--help")) {
  process.stdout.write(${JSON.stringify(fixtureClaudeHelp)});
} else if (process.argv[2] === "auth" && process.argv[3] === "status" && process.argv.includes("--json")) {
  process.stdout.write(JSON.stringify({ loggedIn: true, authMethod: "fixture", apiProvider: "fixture" }));
} else {
  const prompt = fs.readFileSync(0, "utf8");
  if (!prompt.includes("${headSha}") || !prompt.includes("${baseSha}")) process.exit(9);
  if (prompt.includes(${JSON.stringify(evidenceDir)})) process.exit(10);
  if (mode === "timeout") {
    execFileSync("git", ["-C", ${JSON.stringify(repository)}, "commit", "--allow-empty", "-m", "timeout-advance"]);
    setTimeout(() => {}, 60_000);
  }
  else if (mode === "nonzero") { process.stderr.write("fixture failure\\n"); process.exit(7); }
  else if (mode === "malformed") process.stdout.write("not json");
  else {
    if (mode.startsWith("cli-") && process.argv[process.argv.indexOf("--effort") + 1] !== mode.slice(4)) process.exit(11);
    if (mode === "advance") execFileSync("git", ["-C", ${JSON.stringify(repository)}, "commit", "--allow-empty", "-m", "advance"]);
    process.stdout.write(JSON.stringify({
      session_id: "fixture-session",
      structured_output: mode === "findings"
        ? { verdict: "findings", findings: [{ severity: "major", message: "Fixture finding" }], summary: "Has findings" }
        : { verdict: "clean", findings: [], summary: "No findings" },
    }));
  }
}
`);
    await chmod(fakeClaude, 0o755);
  };
  await writeFakeClaude("cli-high");

  const result = await runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
    effort: "high",
  });
  assert.equal(result.evidence.headSha, headSha);
  assert.equal(result.evidence.baseSha, baseSha);
  assert.equal(result.evidence.schemaVersion, 3);
  assert.equal(result.evidence.evidenceKind, "local-attestation");
  assert.equal(result.evidence.claude.observedSessionId, "fixture-session");
  assert.equal(result.evidence.claude.tools.length, 0);
  assert.equal(result.evidence.claude.capabilities.emptyToolsDisabled, true);
  assert.deepEqual(result.evidence.claude.capabilities.effortLevels, fixtureClaudeCliEfforts);
  assert.equal(result.evidence.invocation.effort, "high");
  assert.equal(result.evidence.invocation.minimumEffort, "medium");
  assert.equal(result.evidence.invocation.maximumTimeoutMs, 300_000);
  assert.equal(result.evidence.invocation.maximumBudgetUsd, 10);
  assert.match(result.evidence.limitations.join(" "), /do not authenticate reviewer identity/);
  assert.match(result.evidence.limitations.join(" "), /cannot prove the provider honored/);
  assert.equal(path.isAbsolute(result.evidence.diff.path), false);
  assert.equal(path.isAbsolute(result.evidence.rawResponse.path), false);
  assert.equal((await verifyClaudeReviewEvidence({ evidencePath: result.evidencePath, repoRoot: repository, fetchBase: false })).ok, true);
  assert.equal((await lstat(evidenceDir)).mode & 0o777, 0o700);
  for (const file of [
    result.evidencePath,
    path.join(evidenceDir, result.evidence.diff.path),
    path.join(evidenceDir, result.evidence.rawResponse.path),
    path.join(evidenceDir, result.evidence.claude.capabilities.help.path),
    path.join(evidenceDir, result.evidence.claude.capabilities.authHelp.path),
  ]) {
    assert.equal((await lstat(file)).mode & 0o777, 0o600);
  }
  const exactGitDiff = execFileSync("git", ["diff", "--binary", "--full-index", "--no-ext-diff", baseSha, headSha, "--"], { cwd: repository });
  assert.deepEqual(await readFile(path.join(evidenceDir, result.evidence.diff.path)), exactGitDiff);

  await t.test("CLI propagates explicit and default effort into isolated evidence", async () => {
    await writeFakeClaude("cli-high");
    const issueContractPath = path.join(fixtureRoot, "issue-150.json");
    await writeFile(issueContractPath, JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }));
    const cliScript = path.join(repoRoot, "scripts", "review", "claude-current-head.mjs");
    const cliEnvironment = { ...process.env, PATH: `${fixtureRoot}${path.delimiter}${process.env.PATH ?? ""}` };
    const runFixtureCli = ({ effort, evidenceName, timeoutMs }) => {
      const args = [
        cliScript,
        "--issue", "150",
        "--repo-root", repository,
        "--evidence-dir", path.join(fixtureRoot, evidenceName),
        "--issue-contract-file", issueContractPath,
        "--expected-head", headSha,
        "--delivery-base", "origin/develop",
      ];
      if (effort) args.push("--effort", effort);
      if (timeoutMs) args.push("--timeout-ms", timeoutMs);
      return JSON.parse(execFileSync(process.execPath, args, { encoding: "utf8", env: cliEnvironment }));
    };
    const highEvidence = runFixtureCli({
      effort: "high",
      evidenceName: "cli-high-evidence",
      timeoutMs: "300000",
    });
    assert.equal(highEvidence.invocation.effort, "high");
    assert.equal(highEvidence.invocation.timeoutMs, 300_000);
    assert.equal(highEvidence.verdict, "clean");

    await writeFakeClaude("cli-medium");
    const defaultEvidence = runFixtureCli({ evidenceName: "cli-default-evidence" });
    assert.equal(defaultEvidence.invocation.effort, "medium");
    assert.equal(defaultEvidence.invocation.timeoutMs, 300_000);
    assert.equal(defaultEvidence.verdict, "clean");
    await writeFakeClaude("clean");
  });

  await t.test("default five-minute timeout remains a hard failure", async () => {
    assert.equal(MAX_CLAUDE_REVIEW_TIMEOUT_MS, 300_000);
    await assert.rejects(
      runClaudeCurrentHeadReview({ issue: 150, timeoutMs: MAX_CLAUDE_REVIEW_TIMEOUT_MS + 1 }),
      /review timeout must be an integer between 1 and 300000 milliseconds/,
    );
    const defaultTimeouts = [];
    const timeoutRunner = (_command, args, options = {}) => {
      if (args[0] === "--version") return { status: 0, stdout: "2.1.228 (Claude Code)\n", stderr: "" };
      if (args[0] === "--help") return { status: 0, stdout: fixtureClaudeHelp, stderr: "" };
      if (args.at(-1) === "--help") return { status: 0, stdout: fixtureClaudeAuthHelp, stderr: "" };
      if (args.at(-1) === "--json") return { status: 0, stdout: JSON.stringify({ loggedIn: true }), stderr: "" };
      defaultTimeouts.push(options.timeout);
      return { status: null, stdout: "", stderr: "", signal: "SIGTERM", error: { code: "ETIMEDOUT" } };
    };
    await assert.rejects(
      runClaudeCurrentHeadReview({
        issue: 150,
        repoRoot: repository,
        evidenceDir: path.join(fixtureRoot, "timeout-evidence"),
        expectedHead: headSha,
        claudeCommand: fakeClaude,
        issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
        fetchBase: false,
        claudeRunner: timeoutRunner,
      }),
      /timed out or was terminated after 300000ms/,
    );
    assert.deepEqual(defaultTimeouts, [300_000]);
  });

  await t.test("run path rejects an effort omitted by captured CLI capabilities", async () => {
    let modelCalls = 0;
    const restrictedHelp = fixtureClaudeHelp.replace("(low, medium, high, xhigh, max)", "(low, medium)");
    const restrictedRunner = (_command, args) => {
      if (args[0] === "--version") return { status: 0, stdout: "2.1.228 (Claude Code)\n", stderr: "" };
      if (args[0] === "--help") return { status: 0, stdout: restrictedHelp, stderr: "" };
      if (args.at(-1) === "--help") return { status: 0, stdout: fixtureClaudeAuthHelp, stderr: "" };
      if (args.at(-1) === "--json") return { status: 0, stdout: JSON.stringify({ loggedIn: true }), stderr: "" };
      modelCalls += 1;
      return { status: 0, stdout: "", stderr: "" };
    };
    await assert.rejects(
      runClaudeCurrentHeadReview({
        issue: 150,
        repoRoot: repository,
        evidenceDir: path.join(fixtureRoot, "restricted-effort-evidence"),
        expectedHead: headSha,
        claudeCommand: fakeClaude,
        issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
        effort: "high",
        fetchBase: false,
        claudeRunner: restrictedRunner,
      }),
      /does not document the selected review effort: high/,
    );
    assert.equal(modelCalls, 0);
    const persisted = await readdir(path.join(fixtureRoot, "restricted-effort-evidence"));
    assert.ok(persisted.some((file) => file.endsWith(".claude-help.txt")));
    assert.ok(persisted.some((file) => file.endsWith(".claude-auth-help.txt")));
  });

  const legacyEvidencePath = path.join(evidenceDir, "legacy-v2.evidence.json");
  const legacyEvidence = structuredClone(result.evidence);
  legacyEvidence.schemaVersion = 2;
  delete legacyEvidence.invocation.effort;
  delete legacyEvidence.claude.capabilities.effortLevels;
  await writeFile(legacyEvidencePath, `${JSON.stringify(legacyEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: legacyEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    (error) => error instanceof ClaudeReviewEvidenceVersionError
      && error.code === "CLAUDE_REVIEW_EVIDENCE_VERSION"
      && /schema version 2; rerun/.test(error.message),
  );
  const legacyCliFailure = claudeReviewCliFailure(new ClaudeReviewEvidenceVersionError(2));
  assert.equal(legacyCliFailure.exitCode, 3);
  assert.deepEqual(JSON.parse(legacyCliFailure.output), {
    ok: false,
    code: "CLAUDE_REVIEW_EVIDENCE_VERSION",
    message: "unsupported Claude review attestation schema version 2; rerun the exact-head review",
  });
  const legacyCliProcess = spawnSync(process.execPath, [
    path.join(repoRoot, "scripts", "review", "claude-current-head.mjs"),
    "--verify-evidence", legacyEvidencePath,
    "--repo-root", repository,
  ], { encoding: "utf8" });
  assert.equal(legacyCliProcess.status, 3);
  assert.equal(legacyCliProcess.stdout, "");
  assert.deepEqual(JSON.parse(legacyCliProcess.stderr), {
    ok: false,
    code: "CLAUDE_REVIEW_EVIDENCE_VERSION",
    message: "unsupported Claude review attestation schema version 2; rerun the exact-head review",
  });

  const missingEffortEvidence = path.join(evidenceDir, "missing-effort.evidence.json");
  const missingEffort = structuredClone(result.evidence);
  delete missingEffort.invocation.effort;
  await writeFile(missingEffortEvidence, `${JSON.stringify(missingEffort)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: missingEffortEvidence,
      repoRoot: repository,
      fetchBase: false,
    }),
    /attestation is missing the selected effort/,
  );

  const missingMaximumTimeoutEvidencePath = path.join(evidenceDir, "missing-maximum-timeout.evidence.json");
  const missingMaximumTimeoutEvidence = structuredClone(result.evidence);
  delete missingMaximumTimeoutEvidence.invocation.maximumTimeoutMs;
  await writeFile(missingMaximumTimeoutEvidencePath, `${JSON.stringify(missingMaximumTimeoutEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: missingMaximumTimeoutEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the maximum review timeout/,
  );

  const missingMinimumEvidencePath = path.join(evidenceDir, "missing-minimum-effort.evidence.json");
  const missingMinimumEvidence = structuredClone(result.evidence);
  delete missingMinimumEvidence.invocation.minimumEffort;
  await writeFile(missingMinimumEvidencePath, `${JSON.stringify(missingMinimumEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: missingMinimumEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the minimum review effort/,
  );
  const mismatchedMinimumEffortEvidencePath = path.join(evidenceDir, "mismatched-minimum-effort.evidence.json");
  const mismatchedMinimumEffortEvidence = structuredClone(result.evidence);
  mismatchedMinimumEffortEvidence.invocation.minimumEffort = "max";
  await writeFile(mismatchedMinimumEffortEvidencePath, `${JSON.stringify(mismatchedMinimumEffortEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: mismatchedMinimumEffortEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the minimum review effort/,
  );

  const aboveCeilingBudgetEvidencePath = path.join(evidenceDir, "above-ceiling-budget.evidence.json");
  const aboveCeilingBudgetEvidence = structuredClone(result.evidence);
  aboveCeilingBudgetEvidence.invocation.maxBudgetUsd = 10.01;
  await writeFile(aboveCeilingBudgetEvidencePath, `${JSON.stringify(aboveCeilingBudgetEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: aboveCeilingBudgetEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /attestation records an unsupported review budget/,
  );
  for (const [name, value] of [["string", "10"], ["boolean", true], ["array", [10]]]) {
    const typedBudgetEvidencePath = path.join(evidenceDir, `${name}-budget.evidence.json`);
    const typedBudgetEvidence = structuredClone(result.evidence);
    typedBudgetEvidence.invocation.maxBudgetUsd = value;
    await writeFile(typedBudgetEvidencePath, `${JSON.stringify(typedBudgetEvidence)}\n`, { mode: 0o600 });
    await assert.rejects(
      verifyClaudeReviewEvidence({
        evidencePath: typedBudgetEvidencePath,
        repoRoot: repository,
        fetchBase: false,
      }),
      /attestation records an unsupported review budget/,
    );
  }

  const missingMaximumBudgetEvidencePath = path.join(evidenceDir, "missing-maximum-budget.evidence.json");
  const missingMaximumBudgetEvidence = structuredClone(result.evidence);
  delete missingMaximumBudgetEvidence.invocation.maximumBudgetUsd;
  await writeFile(missingMaximumBudgetEvidencePath, `${JSON.stringify(missingMaximumBudgetEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: missingMaximumBudgetEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the maximum review budget/,
  );
  const mismatchedMaximumBudgetEvidencePath = path.join(evidenceDir, "mismatched-maximum-budget.evidence.json");
  const mismatchedMaximumBudgetEvidence = structuredClone(result.evidence);
  mismatchedMaximumBudgetEvidence.invocation.maximumBudgetUsd = 9;
  await writeFile(mismatchedMaximumBudgetEvidencePath, `${JSON.stringify(mismatchedMaximumBudgetEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: mismatchedMaximumBudgetEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the maximum review budget/,
  );

  const invalidEffortEvidencePath = path.join(evidenceDir, "invalid-effort.evidence.json");
  const invalidEffortEvidence = structuredClone(result.evidence);
  invalidEffortEvidence.invocation.effort = "unbounded";
  await writeFile(invalidEffortEvidencePath, `${JSON.stringify(invalidEffortEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: invalidEffortEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /attestation records an unsupported selected effort/,
  );

  const belowFloorEffortEvidencePath = path.join(evidenceDir, "below-floor-effort.evidence.json");
  const belowFloorEffortEvidence = structuredClone(result.evidence);
  belowFloorEffortEvidence.invocation.effort = "low";
  await writeFile(belowFloorEffortEvidencePath, `${JSON.stringify(belowFloorEffortEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: belowFloorEffortEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /attestation records an unsupported selected effort/,
  );

  const missingCapabilityEvidencePath = path.join(evidenceDir, "missing-effort-capability.evidence.json");
  const missingCapabilityEvidence = structuredClone(result.evidence);
  const reducedHelp = fixtureClaudeHelp.replace(", high", "");
  const reducedHelpName = "missing-effort-capability.claude-help.txt";
  await writeFile(path.join(evidenceDir, reducedHelpName), reducedHelp, { mode: 0o600 });
  missingCapabilityEvidence.claude.capabilities.help = {
    path: reducedHelpName,
    sha256: createHash("sha256").update(reducedHelp).digest("hex"),
    bytes: Buffer.byteLength(reducedHelp),
  };
  missingCapabilityEvidence.claude.capabilities.effortLevels = ["low", "medium", "xhigh", "max"];
  missingCapabilityEvidence.claude.capabilities.effortHelpEntry = reducedHelp
    .split("\n")
    .find((line) => line.includes("--effort"));
  await writeFile(missingCapabilityEvidencePath, `${JSON.stringify(missingCapabilityEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: missingCapabilityEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the selected effort to the captured CLI capabilities/,
  );

  const tamperedHelpEntryEvidencePath = path.join(evidenceDir, "tampered-effort-help-entry.evidence.json");
  const tamperedHelpEntryEvidence = structuredClone(result.evidence);
  tamperedHelpEntryEvidence.claude.capabilities.effortHelpEntry += " altered";
  await writeFile(tamperedHelpEntryEvidencePath, `${JSON.stringify(tamperedHelpEntryEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: tamperedHelpEntryEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /does not bind the captured effort help entry/,
  );

  const overlongTimeoutEvidencePath = path.join(evidenceDir, "overlong-timeout.evidence.json");
  const overlongTimeoutEvidence = structuredClone(result.evidence);
  overlongTimeoutEvidence.invocation.timeoutMs = MAX_CLAUDE_REVIEW_TIMEOUT_MS + 1;
  await writeFile(overlongTimeoutEvidencePath, `${JSON.stringify(overlongTimeoutEvidence)}\n`, { mode: 0o600 });
  await assert.rejects(
    verifyClaudeReviewEvidence({
      evidencePath: overlongTimeoutEvidencePath,
      repoRoot: repository,
      fetchBase: false,
    }),
    /attestation records an unsupported review timeout/,
  );

  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 151,
    repoRoot: repository,
    evidenceDir,
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /issueContract does not match/);
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir: path.join(repository, "review-evidence"),
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /outside the reviewed checkout/);
  const priorStateHome = process.env.XDG_STATE_HOME;
  process.env.XDG_STATE_HOME = "relative/state";
  try {
    await assert.rejects(runClaudeCurrentHeadReview({
      issue: 150,
      repoRoot: repository,
      expectedHead: headSha,
      claudeCommand: fakeClaude,
      issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
      fetchBase: false,
    }), /XDG_STATE_HOME must be absolute/);
  } finally {
    if (priorStateHome === undefined) delete process.env.XDG_STATE_HOME;
    else process.env.XDG_STATE_HOME = priorStateHome;
  }

  await writeFile(path.join(repository, "opaque.bin"), Buffer.from([0, 255, 0, 254]));
  git("add", "opaque.bin");
  git("commit", "--quiet", "-m", "binary");
  const binaryHead = git("rev-parse", "HEAD");
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: binaryHead,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /binary paths.*opaque\.bin/);
  git("reset", "--hard", headSha);

  await writeFile(path.join(repository, "oversized.txt"), "x".repeat(MAX_REVIEW_DIFF_BYTES + 1));
  git("add", "oversized.txt");
  git("commit", "--quiet", "-m", "oversized");
  const oversizedHead = git("rev-parse", "HEAD");
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: oversizedHead,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), new RegExp(`exceeds the ${MAX_REVIEW_DIFF_BYTES}-byte`));
  git("reset", "--hard", headSha);

  await writeFakeClaude("findings");
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

  for (const [mode, pattern, timeoutMs] of [
    ["malformed", /did not return JSON/, 1_000],
    ["nonzero", /exited 7/, 1_000],
  ]) {
    await writeFakeClaude(mode);
    await assert.rejects(runClaudeCurrentHeadReview({
      issue: 150,
      repoRoot: repository,
      evidenceDir,
      expectedHead: headSha,
      claudeCommand: fakeClaude,
      issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
      fetchBase: false,
      timeoutMs,
    }), pattern, mode);
  }
  const sleepingClaude = path.join(fixtureRoot, "claude-sleeping");
  await writeFile(sleepingClaude, `#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\\n' '2.1.228 (Claude Code)'
elif [ "$1" = "auth" ] && [ "$2" = "status" ] && [ "$3" = "--help" ]; then
  cat <<'AUTH_HELP'
${fixtureClaudeAuthHelp}AUTH_HELP
elif [ "$1" = "--help" ]; then
  cat <<'CLAUDE_HELP'
${fixtureClaudeHelp}
CLAUDE_HELP
elif [ "$1" = "auth" ] && [ "$2" = "status" ] && [ "$3" = "--json" ]; then
  printf '%s' '{"loggedIn":true}'
else
  sleep 10
fi
`);
  await chmod(sleepingClaude, 0o755);
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: headSha,
    claudeCommand: sleepingClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
    timeoutMs: 500,
  }), /timed out or was terminated after 500ms/);
  git("reset", "--hard", headSha);

  await writeFakeClaude("advance");
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir,
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /changed during Claude review/);
  git("reset", "--hard", headSha);

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
  }), /real directory|symlink/);

  const unsafeAncestor = path.join(fixtureRoot, "unsafe-ancestor");
  await mkdir(unsafeAncestor, { mode: 0o777 });
  await chmod(unsafeAncestor, 0o777);
  await assert.rejects(runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir: path.join(unsafeAncestor, "evidence"),
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  }), /group\/world-writable without sticky protection/);

  const safeTarget = path.join(fixtureRoot, "safe-target");
  const symlinkAncestor = path.join(fixtureRoot, "symlink-ancestor");
  await mkdir(safeTarget, { mode: 0o700 });
  await symlink(safeTarget, symlinkAncestor, "dir");
  await writeFakeClaude("clean");
  const aliasedAncestorResult = await runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir: path.join(symlinkAncestor, "evidence"),
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  });
  assert.equal(aliasedAncestorResult.evidence.verdict, "clean", "safe resolved aliases such as macOS /var are admitted");
  assert.equal(await realpath(path.dirname(aliasedAncestorResult.evidencePath)), path.join(safeTarget, "evidence"));

  const stickyAncestor = path.join(fixtureRoot, "sticky-ancestor");
  await mkdir(stickyAncestor, { mode: 0o1777 });
  await chmod(stickyAncestor, 0o1777);
  await writeFakeClaude("clean");
  const stickyResult = await runClaudeCurrentHeadReview({
    issue: 150,
    repoRoot: repository,
    evidenceDir: path.join(stickyAncestor, "evidence"),
    expectedHead: headSha,
    claudeCommand: fakeClaude,
    issueContract: JSON.stringify({ issue: 150, title: "Fixture", body: "Contract" }),
    fetchBase: false,
  });
  assert.equal(stickyResult.evidence.verdict, "clean");

  const missingEvidence = path.join(fixtureRoot, "must-not-be-created", "evidence.json");
  await assert.rejects(
    verifyClaudeReviewEvidence({ evidencePath: missingEvidence, repoRoot: repository, fetchBase: false }),
    /ENOENT|no such file/i,
  );
  await assert.rejects(lstat(path.dirname(missingEvidence)), /ENOENT/);

  const malformedEvidence = path.join(evidenceDir, "malformed.evidence.json");
  await writeFile(malformedEvidence, JSON.stringify({
    ...result.evidence,
    diff: null,
  }), { mode: 0o600 });
  await chmod(malformedEvidence, 0o600);
  await assert.rejects(
    verifyClaudeReviewEvidence({ evidencePath: malformedEvidence, repoRoot: repository, fetchBase: false }),
    /missing artifact descriptors/,
  );

  await writeFile(path.join(repository, "dirty.txt"), "dirty\n");
  await assert.rejects(
    verifyClaudeReviewEvidence({ evidencePath: result.evidencePath, repoRoot: repository, fetchBase: false }),
    /clean worktree/,
  );
});

test("routine gates stay bounded and preserve the explicit high-risk review route", async () => {
  const config = await read(".devloops");
  assert.match(config, /^\s+maxCopilotRounds: 0$/m);
  for (const block of [
    config.slice(config.indexOf("  draft:"), config.indexOf("  preApproval:")),
    config.slice(config.indexOf("  preApproval:"), config.indexOf("  requireFanoutEvidence:")),
  ]) {
    assert.doesNotMatch(block, /^\s+- external-review$/m);
  }
  assert.match(config, /^  maxFanoutReviewers: 1$/m);
  assert.equal((config.match(/^    blockCleanOnFindingSeverities:\n      - must-fix$/gm) ?? []).length, 2);
  assert.match(config, /^  requireFanoutEvidence: false$/m);
  assert.match(config, /^  requireFanoutProvenance: false$/m);
  assert.match(config, /^  stopAt: \[\]$/m);
  assert.match(config, /^  humanMergeOnly: false$/m);
  assert.match(config, /^    mandatoryAngles: \[\]$/m);
  assert.match(await read("docs/dev-loop-stability.md"), /manually\s+invoke the reviewer once/i);
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
  assert.match(doc, /return\[0\]\.status \.\.\. undefined/);
  assert.match(doc, /upstream-only/i);
  assert.doesNotMatch(doc, /Before median|86\.4 \/ 89\.3 ms/);
  assert.match(doc, /do not (?:patch|modify|vendor)/i);
});

test("repository verification runs this stability contract", async () => {
  const run = await read("run.sh");
  assert.match(run, /node --test tests\/repository\/dev-loop-stability-contract\.test\.mjs/);
});
