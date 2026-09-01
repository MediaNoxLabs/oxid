#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  chmod,
  mkdir,
  open,
  readFile,
  realpath,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { runManagedChild } from "../lib/managed-child-process.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const DEFAULT_POLICY_PATH = fileURLToPath(new URL("./policy.json", import.meta.url));
const EXPECTED_SCOPE_IDS = Object.freeze(["workspace-aggregate", "headless-host", "desktop-host"]);
const EXPECTED_COMMANDS = Object.freeze([
  [
    "cargo", "llvm-cov", "--workspace",
    "--exclude", "oxid-ui-dioxus",
    "--exclude", "oxid-app",
    "--exclude", "oxid-headless",
    "--json", "--fail-under-lines", "80",
  ],
  ["cargo", "llvm-cov", "-p", "oxid-headless", "--all-targets", "--json"],
  [
    "cargo", "llvm-cov", "-p", "oxid-ui-dioxus", "--all-targets",
    "--features", "ui-profile-demo,app-profile-authority", "--json",
  ],
]);
const CLASSIFICATION_KEYS = Object.freeze([
  "core",
  "critical",
  "workspaceOnly",
  "additionalScopes",
  "plainTestExclusions",
]);
const SHA_PATTERN = /^[0-9a-f]{40}$/u;

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function privateJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function assertClosedObject(value, expectedKeys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const expected = new Set(expectedKeys);
  for (const key of Object.keys(value)) {
    if (!expected.has(key)) throw new Error(`unknown policy key '${key}' in ${label}`);
  }
  for (const key of expectedKeys) {
    if (!Object.hasOwn(value, key)) throw new Error(`missing policy key '${key}' in ${label}`);
  }
}

function assertStringArray(value, label) {
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== "string" || entry.length === 0)) {
    throw new Error(`${label} must be an array of non-empty strings`);
  }
}

function normalizeRepoPath(candidate, repoRoot) {
  if (typeof candidate !== "string" || candidate.length === 0 || candidate.includes("\0")) {
    throw new Error("malformed LLVM coverage output: invalid source filename");
  }
  const absolute = path.isAbsolute(candidate) ? path.normalize(candidate) : path.resolve(repoRoot, candidate);
  const relative = path.relative(repoRoot, absolute);
  if (relative === "" || relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error(`LLVM coverage source is outside repository: ${candidate}`);
  }
  return relative.split(path.sep).join("/");
}

function normalizeLines(value, label) {
  if (!value || typeof value !== "object") throw new Error(`malformed LLVM coverage output: missing ${label}`);
  const { count, covered } = value;
  if (!Number.isInteger(count) || count < 0 || !Number.isInteger(covered) || covered < 0 || covered > count) {
    throw new Error(`malformed LLVM coverage output: invalid ${label}`);
  }
  return { count, covered };
}

function normalizedRelativePath(candidate, repoRoot) {
  const relative = path.relative(repoRoot, candidate);
  if (relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error("coverage output path escaped the repository");
  }
  return relative.split(path.sep).join("/");
}

function scopeMetadata(scope) {
  if (scope.id === "workspace-aggregate") {
    return { packages: ["workspace"], features: [], profile: "test" };
  }
  if (scope.id === "headless-host") {
    return { packages: ["oxid-headless"], features: [], profile: "all-targets" };
  }
  return {
    packages: ["oxid-ui-dioxus"],
    features: ["ui-profile-demo", "app-profile-authority"],
    profile: "all-targets",
  };
}

function parseWorkspaceMembers(manifest) {
  const match = manifest.match(/\[workspace\][\s\S]*?\nmembers\s*=\s*\[([\s\S]*?)\n\]/u);
  if (!match) throw new Error("workspace Cargo.toml has no readable members inventory");
  return [...match[1].matchAll(/"([^"\n]+)"/gu)].map((entry) => entry[1]);
}

function parsePackageName(manifest, manifestPath) {
  const section = manifest.match(/\[package\]([\s\S]*?)(?:\n\[|$)/u)?.[1];
  const name = section?.match(/^name\s*=\s*"([^"]+)"\s*$/mu)?.[1];
  if (!name) throw new Error(`workspace member has no readable package name: ${manifestPath}`);
  return name;
}

export async function discoverWorkspacePackages(repoRoot) {
  const rootManifest = await readFile(path.join(repoRoot, "Cargo.toml"), "utf8");
  const members = parseWorkspaceMembers(rootManifest);
  const packages = [];
  for (const member of members) {
    const manifestPath = path.join(repoRoot, member, "Cargo.toml");
    packages.push(parsePackageName(await readFile(manifestPath, "utf8"), manifestPath));
  }
  if (new Set(packages).size !== packages.length) throw new Error("workspace contains duplicate package names");
  return packages.sort();
}

export function validatePolicy(policy, workspacePackages) {
  assertClosedObject(policy, ["schemaVersion", "workspaceFloorPercent", "scopes", "classifications"], "policy");
  if (policy.schemaVersion !== 1) throw new Error("unsupported coverage policy schemaVersion");
  if (policy.workspaceFloorPercent !== 80) throw new Error("workspace coverage floor must remain 80 percent");
  if (!Array.isArray(policy.scopes) || policy.scopes.length !== EXPECTED_SCOPE_IDS.length) {
    throw new Error("policy scopes must contain exactly the three reviewed coverage scopes");
  }
  policy.scopes.forEach((scope, index) => {
    assertClosedObject(scope, ["id", "command"], `scope ${index}`);
    if (scope.id !== EXPECTED_SCOPE_IDS[index]) {
      throw new Error("coverage scopes are missing, unknown, or out of serial order");
    }
    assertStringArray(scope.command, `scope ${scope.id} command`);
    if (JSON.stringify(scope.command) !== JSON.stringify(EXPECTED_COMMANDS[index])) {
      throw new Error(`scope ${scope.id} command differs from the reviewed coverage contract`);
    }
  });
  const workspaceCommand = policy.scopes[0].command.join(" ");
  for (const excluded of ["oxid-ui-dioxus", "oxid-app", "oxid-headless"]) {
    if (!workspaceCommand.includes(`--exclude ${excluded}`)) {
      throw new Error(`workspace scope must explicitly exclude ${excluded}`);
    }
  }
  if (!workspaceCommand.includes("--fail-under-lines 80")) {
    throw new Error("workspace command must preserve the 80 percent floor");
  }

  assertClosedObject(policy.classifications, CLASSIFICATION_KEYS, "classifications");
  for (const key of ["core", "critical", "workspaceOnly"]) {
    assertStringArray(policy.classifications[key], `classifications.${key}`);
  }
  if (!Array.isArray(policy.classifications.additionalScopes)) {
    throw new Error("classifications.additionalScopes must be an array");
  }
  for (const [index, entry] of policy.classifications.additionalScopes.entries()) {
    assertClosedObject(entry, ["package", "scope"], `additionalScopes ${index}`);
    if (typeof entry.package !== "string" || !EXPECTED_SCOPE_IDS.slice(1).includes(entry.scope)) {
      throw new Error("malformed additional scope classification");
    }
  }
  if (!Array.isArray(policy.classifications.plainTestExclusions)) {
    throw new Error("classifications.plainTestExclusions must be an array");
  }
  for (const [index, entry] of policy.classifications.plainTestExclusions.entries()) {
    assertClosedObject(
      entry,
      ["package", "owner", "reason", "command", "removalCondition"],
      `plainTestExclusions ${index}`,
    );
    for (const key of ["package", "owner", "reason", "removalCondition"]) {
      if (typeof entry[key] !== "string" || entry[key].length === 0) {
        throw new Error(`malformed plain test exclusion ${key}`);
      }
    }
    assertStringArray(entry.command, "plain test exclusion command");
  }
  const plain = policy.classifications.plainTestExclusions;
  if (plain.length !== 1 || plain[0].package !== "oxid-app"
      || JSON.stringify(plain[0].command) !== JSON.stringify(["cargo", "test", "-p", "oxid-app"])) {
    throw new Error("oxid-app must remain the one narrow plain-test exclusion");
  }

  const classified = [
    ...policy.classifications.core,
    ...policy.classifications.critical,
    ...policy.classifications.workspaceOnly,
    ...policy.classifications.additionalScopes.map((entry) => entry.package),
    ...plain.map((entry) => entry.package),
  ];
  const seen = new Set();
  for (const packageName of classified) {
    if (seen.has(packageName)) throw new Error(`duplicate classification for package '${packageName}'`);
    seen.add(packageName);
  }
  const workspace = new Set(workspacePackages);
  for (const packageName of seen) {
    if (!workspace.has(packageName)) throw new Error(`unknown package classification '${packageName}'`);
  }
  for (const packageName of workspace) {
    if (!seen.has(packageName)) throw new Error(`missing classification for package '${packageName}'`);
  }
  return policy;
}

export function normalizeLlvmReport(raw, { repoRoot, scopeId }) {
  if (!raw || typeof raw !== "object" || !Array.isArray(raw.data) || raw.data.length !== 1) {
    throw new Error("malformed LLVM coverage output: expected one data record");
  }
  const data = raw.data[0];
  if (!data || typeof data !== "object" || !Array.isArray(data.files)) {
    throw new Error("malformed LLVM coverage output: missing files");
  }
  const totals = normalizeLines(data.totals?.lines, "total line counts");
  const files = data.files.map((file) => ({
    path: normalizeRepoPath(file?.filename, repoRoot),
    lines: normalizeLines(file?.summary?.lines, "file line counts"),
  })).sort((left, right) => left.path.localeCompare(right.path));
  if (new Set(files.map((file) => file.path)).size !== files.length) {
    throw new Error("malformed LLVM coverage output: duplicate normalized source path");
  }
  if (scopeId === "workspace-aggregate" && totals.covered * 100 < totals.count * 80) {
    throw new Error("workspace line coverage is below the 80 percent floor");
  }
  return { schemaVersion: 1, scope: scopeId, totals: { lines: totals }, files };
}

export function parseArguments(argv) {
  const parsed = { base: undefined, dryRun: false, policyPath: undefined };
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") {
      if (seen.has(argument)) throw new Error("--dry-run specified more than once");
      seen.add(argument);
      parsed.dryRun = true;
      continue;
    }
    if (argument === "--base" || argument === "--policy") {
      if (seen.has(argument)) throw new Error(`${argument} specified more than once`);
      seen.add(argument);
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${argument} requires a value`);
      index += 1;
      if (argument === "--base") parsed.base = value;
      else parsed.policyPath = value;
      continue;
    }
    throw new Error(`unknown argument: ${argument}`);
  }
  if (!parsed.base) throw new Error("--base is required");
  return parsed;
}

export async function acquireCoverageLock(lockPath, receipt) {
  await mkdir(path.dirname(lockPath), { recursive: true, mode: 0o700 });
  const token = randomUUID();
  let handle;
  try {
    handle = await open(lockPath, "wx", 0o600);
  } catch (error) {
    if (error?.code === "EEXIST") {
      throw new Error(`coverage lock already exists at ${path.basename(lockPath)}; ownership may be active or stale`);
    }
    throw error;
  }
  try {
    await handle.writeFile(privateJson({ schemaVersion: 1, token, ...receipt }), "utf8");
    await handle.sync();
    await handle.close();
    await chmod(lockPath, 0o600);
  } catch (error) {
    await handle?.close().catch(() => {});
    await unlink(lockPath).catch(() => {});
    throw error;
  }
  let released = false;
  return async () => {
    if (released) return;
    const current = JSON.parse(await readFile(lockPath, "utf8"));
    if (current.token !== token) throw new Error("coverage lock ownership changed; refusing to remove it");
    await unlink(lockPath);
    released = true;
  };
}

function defaultGit(repoRoot) {
  return {
    resolve(ref) {
      return execFileSync("git", ["rev-parse", "--verify", `${ref}^{commit}`], {
        cwd: repoRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      }).trim();
    },
    status() {
      return execFileSync("git", ["status", "--porcelain=v1", "--untracked-files=all"], {
        cwd: repoRoot,
        encoding: "utf8",
      });
    },
    isAncestor(base, head) {
      return spawnSync("git", ["merge-base", "--is-ancestor", base, head], { cwd: repoRoot }).status === 0;
    },
  };
}

function resolveRef(git, ref, label) {
  try {
    const resolved = git.resolve(ref);
    if (!SHA_PATTERN.test(resolved)) throw new Error("not a full commit id");
    return resolved;
  } catch (error) {
    throw new Error(`could not resolve ${label} '${ref}': ${error.message}`);
  }
}

function initialSourceState(git, baseRef) {
  const sourceHead = resolveRef(git, "HEAD", "source HEAD");
  const comparisonBase = resolveRef(git, baseRef, "comparison base");
  if (!git.isAncestor(comparisonBase, sourceHead)) {
    throw new Error(`comparison base ${comparisonBase} is not an ancestor of source HEAD`);
  }
  if (git.status().trim()) throw new Error("source tree is dirty; coverage requires a clean checkout");
  return { sourceHead, comparisonBase };
}

function assertStableSource(git, baseRef, expected) {
  if (git.status().trim()) throw new Error("source tree dirty drift detected during coverage");
  const head = resolveRef(git, "HEAD", "source HEAD");
  if (head !== expected.sourceHead) throw new Error("source HEAD drift detected during coverage");
  const base = resolveRef(git, baseRef, "comparison base");
  if (base !== expected.comparisonBase) throw new Error("comparison base drift detected during coverage");
  if (!git.isAncestor(base, head)) throw new Error("comparison base ancestry drift detected during coverage");
}

function coverageJobs(env) {
  const raw = env.OXID_COVERAGE_JOBS ?? "2";
  if (!/^[1-8]$/u.test(raw)) throw new Error("OXID_COVERAGE_JOBS must be an integer from 1 through 8");
  return raw;
}

async function writePrivateFile(filePath, contents) {
  await writeFile(filePath, contents, { flag: "wx", mode: 0o600 });
  await chmod(filePath, 0o600);
}

async function makePrivateDirectory(directory) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  await chmod(directory, 0o700);
}

async function syntheticDryRunScope({ rawReportPath, repoRoot }) {
  const raw = {
    type: "llvm.coverage.json.export",
    version: "dry-run",
    data: [{
      files: [{
        filename: path.join(repoRoot, "crates/foundation/src/lib.rs"),
        summary: { lines: { count: 100, covered: 80, percent: 80 } },
      }],
      totals: { lines: { count: 100, covered: 80, percent: 80 } },
    }],
  };
  await writeFile(rawReportPath, privateJson(raw), { mode: 0o600 });
}

async function spawnCoverageScope({ argv, cwd, env, scope }) {
  const exitCode = await runManagedChild(argv[0], argv.slice(1), {
    cwd,
    env,
    label: `coverage scope ${scope.id}`,
  });
  if (exitCode !== 0) throw new Error(`coverage scope ${scope.id} failed with exit code ${exitCode}`);
}

export async function runCoverage({
  repoRoot: requestedRepoRoot,
  stateRoot: requestedStateRoot,
  base,
  policy: suppliedPolicy,
  policyPath = DEFAULT_POLICY_PATH,
  dryRun = false,
  git: suppliedGit,
  executeScope: suppliedExecutor,
  env = process.env,
  now = () => new Date(),
} = {}) {
  if (typeof base !== "string" || base.length === 0) throw new Error("comparison base is required");
  const repoRoot = await realpath(requestedRepoRoot ?? path.resolve(path.dirname(SCRIPT_PATH), "../.."));
  const stateRoot = path.resolve(requestedStateRoot ?? path.join(repoRoot, "target"));
  const git = suppliedGit ?? defaultGit(repoRoot);
  const policy = suppliedPolicy ?? JSON.parse(await readFile(policyPath, "utf8"));
  const workspacePackages = await discoverWorkspacePackages(repoRoot);
  validatePolicy(policy, workspacePackages);
  const jobs = coverageJobs(env);
  const source = initialSourceState(git, base);
  const lockPath = path.join(stateRoot, ".oxid-coverage.lock");
  const releaseLock = await acquireCoverageLock(lockPath, {
    pid: process.pid,
    sourceHead: source.sourceHead,
    comparisonBase: source.comparisonBase,
  });

  const coverageRoot = path.join(stateRoot, "coverage");
  const headRoot = path.join(coverageRoot, source.sourceHead);
  const buildRoot = path.join(headRoot, "build");
  const temporaryRoot = path.join(headRoot, "tmp");
  const reportRoot = path.join(headRoot, "reports");
  let ownsHeadRoot = false;
  let completed = false;
  try {
    assertStableSource(git, base, source);
    await makePrivateDirectory(coverageRoot);
    try {
      await mkdir(headRoot, { mode: 0o700 });
      ownsHeadRoot = true;
    } catch (error) {
      if (error?.code === "EEXIST") {
        throw new Error("coverage output already exists for this source HEAD; refusing to mix evidence");
      }
      throw error;
    }
    await Promise.all([buildRoot, temporaryRoot, reportRoot].map(makePrivateDirectory));

    const summaries = [];
    const commands = [];
    const executor = suppliedExecutor ?? (dryRun ? syntheticDryRunScope : spawnCoverageScope);
    const executionMode = suppliedExecutor ? "test" : dryRun ? "dry-run" : "coverage";
    for (const scope of policy.scopes) {
      const rawReportPath = path.join(temporaryRoot, `${scope.id}.llvm.json`);
      const relativeRawPath = normalizedRelativePath(rawReportPath, repoRoot);
      const scopeBuildRoot = path.join(buildRoot, scope.id);
      await makePrivateDirectory(scopeBuildRoot);
      const argv = [...scope.command, "--output-path", relativeRawPath];
      const commandEnvironment = {
        ...env,
        CARGO_BUILD_JOBS: jobs,
        CARGO_TARGET_DIR: scopeBuildRoot,
      };
      await executor({
        argv,
        env: commandEnvironment,
        rawReportPath,
        repoRoot,
        scope,
      });
      const raw = JSON.parse(await readFile(rawReportPath, "utf8"));
      const summary = normalizeLlvmReport(raw, { repoRoot, scopeId: scope.id });
      const summaryName = `${scope.id}-summary.json`;
      const summaryContents = privateJson(summary);
      await writePrivateFile(path.join(reportRoot, summaryName), summaryContents);
      summaries.push({ id: scope.id, name: summaryName, contents: summaryContents, totals: summary.totals });
      commands.push({
        id: scope.id,
        argv,
        buildRoot: normalizedRelativePath(scopeBuildRoot, repoRoot),
        report: normalizedRelativePath(path.join(reportRoot, summaryName), repoRoot),
        ...scopeMetadata(scope),
      });
      await rm(rawReportPath, { force: true });
      await rm(scopeBuildRoot, { recursive: true, force: true });
      assertStableSource(git, base, source);
    }
    await rm(temporaryRoot, { recursive: true, force: true });
    await rm(buildRoot, { recursive: true, force: true });
    assertStableSource(git, base, source);

    const policyContents = privateJson(policy);
    const manifest = {
      schemaVersion: 1,
      mode: executionMode,
      sourceHead: source.sourceHead,
      comparisonBase: source.comparisonBase,
      generatedAt: now().toISOString(),
      jobs: Number(jobs),
      workspaceFloorPercent: policy.workspaceFloorPercent,
      policySha256: sha256(policyContents),
      packageInventory: policy.classifications,
      commands,
      summaries: summaries.map((summary) => ({
        id: summary.id,
        report: normalizedRelativePath(path.join(reportRoot, summary.name), repoRoot),
        sha256: sha256(summary.contents),
        totals: summary.totals,
      })),
    };
    const manifestContents = privateJson(manifest);
    await writePrivateFile(path.join(reportRoot, "manifest.json"), manifestContents);
    const checksums = {
      schemaVersion: 1,
      algorithm: "sha256",
      files: Object.fromEntries([
        ["manifest.json", sha256(manifestContents)],
        ...summaries.map((summary) => [summary.name, sha256(summary.contents)]),
      ].sort(([left], [right]) => left.localeCompare(right))),
    };
    await writePrivateFile(path.join(reportRoot, "checksums.json"), privateJson(checksums));
    completed = true;
    return { sourceHead: source.sourceHead, comparisonBase: source.comparisonBase, headRoot, reportRoot, manifest };
  } finally {
    if (!completed && ownsHeadRoot) await rm(headRoot, { recursive: true, force: true });
    await releaseLock();
  }
}

function usage() {
  return "Usage: node scripts/coverage/run.mjs --base <commit-ish> [--policy <path>] [--dry-run]\n";
}

async function main() {
  if (process.argv.slice(2).includes("--help")) {
    process.stdout.write(usage());
    return;
  }
  const options = parseArguments(process.argv.slice(2));
  const result = await runCoverage({
    base: options.base,
    dryRun: options.dryRun,
    policyPath: options.policyPath ?? DEFAULT_POLICY_PATH,
  });
  const relative = normalizedRelativePath(result.reportRoot, path.resolve(path.dirname(SCRIPT_PATH), "../.."));
  process.stdout.write(`[coverage] source ${result.sourceHead}; reports ${relative}\n`);
}

if (path.resolve(process.argv[1] ?? "") === SCRIPT_PATH) {
  main().catch((error) => {
    process.stderr.write(`[coverage] ${error.message}\n`);
    process.exitCode = 1;
  });
}
