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
  "core", "critical", "workspaceOnly", "additionalScopes", "plainTestExclusions",
]);
const POLICY_KEYS = Object.freeze([
  "schemaVersion", "workspaceFloorPercent", "packageFloorsPercent", "scopes", "baselines",
  "pathRules", "changedLines", "comparisonBase", "classifications", "exceptions",
]);
const EXCEPTION_KEYS = Object.freeze([
  "id", "kind", "packages", "owner", "issue", "rationale", "compensatingTest", "approval",
  "expiry", "removalCondition",
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

function slashPath(candidate) {
  return candidate.split(path.sep).join("/");
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
  return slashPath(relative);
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
  return slashPath(relative);
}

function scopeMetadata(scope) {
  if (scope.id === "workspace-aggregate") return { packages: ["workspace"], features: [], profile: "test" };
  if (scope.id === "headless-host") return { packages: ["oxid-headless"], features: [], profile: "all-targets" };
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

export async function discoverWorkspacePackageInventory(repoRoot) {
  const rootManifest = await readFile(path.join(repoRoot, "Cargo.toml"), "utf8");
  const members = parseWorkspaceMembers(rootManifest);
  const inventory = [];
  for (const member of members) {
    if (path.isAbsolute(member) || member.split("/").includes("..")) {
      throw new Error(`workspace member path escapes repository: ${member}`);
    }
    const root = slashPath(path.normalize(member));
    const manifestPath = path.join(repoRoot, root, "Cargo.toml");
    inventory.push({ name: parsePackageName(await readFile(manifestPath, "utf8"), manifestPath), root });
  }
  if (new Set(inventory.map(({ name }) => name)).size !== inventory.length) {
    throw new Error("workspace contains duplicate package names");
  }
  if (new Set(inventory.map(({ root }) => root)).size !== inventory.length) {
    throw new Error("workspace contains duplicate package roots");
  }
  return inventory.sort((left, right) => left.name.localeCompare(right.name));
}

export async function discoverWorkspacePackages(repoRoot) {
  return (await discoverWorkspacePackageInventory(repoRoot)).map(({ name }) => name);
}

function normalizedInventory(workspacePackages) {
  if (!Array.isArray(workspacePackages)) throw new Error("workspace package inventory must be an array");
  return workspacePackages.map((entry) => (
    typeof entry === "string" ? { name: entry, root: null } : entry
  ));
}

function expectedPackagesByScope(policy) {
  return new Map([
    ["workspace-aggregate", [
      ...policy.classifications.core,
      ...policy.classifications.critical,
      ...policy.classifications.workspaceOnly,
    ]],
    ...policy.classifications.additionalScopes.map(({ package: packageName, scope }) => [scope, [packageName]]),
  ]);
}

export function validatePolicy(policy, workspacePackages, { now = new Date() } = {}) {
  assertClosedObject(policy, POLICY_KEYS, "policy");
  if (policy.schemaVersion !== 2) throw new Error("unsupported coverage policy schemaVersion");
  if (policy.workspaceFloorPercent !== 80) throw new Error("workspace coverage floor must remain 80 percent");
  assertClosedObject(policy.packageFloorsPercent, ["core", "critical"], "packageFloorsPercent");
  if (policy.packageFloorsPercent.core !== 85 || policy.packageFloorsPercent.critical !== 90) {
    throw new Error("package coverage floors must preserve the reviewed 85/90 contract");
  }

  if (!Array.isArray(policy.scopes) || policy.scopes.length !== EXPECTED_SCOPE_IDS.length) {
    throw new Error("policy scopes must contain exactly the three reviewed coverage scopes");
  }
  policy.scopes.forEach((scope, index) => {
    assertClosedObject(scope, ["id", "command"], `scope ${index}`);
    if (scope.id !== EXPECTED_SCOPE_IDS[index]) throw new Error("coverage scopes are missing, unknown, or out of serial order");
    assertStringArray(scope.command, `scope ${scope.id} command`);
    if (JSON.stringify(scope.command) !== JSON.stringify(EXPECTED_COMMANDS[index])) {
      throw new Error(`scope ${scope.id} command differs from the reviewed coverage contract`);
    }
  });

  assertClosedObject(policy.baselines, ["mode", "scopes"], "baselines");
  if (policy.baselines.mode !== "placeholder" || !Array.isArray(policy.baselines.scopes)
      || policy.baselines.scopes.length !== EXPECTED_SCOPE_IDS.length) {
    throw new Error("scope baselines must remain explicit placeholders until reviewed");
  }
  policy.baselines.scopes.forEach((baseline, index) => {
    assertClosedObject(baseline, ["scope", "count", "covered"], `baseline ${index}`);
    if (baseline.scope !== EXPECTED_SCOPE_IDS[index] || baseline.count !== null || baseline.covered !== null) {
      throw new Error("scope baseline placeholder differs from the reviewed contract");
    }
  });

  assertClosedObject(
    policy.pathRules,
    ["production", "excludedDirectories", "generated", "siblingTestSuffix"],
    "pathRules",
  );
  if (JSON.stringify(policy.pathRules.production) !== JSON.stringify([
    "apps/*/src/**/*.rs", "crates/*/src/**/*.rs",
  ]) || JSON.stringify(policy.pathRules.excludedDirectories) !== JSON.stringify(["tests", "examples", "benches"])
      || JSON.stringify(policy.pathRules.generated) !== JSON.stringify(["**/generated/**"])
      || policy.pathRules.siblingTestSuffix !== "_tests.rs") {
    throw new Error("production, test, generated, or sibling path rules differ from the reviewed contract");
  }
  assertClosedObject(policy.changedLines, ["floorPercent", "diffArguments", "range", "zeroDenominator"], "changedLines");
  if (policy.changedLines.floorPercent !== 90
      || JSON.stringify(policy.changedLines.diffArguments) !== JSON.stringify(["--unified=0", "--diff-filter=AMCR"])
      || policy.changedLines.range !== "BASE...HEAD"
      || policy.changedLines.zeroDenominator !== "not-applicable") {
    throw new Error("changed-line policy differs from the reviewed 90 percent contract");
  }
  assertClosedObject(policy.comparisonBase, ["resolveToCommit", "requireAncestor", "range"], "comparisonBase");
  if (policy.comparisonBase.resolveToCommit !== true || policy.comparisonBase.requireAncestor !== true
      || policy.comparisonBase.range !== "BASE...HEAD") {
    throw new Error("comparison-base semantics differ from the reviewed contract");
  }

  assertClosedObject(policy.classifications, CLASSIFICATION_KEYS, "classifications");
  for (const key of ["core", "critical", "workspaceOnly"]) {
    assertStringArray(policy.classifications[key], `classifications.${key}`);
  }
  if (!Array.isArray(policy.classifications.additionalScopes)) throw new Error("classifications.additionalScopes must be an array");
  for (const [index, entry] of policy.classifications.additionalScopes.entries()) {
    assertClosedObject(entry, ["package", "scope"], `additionalScopes ${index}`);
    if (typeof entry.package !== "string" || !EXPECTED_SCOPE_IDS.slice(1).includes(entry.scope)) {
      throw new Error("malformed additional scope classification");
    }
  }
  if (!Array.isArray(policy.classifications.plainTestExclusions)) throw new Error("classifications.plainTestExclusions must be an array");
  for (const [index, entry] of policy.classifications.plainTestExclusions.entries()) {
    assertClosedObject(entry, ["package", "owner", "reason", "command", "removalCondition"], `plainTestExclusions ${index}`);
    for (const key of ["package", "owner", "reason", "removalCondition"]) {
      if (typeof entry[key] !== "string" || entry[key].length === 0) throw new Error(`malformed plain test exclusion ${key}`);
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
  const inventory = normalizedInventory(workspacePackages);
  const workspace = new Set(inventory.map(({ name }) => name));
  for (const packageName of seen) {
    if (!workspace.has(packageName)) throw new Error(`unknown package classification '${packageName}'`);
  }
  for (const packageName of workspace) {
    if (!seen.has(packageName)) throw new Error(`missing classification for package '${packageName}'`);
  }

  if (!Array.isArray(policy.exceptions)) throw new Error("exceptions must be an array");
  const exceptionIds = new Set();
  for (const [index, exception] of policy.exceptions.entries()) {
    assertClosedObject(exception, EXCEPTION_KEYS, `exception ${index}`);
    for (const key of ["id", "kind", "owner", "issue", "rationale", "compensatingTest", "approval", "expiry", "removalCondition"]) {
      if (typeof exception[key] !== "string" || exception[key].trim().length === 0) throw new Error(`malformed exception ${key}`);
    }
    if (exceptionIds.has(exception.id)) throw new Error(`duplicate exception id '${exception.id}'`);
    exceptionIds.add(exception.id);
    if (!["changed-lines", "regression"].includes(exception.kind)) {
      throw new Error("exception kind cannot bypass an absolute package floor");
    }
    assertStringArray(exception.packages, `exception ${exception.id} packages`);
    if (new Set(exception.packages).size !== exception.packages.length
        || exception.packages.some((name) => !workspace.has(name))) {
      throw new Error(`exception ${exception.id} has duplicate or unknown packages`);
    }
    if (!/^\d{4}-\d{2}-\d{2}$/u.test(exception.expiry)) throw new Error(`exception ${exception.id} has malformed expiry`);
    const expiry = new Date(`${exception.expiry}T23:59:59.999Z`);
    if (Number.isNaN(expiry.valueOf()) || now.valueOf() > expiry.valueOf()) {
      throw new Error(`exception ${exception.id} expired on ${exception.expiry}`);
    }
  }
  return policy;
}

function packageForPath(sourcePath, packageInventory) {
  const matches = normalizedInventory(packageInventory).filter(({ root }) => (
    typeof root === "string" && (sourcePath === root || sourcePath.startsWith(`${root}/`))
  ));
  if (matches.length > 1) throw new Error(`ambiguous package mapping for '${sourcePath}'`);
  return matches[0] ?? null;
}

function pathRule(sourcePath, packageInventory) {
  const packageEntry = packageForPath(sourcePath, packageInventory);
  if (!packageEntry) return { packageEntry: null, production: false, reason: "outside-production" };
  const relative = sourcePath.slice(packageEntry.root.length + 1);
  const parts = relative.split("/");
  if (parts.some((part) => ["tests", "examples", "benches"].includes(part))) {
    return { packageEntry, production: false, reason: "test-directory" };
  }
  if (parts.includes("generated")) return { packageEntry, production: false, reason: "generated" };
  if (sourcePath.endsWith("_tests.rs")) return { packageEntry, production: false, reason: "sibling-test" };
  const production = relative.startsWith("src/") && sourcePath.endsWith(".rs")
    && (packageEntry.root.startsWith("apps/") || packageEntry.root.startsWith("crates/"));
  return { packageEntry, production, reason: production ? null : "outside-production" };
}

function lineMapFromSegments(segments, label) {
  if (!Array.isArray(segments)) return null;
  const lineCounts = new Map();
  let previousCoordinate = [0, 0];
  for (let index = 0; index < segments.length; index += 1) {
    const segment = segments[index];
    if (!Array.isArray(segment) || segment.length < 4) throw new Error(`malformed LLVM coverage output: invalid segments for ${label}`);
    const [line, column, count, hasCount] = segment;
    if (!Number.isInteger(line) || line < 1 || !Number.isInteger(column) || column < 1
        || !Number.isInteger(count) || count < 0 || typeof hasCount !== "boolean"
        || line < previousCoordinate[0] || (line === previousCoordinate[0] && column < previousCoordinate[1])) {
      throw new Error(`malformed LLVM coverage output: invalid segments for ${label}`);
    }
    previousCoordinate = [line, column];
    if (!hasCount) continue;
    const next = segments[index + 1];
    let endLine = line;
    if (next) {
      if (!Array.isArray(next) || !Number.isInteger(next[0]) || !Number.isInteger(next[1])) {
        throw new Error(`malformed LLVM coverage output: invalid segments for ${label}`);
      }
      endLine = next[0] - (next[1] === 1 ? 1 : 0);
    }
    for (let candidate = line; candidate <= Math.max(line, endLine); candidate += 1) {
      lineCounts.set(candidate, Math.max(lineCounts.get(candidate) ?? 0, count));
    }
  }
  return lineCounts;
}

function addRegions(lineCounts, regions, label) {
  if (!Array.isArray(regions)) return false;
  for (const region of regions) {
    if (!Array.isArray(region) || region.length < 6) throw new Error(`malformed LLVM coverage output: invalid regions for ${label}`);
    const [startLine, startColumn, endLine, endColumn, count] = region;
    if (![startLine, startColumn, endLine, endColumn, count].every(Number.isInteger)
        || startLine < 1 || startColumn < 1 || endLine < startLine || endColumn < 1 || count < 0) {
      throw new Error(`malformed LLVM coverage output: invalid regions for ${label}`);
    }
    const inclusiveEnd = endLine - (endColumn === 1 ? 1 : 0);
    for (let line = startLine; line <= Math.max(startLine, inclusiveEnd); line += 1) {
      lineCounts.set(line, Math.max(lineCounts.get(line) ?? 0, count));
    }
  }
  return true;
}

function functionRegionsByPath(data, repoRoot) {
  const result = new Map();
  if (!Array.isArray(data.functions)) return result;
  for (const fn of data.functions) {
    if (!Array.isArray(fn?.filenames) || !Array.isArray(fn?.regions)) {
      throw new Error("malformed LLVM coverage output: invalid function regions");
    }
    for (const region of fn.regions) {
      const fileIndex = region?.[5];
      if (!Number.isInteger(fileIndex) || fileIndex < 0 || fileIndex >= fn.filenames.length) {
        throw new Error("malformed LLVM coverage output: invalid function region file mapping");
      }
      const sourcePath = normalizeRepoPath(fn.filenames[fileIndex], repoRoot);
      if (!result.has(sourcePath)) result.set(sourcePath, []);
      result.get(sourcePath).push(region);
    }
  }
  return result;
}

export function normalizeLlvmReport(raw, {
  repoRoot,
  scopeId,
  packageInventory = [],
  workspaceFloorPercent = 80,
  pathRules: _pathRules,
} = {}) {
  if (!raw || typeof raw !== "object" || !Array.isArray(raw.data) || raw.data.length !== 1) {
    throw new Error("malformed LLVM coverage output: expected one data record");
  }
  const data = raw.data[0];
  if (!data || typeof data !== "object" || !Array.isArray(data.files)) {
    throw new Error("malformed LLVM coverage output: missing files");
  }
  const totals = normalizeLines(data.totals?.lines, "total line counts");
  const functionRegions = functionRegionsByPath(data, repoRoot);
  const files = data.files.map((file) => {
    const sourcePath = normalizeRepoPath(file?.filename, repoRoot);
    const packageEntry = packageForPath(sourcePath, packageInventory);
    if (packageInventory.length > 0 && !packageEntry) {
      throw new Error(`unmapped package for LLVM coverage source '${sourcePath}'`);
    }
    let lineCounts = lineMapFromSegments(file?.segments, sourcePath);
    let lineEvidence = "llvm-segments";
    if (lineCounts === null) {
      lineCounts = new Map();
      lineEvidence = addRegions(lineCounts, functionRegions.get(sourcePath), sourcePath)
        ? "llvm-regions"
        : "unavailable";
    }
    return {
      path: sourcePath,
      ...(packageEntry ? { package: packageEntry.name } : {}),
      lines: normalizeLines(file?.summary?.lines, "file line counts"),
      lineEvidence,
      executableLines: [...lineCounts.entries()]
        .map(([line, count]) => ({ line, covered: count > 0 }))
        .sort((left, right) => left.line - right.line),
    };
  }).sort((left, right) => left.path.localeCompare(right.path));
  if (new Set(files.map((file) => file.path)).size !== files.length) {
    throw new Error("malformed LLVM coverage output: duplicate normalized source path");
  }
  if (scopeId === "workspace-aggregate" && workspaceFloorPercent !== null
      && totals.covered * 100 < totals.count * workspaceFloorPercent) {
    throw new Error(`workspace line coverage is below the ${workspaceFloorPercent} percent floor`);
  }
  const packages = [];
  for (const packageName of [...new Set(files.map((file) => file.package).filter(Boolean))].sort()) {
    const packageFiles = files.filter((file) => file.package === packageName);
    packages.push({
      name: packageName,
      lines: packageFiles.reduce((sum, file) => ({
        count: sum.count + file.lines.count,
        covered: sum.covered + file.lines.covered,
      }), { count: 0, covered: 0 }),
    });
  }
  return { schemaVersion: 2, scope: scopeId, totals: { lines: totals }, packages, files };
}

function verifyNormalizedReports(policy, reports, packageInventory) {
  if (!Array.isArray(reports)) throw new Error("normalized coverage reports must be an array");
  const reportByScope = new Map();
  const sourcePaths = new Set();
  const expectedByScope = expectedPackagesByScope(policy);
  for (const report of reports) {
    if (!report || !EXPECTED_SCOPE_IDS.includes(report.scope) || !Array.isArray(report.files)) {
      throw new Error("malformed normalized coverage report");
    }
    if (reportByScope.has(report.scope)) throw new Error(`duplicate normalized report for scope '${report.scope}'`);
    reportByScope.set(report.scope, report);
    const expected = new Set(expectedByScope.get(report.scope) ?? []);
    const evidenced = new Set();
    for (const file of report.files) {
      if (typeof file.path !== "string" || !file.lines || !Array.isArray(file.executableLines)) {
        throw new Error("malformed normalized file evidence");
      }
      if (sourcePaths.has(file.path)) throw new Error(`duplicate normalized source path '${file.path}' across coverage reports`);
      sourcePaths.add(file.path);
      const mapped = packageForPath(file.path, packageInventory);
      if (!mapped) throw new Error(`unmapped package for normalized source '${file.path}'`);
      if (file.package && file.package !== mapped.name) throw new Error(`ambiguous package evidence for '${file.path}'`);
      if (!expected.has(mapped.name)) throw new Error(`unexpected package evidence '${mapped.name}' in scope '${report.scope}'`);
      const rule = pathRule(file.path, packageInventory);
      if (rule.production) evidenced.add(mapped.name);
    }
    for (const packageName of expected) {
      if (!evidenced.has(packageName)) throw new Error(`missing package evidence for '${packageName}' in scope '${report.scope}'`);
    }
  }
  for (const scope of EXPECTED_SCOPE_IDS) {
    if (!reportByScope.has(scope)) throw new Error(`missing normalized report for scope '${scope}'`);
  }
  return reportByScope;
}

function sumLines(files) {
  return files.reduce((sum, file) => ({
    count: sum.count + file.lines.count,
    covered: sum.covered + file.lines.covered,
  }), { count: 0, covered: 0 });
}

export function evaluateCoverage(policy, reports, {
  packageInventory,
  changedLines = null,
} = {}) {
  const reportByScope = verifyNormalizedReports(policy, reports, packageInventory);
  const allFiles = reports.flatMap((report) => report.files)
    .filter((file) => pathRule(file.path, packageInventory).production);
  const packageClasses = new Map([
    ...policy.classifications.core.map((name) => [name, "core"]),
    ...policy.classifications.critical.map((name) => [name, "critical"]),
  ]);
  const packages = [...packageClasses.entries()].map(([name, classification]) => {
    const lines = sumLines(allFiles.filter((file) => packageForPath(file.path, packageInventory)?.name === name));
    const floorPercent = policy.packageFloorsPercent[classification];
    const requiredCovered = Math.ceil((lines.count * floorPercent) / 100);
    return {
      name,
      classification,
      floorPercent,
      lines,
      requiredCovered,
      deficit: Math.max(0, requiredCovered - lines.covered),
      status: lines.covered * 100 >= lines.count * floorPercent ? "pass" : "fail",
      exceptions: [],
    };
  }).sort((left, right) => left.name.localeCompare(right.name));
  const workspace = reportByScope.get("workspace-aggregate").totals;
  const workspaceStatus = workspace.lines.covered * 100 >= workspace.lines.count * policy.workspaceFloorPercent
    ? "pass" : "fail";
  const effectiveChangedStatus = changedLines?.status ?? "not-evaluated";
  const status = workspaceStatus === "fail" || packages.some((entry) => entry.status === "fail")
      || effectiveChangedStatus === "fail"
    ? "fail" : "pass";
  return {
    schemaVersion: 1,
    status,
    aggregate: { floorPercent: policy.workspaceFloorPercent, status: workspaceStatus, ...workspace },
    baselines: policy.baselines.scopes.map((baseline) => ({
      ...baseline,
      status: "not-applicable",
      reason: "placeholder",
    })),
    packages,
    changedLines: changedLines ?? { status: "not-evaluated" },
  };
}

function parseDiffPath(line, prefix, label) {
  if (!line.startsWith(prefix)) throw new Error(`malformed git diff: missing ${label} path`);
  const value = line.slice(prefix.length);
  if (!value || value.startsWith('"') || value.includes("\t") || value.includes("\0")) {
    throw new Error(`malformed git diff: ambiguous ${label} path`);
  }
  return value;
}

function parseChangedDiff(diff) {
  if (typeof diff !== "string" || diff.includes("\0")) throw new Error("malformed git diff output");
  if (diff === "") return [];
  const lines = diff.split("\n");
  const files = [];
  let current = null;
  let hunk = null;
  const finishHunk = () => {
    if (hunk && hunk.observed !== hunk.expected) throw new Error("malformed git diff: HEAD hunk line count mismatch");
    hunk = null;
  };
  for (const line of lines) {
    if (line.startsWith("diff --git ")) {
      finishHunk();
      const match = line.match(/^diff --git a\/(\S+) b\/(\S+)$/u);
      if (!match) throw new Error("malformed git diff: ambiguous diff paths");
      current = { oldPath: match[1], path: match[2], change: "modified", addedLines: new Set() };
      files.push(current);
      continue;
    }
    if (!current) {
      if (line === "") continue;
      throw new Error("malformed git diff: content before file header");
    }
    if (line.startsWith("new file mode ")) current.change = "added";
    if (line.startsWith("rename from ")) {
      current.oldPath = parseDiffPath(line, "rename from ", "rename source");
      current.change = "renamed";
    }
    if (line.startsWith("rename to ")) {
      current.path = parseDiffPath(line, "rename to ", "rename destination");
      current.change = "renamed";
    }
    if (line.startsWith("@@ ")) {
      finishHunk();
      const match = line.match(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/u);
      if (!match) throw new Error("malformed git diff: invalid zero-context hunk");
      hunk = { nextLine: Number(match[1]), expected: match[2] === undefined ? 1 : Number(match[2]), observed: 0 };
      continue;
    }
    if (!hunk) continue;
    if (line.startsWith("+") && !line.startsWith("+++")) {
      current.addedLines.add(hunk.nextLine);
      hunk.nextLine += 1;
      hunk.observed += 1;
    } else if (line.startsWith(" ")) {
      hunk.nextLine += 1;
      hunk.observed += 1;
    } else if (line.startsWith("-") || line.startsWith("\\ No newline")) {
      // Removed BASE-side lines and markers do not advance the HEAD line number.
    } else if (line === "" && hunk.observed < hunk.expected) {
      throw new Error("malformed git diff: unexpected empty hunk line");
    }
  }
  finishHunk();
  return files;
}

function activeChangedLineException(policy, packages, now) {
  return policy.exceptions.find((exception) => exception.kind === "changed-lines"
    && now.valueOf() <= new Date(`${exception.expiry}T23:59:59.999Z`).valueOf()
    && packages.every((name) => exception.packages.includes(name)));
}

export function scoreChangedLines(diff, reports, {
  policy,
  packageInventory,
  now = new Date(),
} = {}) {
  const changedFiles = parseChangedDiff(diff);
  const coverageByPath = new Map();
  for (const report of reports) {
    for (const file of report.files ?? []) {
      if (coverageByPath.has(file.path)) throw new Error(`ambiguous coverage mapping for changed source '${file.path}'`);
      coverageByPath.set(file.path, file);
    }
  }
  const files = [];
  for (const changed of changedFiles) {
    const rule = pathRule(changed.path, packageInventory);
    if (!rule.packageEntry && /^(apps|crates)\/.*\/src\/.*\.rs$/u.test(changed.path)) {
      throw new Error(`unmapped package mapping for changed production source '${changed.path}'`);
    }
    if (!rule.production) {
      files.push({ path: changed.path, change: changed.change, status: "excluded", reason: rule.reason });
      continue;
    }
    const addedLines = [...changed.addedLines].sort((left, right) => left - right);
    if (addedLines.length === 0) {
      files.push({
        path: changed.path,
        package: rule.packageEntry.name,
        change: changed.change,
        status: "not-applicable",
        lines: { count: 0, covered: 0, requiredCovered: null },
      });
      continue;
    }
    const evidence = coverageByPath.get(changed.path);
    if (!evidence) throw new Error(`missing coverage mapping for changed production source '${changed.path}'`);
    if (!Array.isArray(evidence.executableLines)) {
      throw new Error(`malformed coverage mapping for changed production source '${changed.path}'`);
    }
    const executable = new Map();
    for (const entry of evidence.executableLines) {
      if (!Number.isInteger(entry?.line) || entry.line < 1 || typeof entry.covered !== "boolean"
          || executable.has(entry.line)) {
        throw new Error(`ambiguous executable-line mapping for changed source '${changed.path}'`);
      }
      executable.set(entry.line, entry.covered);
    }
    const intersected = addedLines.filter((line) => executable.has(line));
    const covered = intersected.filter((line) => executable.get(line)).length;
    const count = intersected.length;
    const requiredCovered = count === 0 ? null : Math.ceil((count * policy.changedLines.floorPercent) / 100);
    files.push({
      path: changed.path,
      package: rule.packageEntry.name,
      change: changed.change,
      status: count === 0 ? "not-applicable" : covered * 100 >= count * policy.changedLines.floorPercent ? "pass" : "fail",
      lines: { count, covered, requiredCovered },
    });
  }
  files.sort((left, right) => left.path.localeCompare(right.path));
  const included = files.filter((file) => file.lines);
  const count = included.reduce((sum, file) => sum + file.lines.count, 0);
  const covered = included.reduce((sum, file) => sum + file.lines.covered, 0);
  const requiredCovered = count === 0 ? null : Math.ceil((count * policy.changedLines.floorPercent) / 100);
  let status = count === 0 ? "not-applicable"
    : covered * 100 >= count * policy.changedLines.floorPercent ? "pass" : "fail";
  let exception = null;
  if (status === "fail") {
    const packages = [...new Set(included.filter((file) => file.lines.count > 0).map((file) => file.package))].sort();
    exception = activeChangedLineException(policy, packages, now) ?? null;
    if (exception) status = "excepted";
  }
  return {
    schemaVersion: 1,
    floorPercent: policy.changedLines.floorPercent,
    status,
    percent: count === 0 ? null : (covered * 100) / count,
    lines: { count, covered, requiredCovered },
    ...(exception ? { exception: exception.id } : {}),
    files,
  };
}

export function parseArguments(argv) {
  const parsed = { base: undefined, dryRun: false, enforce: false, policyPath: undefined };
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run" || argument === "--enforce") {
      if (seen.has(argument)) throw new Error(`${argument} specified more than once`);
      seen.add(argument);
      parsed[argument === "--dry-run" ? "dryRun" : "enforce"] = true;
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
        cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
      }).trim();
    },
    status() {
      return execFileSync("git", ["status", "--porcelain=v1", "--untracked-files=all"], {
        cwd: repoRoot, encoding: "utf8",
      });
    },
    isAncestor(base, head) {
      return spawnSync("git", ["merge-base", "--is-ancestor", base, head], { cwd: repoRoot }).status === 0;
    },
    diff(base, head) {
      return execFileSync("git", ["diff", "--unified=0", "--diff-filter=AMCR", `${base}...${head}`], {
        cwd: repoRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024,
      });
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
  if (!git.isAncestor(comparisonBase, sourceHead)) throw new Error(`comparison base ${comparisonBase} is not an ancestor of source HEAD`);
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

async function syntheticDryRunScope({ rawReportPath, repoRoot, scope, policy, packageInventory }) {
  const expected = expectedPackagesByScope(policy).get(scope.id);
  const byName = new Map(packageInventory.map((entry) => [entry.name, entry]));
  const files = expected.map((name) => ({
    filename: path.join(repoRoot, byName.get(name).root, "src/lib.rs"),
    segments: [[1, 1, 1, true, true, false], [101, 1, 0, false, false, false]],
    summary: { lines: { count: 100, covered: 100, percent: 100 } },
  }));
  const count = files.length * 100;
  const raw = {
    type: "llvm.coverage.json.export", version: "dry-run",
    data: [{ files, totals: { lines: { count, covered: count, percent: 100 } } }],
  };
  await writeFile(rawReportPath, privateJson(raw), { mode: 0o600 });
}

async function spawnCoverageScope({ argv, cwd, env, scope }) {
  const exitCode = await runManagedChild(argv[0], argv.slice(1), { cwd, env, label: `coverage scope ${scope.id}` });
  if (exitCode !== 0) throw new Error(`coverage scope ${scope.id} failed with exit code ${exitCode}`);
}

export async function runCoverage({
  repoRoot: requestedRepoRoot,
  stateRoot: requestedStateRoot,
  base,
  policy: suppliedPolicy,
  policyPath = DEFAULT_POLICY_PATH,
  dryRun = false,
  enforce = false,
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
  const packageInventory = await discoverWorkspacePackageInventory(repoRoot);
  const generatedAt = now();
  validatePolicy(policy, packageInventory, { now: generatedAt });
  const jobs = coverageJobs(env);
  const source = initialSourceState(git, base);
  const lockPath = path.join(stateRoot, ".oxid-coverage.lock");
  const releaseLock = await acquireCoverageLock(lockPath, {
    pid: process.pid, sourceHead: source.sourceHead, comparisonBase: source.comparisonBase,
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
      if (error?.code === "EEXIST") throw new Error("coverage output already exists for this source HEAD; refusing to mix evidence");
      throw error;
    }
    await Promise.all([buildRoot, temporaryRoot, reportRoot].map(makePrivateDirectory));

    const summaries = [];
    const normalizedReports = [];
    const commands = [];
    const executor = suppliedExecutor ?? (dryRun ? syntheticDryRunScope : spawnCoverageScope);
    const executionMode = suppliedExecutor ? "test" : dryRun ? "dry-run" : "coverage";
    for (const scope of policy.scopes) {
      const rawReportPath = path.join(temporaryRoot, `${scope.id}.llvm.json`);
      const relativeRawPath = normalizedRelativePath(rawReportPath, repoRoot);
      const scopeBuildRoot = path.join(buildRoot, scope.id);
      await makePrivateDirectory(scopeBuildRoot);
      const argv = [...scope.command, "--output-path", relativeRawPath];
      const commandEnvironment = { ...env, CARGO_BUILD_JOBS: jobs, CARGO_TARGET_DIR: scopeBuildRoot };
      await executor({
        argv, env: commandEnvironment, rawReportPath, repoRoot, scope, policy, packageInventory,
      });
      const raw = JSON.parse(await readFile(rawReportPath, "utf8"));
      const summary = normalizeLlvmReport(raw, {
        repoRoot,
        scopeId: scope.id,
        packageInventory,
        workspaceFloorPercent: policy.workspaceFloorPercent,
        pathRules: policy.pathRules,
      });
      const summaryName = `${scope.id}-summary.json`;
      const summaryContents = privateJson(summary);
      await writePrivateFile(path.join(reportRoot, summaryName), summaryContents);
      summaries.push({ id: scope.id, name: summaryName, contents: summaryContents, totals: summary.totals });
      normalizedReports.push(summary);
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
    if (typeof git.diff !== "function") throw new Error("git adapter does not provide deterministic changed-line diff evidence");
    const diff = git.diff(source.comparisonBase, source.sourceHead);
    assertStableSource(git, base, source);
    const changedLines = scoreChangedLines(diff, normalizedReports, {
      policy, packageInventory, now: generatedAt,
    });
    const evaluation = evaluateCoverage(policy, normalizedReports, { packageInventory, changedLines });
    const evaluationName = "evaluation.json";
    const changedLinesName = "changed-lines.json";
    const evaluationContents = privateJson(evaluation);
    const changedLinesContents = privateJson(changedLines);
    await writePrivateFile(path.join(reportRoot, evaluationName), evaluationContents);
    await writePrivateFile(path.join(reportRoot, changedLinesName), changedLinesContents);

    const policyContents = privateJson(policy);
    const manifest = {
      schemaVersion: 2,
      mode: executionMode,
      evaluationMode: enforce ? "enforce" : "measurement",
      sourceHead: source.sourceHead,
      comparisonBase: source.comparisonBase,
      comparisonRange: `${source.comparisonBase}...${source.sourceHead}`,
      generatedAt: generatedAt.toISOString(),
      jobs: Number(jobs),
      workspaceFloorPercent: policy.workspaceFloorPercent,
      packageFloorsPercent: policy.packageFloorsPercent,
      policySha256: sha256(policyContents),
      packageInventory: policy.classifications,
      commands,
      summaries: summaries.map((summary) => ({
        id: summary.id,
        report: normalizedRelativePath(path.join(reportRoot, summary.name), repoRoot),
        sha256: sha256(summary.contents),
        totals: summary.totals,
      })),
      evaluation: {
        status: evaluation.status,
        report: normalizedRelativePath(path.join(reportRoot, evaluationName), repoRoot),
        sha256: sha256(evaluationContents),
      },
      changedLines: {
        status: changedLines.status,
        report: normalizedRelativePath(path.join(reportRoot, changedLinesName), repoRoot),
        sha256: sha256(changedLinesContents),
        lines: changedLines.lines,
      },
    };
    const manifestContents = privateJson(manifest);
    await writePrivateFile(path.join(reportRoot, "manifest.json"), manifestContents);
    const evidenceFiles = [
      ["manifest.json", manifestContents],
      [evaluationName, evaluationContents],
      [changedLinesName, changedLinesContents],
      ...summaries.map((summary) => [summary.name, summary.contents]),
    ];
    const checksums = {
      schemaVersion: 1,
      algorithm: "sha256",
      files: Object.fromEntries(evidenceFiles
        .map(([name, contents]) => [name, sha256(contents)])
        .sort(([left], [right]) => left.localeCompare(right))),
    };
    await writePrivateFile(path.join(reportRoot, "checksums.json"), privateJson(checksums));
    completed = true;
    if (enforce && evaluation.status !== "pass") {
      throw new Error(`coverage policy enforcement failed; evidence retained at ${normalizedRelativePath(reportRoot, repoRoot)}`);
    }
    return {
      sourceHead: source.sourceHead,
      comparisonBase: source.comparisonBase,
      headRoot,
      reportRoot,
      manifest,
      evaluation,
      changedLines,
    };
  } finally {
    if (!completed && ownsHeadRoot) await rm(headRoot, { recursive: true, force: true });
    await releaseLock();
  }
}

function usage() {
  return "Usage: node scripts/coverage/run.mjs --base <commit-ish> [--policy <path>] [--dry-run] [--enforce]\n";
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
    enforce: options.enforce,
    policyPath: options.policyPath ?? DEFAULT_POLICY_PATH,
  });
  const relative = normalizedRelativePath(result.reportRoot, path.resolve(path.dirname(SCRIPT_PATH), "../.."));
  process.stdout.write(`[coverage] source ${result.sourceHead}; ${options.enforce ? "enforced" : "measured"}; reports ${relative}\n`);
}

if (path.resolve(process.argv[1] ?? "") === SCRIPT_PATH) {
  main().catch((error) => {
    process.stderr.write(`[coverage] ${error.message}\n`);
    process.exitCode = 1;
  });
}
