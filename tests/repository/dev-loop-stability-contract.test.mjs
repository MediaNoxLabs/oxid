// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, lstat, mkdtemp, mkdir, readFile, readdir, realpath, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  checkAgentToolAllowlists,
  devLoopPreflightCacheKey,
  parseAgentFrontmatter,
  resolveDevLoopsPackageRoot,
} from "../../scripts/lib/dev-loop-runtime.mjs";
import { normalizeDevLoopsArgs, runDevLoops } from "../../scripts/dev-loops.mjs";
import { inferSubagentAvailability, runPreFlightGate } from "../../scripts/loop/pre-flight-gate.mjs";
import { normalizeLinkedWorktreeContext, normalizeWorktreeArgs, runEnsureWorktree } from "../../scripts/loop/ensure-worktree.mjs";
import { summarizeCurrentCi } from "../../scripts/lib/ci-check-selection.mjs";
import { validateFanoutRepairEvidence } from "../../scripts/lib/gate-evidence-repair.mjs";
import { resolveCanonicalReviewRoute } from "../../scripts/lib/review-routing.mjs";
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
  assertClaudeHelpCapabilities,
  assertMinimumClaudeVersion,
  MAXIMUM_EXCLUSIVE_CLAUDE_VERSION,
  buildClaudeInvocation,
  ClaudeReviewFindingsError,
  MAX_REVIEW_DIFF_BYTES,
  parseClaudeReviewResult,
  parseClaudeVersion,
  probeClaudeCliCapabilities,
  runClaudeCurrentHeadReview,
  verifyClaudeReviewEvidence,
} from "../../scripts/review/claude-current-head.mjs";
import registerDevLoopPreflight from "../../scripts/lib/dev-loop-preflight-core.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFile(path.join(repoRoot, relativePath), "utf8");
const legacyTools = new Set(["search", "execute", "agent", "todo"]);
const supportedTools = [
  "read", "grep", "find", "ls", "bash", "edit", "write", "subagent",
  "labels_bootstrap", "pr_approve_dep_upgrade", "pr_expedite", "pr_request_review",
  "pr_stabilize", "pr_watch", "review_claim", "review_complete", "review_create",
  "review_enrich", "review_list",
];

const fixtureClaudeHelp = [
  "  --print",
  "  --output-format <format>",
  "  --json-schema <schema>",
  "  --max-budget-usd <amount>",
  "  --safe-mode",
  '  --tools <tools...> Specify tools. Use "" to disable all tools.',
  "  --no-session-persistence",
  '  --permission-mode <mode> (choices: "acceptEdits", "dontAsk", "plan")',
  "  --system-prompt <prompt>",
].join("\n");
const fixtureClaudeAuthHelp = "Usage: claude auth status [options]\n  --json Output as JSON (default)\n";

async function realMkdtemp(prefix) {
  return realpath(await mkdtemp(path.join(os.tmpdir(), prefix)));
}

async function makeFixture() {
  const root = await realMkdtemp("oxid-dev-loop-root-");
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
  await writeFile(path.join(packageRoot, "cli", "index.mjs"), 'process.stdout.write("dev-loop-out\\n"); process.stderr.write("dev-loop-err\\n");\n');
  await mkdir(path.join(packageRoot, "scripts", "loop"), { recursive: true });
  await writeFile(path.join(packageRoot, "scripts", "loop", "ensure-worktree.mjs"), 'process.stdout.write("worktree-out\\n"); process.stderr.write("worktree-err\\n");\n');
  await writeFile(path.join(packageRoot, "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools: read, search, execute, bash, edit, write", "---", "fixture",
  ].join("\n"));
  await writeFile(path.join(root, ".pi", "agents", "developer.agent.md"), [
    "---", "name: developer", "description: fixture", "tools:", "  - read", "  - grep", "  - find", "  - ls", "  - bash", "  - edit", "  - write", "---", "fixture",
  ].join("\n"));
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
  const outside = await realMkdtemp("oxid-dev-loop-outside-");
  t.after(() => rm(outside, { recursive: true, force: true }));
  await mkdir(path.join(outside, "cli"));
  await writeFile(path.join(outside, "cli", "index.mjs"), "");
  await writeFile(path.join(outside, "package.json"), JSON.stringify({ name: "dev-loops", version: "0.9.0" }));
  await symlink(outside, fixture.packageRoot, "dir");
  await assert.rejects(resolveDevLoopsPackageRoot({ cwd: fixture.root }), /escapes allowed project roots/);
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

test("preflight scans all installed pinned package agents, ignores notes, and invalidates its mtime key", async (t) => {
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

  const firstKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  await writeFile(path.join(reviewPackage, "agents", "auditor.agent.md"), "---\nname: auditor\ntools: [read]\n---\nupdated\n");
  const secondKey = await devLoopPreflightCacheKey({ resolved, availableTools: supportedTools });
  assert.notEqual(firstKey, secondKey, "manifest size/mtime invalidates the per-session result cache");

  await rm(reviewPackage, { recursive: true, force: true });
  const cliOnly = await resolveDevLoopsPackageRoot({ cwd: fixture.root });
  assert.equal(await realpath(cliOnly.packageRoot), await realpath(fixture.packageRoot), "CLI wrapper resolution does not couple to unrelated pins");
  await assert.rejects(
    resolveDevLoopsPackageRoot({ cwd: fixture.root, includeAllPinnedPackages: true }),
    /missing exact @input-output-hk\/agent-review-pi@0\.5\.0/,
  );
});

test("selected dev-loop hook validates active and future-child tool scopes without the edit/write false positive", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  await writeFile(path.join(fixture.root, ".pi", "agents", "dev-loop.agent.md"), [
    "---", "name: dev-loop", "tools: read, grep, find, ls, bash, subagent", "---", "fixture",
  ].join("\n"));
  await writeFile(path.join(fixture.packageRoot, "agents", "dev-loop.agent.md"), [
    "---", "name: dev-loop", "tools: read, search, execute, bash, subagent", "---", "fixture",
  ].join("\n"));
  const settings = JSON.parse(await readFile(path.join(fixture.root, ".pi", "settings.json"), "utf8"));
  const handlers = new Map();
  const pi = {
    getAllTools: () => ["read", "grep", "find", "ls", "bash", "subagent"].map((name) => ({ name })),
    getActiveTools: () => ["read", "grep", "find", "ls", "bash", "subagent"],
    on: (event, handler) => handlers.set(event, handler),
  };
  let manifestRevision = 0;
  registerDevLoopPreflight(pi, {
    env: { PI_SUBAGENT_CHILD_AGENT: "dev-loop" },
    resolve: async () => ({ packageRoot: fixture.packageRoot, gitRoot: fixture.root, settings }),
    check: checkAgentToolAllowlists,
    cacheKey: async ({ activeAgent, activeTools, futureTools }) => JSON.stringify({ activeAgent, activeTools, futureTools, manifestRevision }),
  });
  const ctx = { cwd: fixture.root, ui: { notify: assert.fail }, abort() {} };
  await handlers.get("before_agent_start")({
    systemPromptOptions: { selectedTools: ["read", "grep", "find", "ls", "bash", "subagent"] },
  }, ctx);
  await handlers.get("before_provider_request")({}, ctx);

  await writeFile(path.join(fixture.packageRoot, "agents", "auditor.agent.md"), "---\nname: auditor\ntools: [read, web_search]\n---\n");
  manifestRevision += 1;
  await assert.rejects(handlers.get("before_provider_request")({}, ctx), /auditor@package:dev-loops:future-child=\[web_search\]/);
});

test("tracked extension registers once and blocks invalid allowlists before launch", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const settings = JSON.parse(await readFile(path.join(fixture.root, ".pi", "settings.json"), "utf8"));
  const handlers = new Map();
  const notifications = [];
  const pi = {
    getAllTools: () => [{ name: "read" }],
    on: (event, handler) => handlers.set(event, [...(handlers.get(event) ?? []), handler]),
  };
  const runtime = {
    env: { PI_SUBAGENT_CHILD_AGENT: "developer" },
    resolve: async () => ({ packageRoot: fixture.packageRoot, gitRoot: fixture.root, settings }),
    check: checkAgentToolAllowlists,
    cacheKey: async () => "fixture-invalid",
  };
  registerDevLoopPreflight(pi, runtime);
  registerDevLoopPreflight(pi, runtime);
  for (const event of ["input", "before_agent_start", "before_provider_request"]) {
    assert.equal(handlers.get(event).length, 1, `${event} is registered once`);
  }
  const ctx = {
    cwd: fixture.root,
    ui: { notify: (message) => notifications.push(message) },
    abortCalled: false,
    abort() { this.abortCalled = true; },
  };
  const inputResult = await handlers.get("input")[0]({}, ctx);
  assert.deepEqual(inputResult, { action: "continue" });
  assert.match(notifications[0], /unavailable repository\/package agent tools/);
  assert.match(notifications[0], /interactive input is allowed/);
  await assert.rejects(handlers.get("before_provider_request")[0]({}, ctx), /unavailable repository\/package agent tools/);
  assert.equal(ctx.abortCalled, true);
});

test("unprovisioned package keeps input interactive but blocks agent/provider launch", async () => {
  const handlers = new Map();
  const notifications = [];
  const pi = {
    getAllTools: () => [{ name: "read" }],
    on: (event, handler) => handlers.set(event, handler),
  };
  registerDevLoopPreflight(pi, {
    resolve: async () => { throw new Error("missing exact dev-loops@0.9.0"); },
  });
  const ctx = {
    cwd: repoRoot,
    ui: { notify: (message, level) => notifications.push({ message, level }) },
    abortCalled: false,
    abort() { this.abortCalled = true; },
  };
  assert.deepEqual(await handlers.get("input")({}, ctx), { action: "continue" });
  assert.equal(notifications[0].level, "warning");
  assert.match(notifications[0].message, /interactive input is allowed.*agent\/provider launch remains blocked/i);
  await handlers.get("before_agent_start")({}, ctx);
  assert.equal(ctx.abortCalled, true);
  await assert.rejects(handlers.get("before_provider_request")({}, ctx), /environment is not ready/);
});

test("manifest-shape drift preserves interactive recovery but blocks agent/provider launch", async () => {
  for (const detail of [
    "invalid YAML frontmatter in agent manifest package/agent.agent.md: unexpected list item",
    "agent manifest requires a non-empty name and tools allowlist: package/agent.agent.md",
  ]) {
    const handlers = new Map();
    const notifications = [];
    const pi = {
      getAllTools: () => [{ name: "read" }],
      on: (event, handler) => handlers.set(event, handler),
    };
    registerDevLoopPreflight(pi, { resolve: async () => { throw new Error(detail); } });
    const ctx = {
      cwd: repoRoot,
      ui: { notify: (message, level) => notifications.push({ message, level }) },
      abortCalled: false,
      abort() { this.abortCalled = true; },
    };
    assert.deepEqual(await handlers.get("input")({}, ctx), { action: "continue" });
    assert.equal(notifications[0].level, "warning");
    await handlers.get("before_agent_start")({}, ctx);
    assert.equal(ctx.abortCalled, true);
    await assert.rejects(handlers.get("before_provider_request")({}, ctx), /environment is not ready/);
  }
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
  assert.match(devLoop, /review-routing\.mjs.*never request or await Copilot/s);
  assert.doesNotMatch(devLoop, /~\/.pi|npm root -g|require\.resolve\(['"]dev-loops|<dev-loops-package-root>\/cli\/index\.mjs/);
  const review = await read(".pi/agents/review.agent.md");
  assert.doesNotMatch(review, /\bgh api\b|\bgit (?:diff|log)\b/);
  assert.match(review, /gate-context artifact/i);
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

test("repository wrappers force only the public PR-creation and managed-worktree routes", () => {
  assert.deepEqual(normalizeDevLoopsArgs(["--help"]), ["help"]);
  assert.deepEqual(normalizeDevLoopsArgs(["-h"]), ["help"]);
  assert.throws(() => normalizeDevLoopsArgs(["--help", "pr", "create"]), /unsupported leading/);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "create", "--head", "topic"]), ["pr", "create", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--silent", "pr", "create-draft", "--head", "topic"]), ["--silent", "pr", "create-draft", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["-s", "pr", "create", "--head", "topic"]), ["-s", "pr", "create", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--repo", "MediaNoxLabs/oxid", "pr", "create", "--head", "topic"]), ["--repo", "MediaNoxLabs/oxid", "pr", "create", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--repo=MediaNoxLabs/oxid", "--json", "pr", "create", "--head", "topic"]), ["--repo=MediaNoxLabs/oxid", "--json", "pr", "create", "--head", "topic", "--base", "integration"]);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "ready-for-review", "--pr", "153"]), ["pr", "ready-for-review", "--pr", "153"]);
  assert.deepEqual(normalizeDevLoopsArgs(["pr", "edit", "--pr", "153", "--base", "integration"]), ["pr", "edit", "--pr", "153", "--base", "integration"]);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "edit", "--pr", "153", "--base", "main"]), /must use integration/);
  assert.deepEqual(normalizeDevLoopsArgs(["queue", "add", "--title", "pr", "create"]), ["queue", "add", "--title", "pr", "create"]);
  assert.deepEqual(normalizeDevLoopsArgs(["--jq", ".ok", "pr", "create"]), ["--jq", ".ok", "pr", "create", "--base", "integration"]);
  assert.throws(() => normalizeDevLoopsArgs(["--silent", "pr", "create", "--base", "main"]), /must use integration/);
  assert.throws(() => normalizeDevLoopsArgs(["--repo", "MediaNoxLabs/oxid", "pr", "create", "--base", "main"]), /must use integration/);
  assert.throws(() => normalizeDevLoopsArgs(["--future-global", "pr", "create", "--base", "main"]), /unsupported leading dev-loops@0\.9\.0 option/);
  assert.throws(() => normalizeDevLoopsArgs(["pr", "create-draft", "--base=develop"]), /must use integration/);
  assert.deepEqual(normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150"]), ["--repo-root", "/repo", "--issue", "150", "--base", "origin/integration"]);
  assert.throws(() => normalizeWorktreeArgs(["--repo-root", "/repo", "--issue", "150", "--base", "origin/main"]), /must use origin\/integration/);
});

test("linked worktree context is rewritten to the main checkout and rejects nested targets", async (t) => {
  const root = await realMkdtemp("oxid-worktree-context-");
  const unusual = path.join(root, "checkout with spaces");
  const target = path.join(unusual, "tmp", "worktrees", "dev-loops", "issue-150");
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(unusual, { recursive: true });
  execFileSync("git", ["init", "--quiet"], { cwd: unusual });
  execFileSync("git", ["config", "user.name", "Fixture"], { cwd: unusual });
  execFileSync("git", ["config", "user.email", "fixture@example.invalid"], { cwd: unusual });
  await writeFile(path.join(unusual, "tracked"), "base\n");
  execFileSync("git", ["add", "tracked"], { cwd: unusual });
  execFileSync("git", ["commit", "--quiet", "-m", "base"], { cwd: unusual });
  execFileSync("git", ["worktree", "add", "--quiet", "-b", "issue-150", target], { cwd: unusual });

  const rewritten = normalizeLinkedWorktreeContext(["--repo-root", target, "--issue", "150"]);
  assert.equal(optionAfter(rewritten, "--repo-root"), await realpath(unusual));
  assert.equal(optionAfter(rewritten, "--issue"), "150");
  assert.throws(
    () => normalizeLinkedWorktreeContext(["--repo-root", target, "--issue", "151"]),
    /refusing nested worktree creation.*canonical target/s,
  );
});

function optionAfter(args, option) {
  const index = args.indexOf(option);
  return index >= 0 ? args[index + 1] : args.find((arg) => arg.startsWith(`${option}=`))?.slice(option.length + 1);
}

test("tracked pre-flight wrapper reports Pi child dispatch availability deterministically", async (t) => {
  assert.equal(inferSubagentAvailability({ PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" }), "1");
  assert.equal(inferSubagentAvailability({ PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "2", PI_SUBAGENT_MAX_DEPTH: "2" }), "0");
  assert.equal(inferSubagentAvailability({ DEVLOOPS_SUBAGENT_AVAILABLE: "0", PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" }), "0");
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const script = path.join(fixture.packageRoot, "scripts", "loop", "pre-flight-gate.mjs");
  await writeFile(script, "process.stdout.write(JSON.stringify({available:process.env.DEVLOOPS_SUBAGENT_AVAILABLE}) + '\\n');\n");
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  assert.equal(await runPreFlightGate(["--check-subagents"], {
    cwd: fixture.root,
    env: { ...process.env, PI_SUBAGENT_CHILD: "1", PI_SUBAGENT_DEPTH: "1", PI_SUBAGENT_MAX_DEPTH: "2" },
    stdout: sink,
    stderr: sink,
  }), 0);
  assert.deepEqual(JSON.parse(output.join("")), { available: "1" });
});

test("attempt-aware CI selection makes the newest same-name attempt authoritative", () => {
  const check = (id, status, conclusion) => ({ id, name: "PR title", app: { id: 1 }, status, conclusion, started_at: `2026-08-20T00:00:0${id}Z` });
  assert.equal(summarizeCurrentCi({ checkRuns: [check(1, "completed", "failure"), check(2, "completed", "success")], statuses: [] }).ciStatus, "success");
  assert.deepEqual(summarizeCurrentCi({ checkRuns: [check(1, "completed", "success"), check(2, "completed", "failure")], statuses: [] }), {
    ciStatus: "failure",
    failedChecks: [{ name: "PR title", conclusion: "failure" }],
  });
  assert.equal(summarizeCurrentCi({ checkRuns: [check(1, "completed", "failure"), check(2, "in_progress", null)], statuses: [] }).ciStatus, "pending");
  assert.equal(summarizeCurrentCi({
    checkRuns: [check(1, "completed", "failure"), check(2, "in_progress", null), { ...check(3, "completed", "failure"), name: "security" }],
    statuses: [],
  }).ciStatus, "failure", "a different current failure is not hidden by a retry");
});

test("repository watch-ci route applies attempt-aware selection end to end", async (t) => {
  const root = await realMkdtemp("oxid-watch-ci-");
  t.after(() => rm(root, { recursive: true, force: true }));
  const fakeGh = path.join(root, "gh");
  await writeFile(fakeGh, `#!/usr/bin/env node
const args = process.argv.slice(2);
if (args[0] === "--version") process.stdout.write("gh version 2.97.0 (fixture)\\n");
else if (args[0] === "pr") process.stdout.write(JSON.stringify({headRefOid:"${"a".repeat(40)}"}));
else if (args.at(-1).includes("check-runs")) process.stdout.write(JSON.stringify({check_runs:[
  {id:1,name:"PR title",app:{id:1},status:"completed",conclusion:"failure"},
  {id:2,name:"PR title",app:{id:1},status:"completed",conclusion:"success"}
]}));
else if (args.at(-1).includes("/status?")) process.stdout.write(JSON.stringify({statuses:[]}));
else process.exit(9);
`);
  await chmod(fakeGh, 0o755);
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  const priorPath = process.env.PATH;
  process.env.PATH = `${root}${path.delimiter}${priorPath ?? ""}`;
  try {
    assert.equal(await runDevLoops(["loop", "watch-ci", "--repo", "owner/repo", "--pr", "1", "--timeout-ms", "0"], {
      cwd: repoRoot,
      stdout: sink,
      stderr: sink,
    }), 0);
  } finally {
    if (priorPath === undefined) delete process.env.PATH;
    else process.env.PATH = priorPath;
  }
  assert.equal(JSON.parse(output.join("")).status, "success");
});

test("current-head inline evidence upgrade preserves findings and validates provenance", () => {
  const nowMs = Date.parse("2026-08-20T00:10:00Z");
  const headSha = "a".repeat(40);
  const existing = {
    visible: true,
    contractComplete: true,
    headSha,
    verdict: "clean",
    executionMode: "inline_single_agent",
    commentId: 42,
  };
  const reviewer = (reviewerId, angle, verdict = "clean", findings = []) => ({
    reviewerId,
    angle,
    verdict,
    findings,
    artifactSha256: reviewerId === "one" ? "1".repeat(64) : "2".repeat(64),
    completedAt: "2026-08-20T00:08:00Z",
  });
  const provenance = {
    schemaVersion: 1,
    gate: "draft_gate",
    headSha,
    generatedAt: "2026-08-20T00:09:00Z",
    reviewers: [reviewer("one", "scope"), reviewer("two", "security")],
  };
  assert.equal(validateFanoutRepairEvidence({
    existing,
    requested: { gate: "draft_gate", headSha, verdict: "clean" },
    provenance,
    nowMs,
  }).action, "upgrade");
  const findingsProvenance = {
    ...provenance,
    reviewers: [reviewer("one", "scope", "findings", [{ severity: "must-fix", summary: "bad" }]), reviewer("two", "security")],
  };
  assert.equal(validateFanoutRepairEvidence({
    existing,
    requested: { gate: "draft_gate", headSha, verdict: "findings" },
    provenance: findingsProvenance,
    nowMs,
  }).findingCount, 1);
  assert.throws(() => validateFanoutRepairEvidence({
    existing,
    requested: { gate: "draft_gate", headSha: "b".repeat(40), verdict: "clean" },
    provenance,
    nowMs,
  }), /exact current.*head SHA/);
  assert.throws(() => validateFanoutRepairEvidence({
    existing,
    requested: { gate: "draft_gate", headSha, verdict: "clean" },
    provenance: { ...provenance, reviewers: [reviewer("one", "scope"), reviewer("one", "security")] },
    nowMs,
  }), /identities.*distinct/);
  assert.equal(validateFanoutRepairEvidence({
    existing: { ...existing, executionMode: "fanout_fanin" },
    requested: { gate: "draft_gate", headSha, verdict: "clean" },
    provenance,
    nowMs,
  }).action, "noop");
  assert.throws(() => validateFanoutRepairEvidence({
    existing: { ...existing, verdict: "findings" },
    requested: { gate: "draft_gate", headSha, verdict: "clean" },
    provenance,
    nowMs,
  }), /cannot turn findings into clean/);
});

test("review routing has one canonical zero-round fallback and preserves required review", () => {
  assert.deepEqual(resolveCanonicalReviewRoute({ maxCopilotRounds: 0, mandatoryAngles: ["external-review"], copilotAvailable: false }), {
    route: "external-review",
    action: "run_independent_current_head_review",
    preservesRequiredReview: true,
  });
  assert.equal(resolveCanonicalReviewRoute({ maxCopilotRounds: 2, mandatoryAngles: ["external-review"], copilotAvailable: true }).route, "copilot");
  assert.throws(
    () => resolveCanonicalReviewRoute({ maxCopilotRounds: 0, mandatoryAngles: [], copilotAvailable: false }),
    /external-review is not mandatory/,
  );
});

test("repository wrappers await child close and preserve trailing output", async (t) => {
  const fixture = await makeFixture();
  t.after(() => rm(fixture.root, { recursive: true, force: true }));
  const output = [];
  const sink = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  assert.equal(await runDevLoops(["gates"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
  assert.equal(await runEnsureWorktree(["--issue", "150"], { cwd: fixture.root, stdout: sink, stderr: sink }), 0);
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
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace('Use "" to disable all tools.', "Use defaults."), [2, 1, 228]),
    /no-tools form/,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(fixtureClaudeHelp.replace('"dontAsk", ', ""), [2, 1, 228]),
    /dontAsk permission mode/,
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
  await writeFile(path.join(repository, "contract.txt"), "base\n");
  git("add", "contract.txt");
  git("commit", "--quiet", "-m", "base");
  const baseSha = git("rev-parse", "HEAD");
  git("update-ref", "refs/remotes/origin/integration", baseSha);
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
  await writeFakeClaude("clean");

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
  assert.equal(result.evidence.claude.capabilities.emptyToolsDisabled, true);
  assert.match(result.evidence.limitations.join(" "), /do not authenticate reviewer identity/);
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
