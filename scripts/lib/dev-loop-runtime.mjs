// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { cp, lstat, mkdir, readdir, readFile, realpath, rename, rm, stat, symlink, writeFile } from "node:fs/promises";
import path from "node:path";

const SETTINGS_PATH = path.join(".pi", "settings.json");
const PROJECT_AGENTS_PATH = path.join(".pi", "agents");
const PI_CLOSURE_SCHEMA_VERSION = 1;
const PI_CLOSURE_LOCK_WAIT_MS = 15_000;
const PI_CLOSURE_STALE_MS = 10 * 60_000;
const PI_CLOSURE_RETENTION_MS = 7 * 24 * 60 * 60_000;

export const PI_BUILTIN_CHILD_TOOLS = Object.freeze([
  "read", "grep", "find", "ls", "bash", "edit", "write",
]);

export const DEV_LOOP_SELECTED_TOOLS = Object.freeze([
  "read", "grep", "find", "ls", "bash", "subagent",
]);

export const REPOSITORY_CONFIGURED_TOOLS = Object.freeze([
  ...PI_BUILTIN_CHILD_TOOLS,
  "subagent",
  "labels_bootstrap", "pr_approve_dep_upgrade", "pr_expedite", "pr_request_review",
  "pr_stabilize", "pr_watch", "review_claim", "review_complete", "review_create",
  "review_enrich", "review_list",
]);

async function exists(candidate) {
  try {
    await stat(candidate);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT" || error?.code === "ENOTDIR") return false;
    throw error;
  }
}

async function findGitRoot(cwd) {
  const requested = await realpath(path.resolve(cwd));
  let reported;
  try {
    reported = execFileSync("git", ["-C", requested, "rev-parse", "--path-format=absolute", "--show-toplevel"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  } catch (error) {
    throw new Error(`not inside a registered Git checkout: ${cwd}: ${(error.stderr ?? error.message).toString().trim()}`, { cause: error });
  }
  const gitRoot = await realpath(reported);
  if (!isContained(gitRoot, requested)) throw new Error(`Git checkout root ${gitRoot} does not contain requested path ${requested}`);
  return gitRoot;
}

async function resolveCommonCheckoutRoot(gitRoot) {
  let porcelain;
  try {
    porcelain = execFileSync("git", ["-C", gitRoot, "worktree", "list", "--porcelain"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    throw new Error(`could not verify registered Git worktrees for ${gitRoot}: ${(error.stderr ?? error.message).toString().trim()}`, { cause: error });
  }
  const registered = porcelain.split(/\r?\n/)
    .filter((line) => line.startsWith("worktree "))
    .map((line) => line.slice("worktree ".length));
  if (registered.length === 0) throw new Error(`Git did not report a common checkout for ${gitRoot}`);
  const resolved = await Promise.all(registered.map(async (candidate) => {
    try {
      return await realpath(candidate);
    } catch {
      return null;
    }
  }));
  if (!resolved.includes(gitRoot)) throw new Error(`Git checkout is not registered in its common worktree list: ${gitRoot}`);
  const commonRoot = resolved[0];
  if (commonRoot === null) throw new Error(`registered common checkout is unavailable: ${registered[0]}`);
  return commonRoot;
}

async function lstatIfPresent(candidate) {
  try {
    return await lstat(candidate);
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function closureStateRoot(commonRoot) {
  return path.join(commonRoot, ".git", "oxid-factory", "pi-package-closures-v1");
}

function closurePaths(commonRoot, identity) {
  const root = closureStateRoot(commonRoot);
  return {
    root,
    closures: path.join(root, "closures"),
    staging: path.join(root, "staging"),
    locks: path.join(root, "locks"),
    closure: path.join(root, "closures", identity),
    lock: path.join(root, "locks", `${identity}.lock`),
  };
}

/** A closure key includes the complete ordered settings entries, not just versions. */
export function piPackageClosureIdentity(settings) {
  const pins = parseExactNpmPins(settings);
  const configuration = settings?.packages;
  if (!Array.isArray(configuration) || configuration.length === 0) {
    throw new Error(".pi/settings.json packages must be a non-empty array");
  }
  const canonical = canonicalJson(configuration);
  return {
    identity: createHash("sha256").update(`oxid-pi-closure-v${PI_CLOSURE_SCHEMA_VERSION}\n${canonical}\n`).digest("hex"),
    configuration,
    pins,
  };
}

async function readClosureMarker(closure) {
  try {
    return await readJson(path.join(closure, "closure.json"), "Pi package closure marker");
  } catch (error) {
    if (error?.cause?.code === "ENOENT" || error?.code === "ENOENT") return null;
    throw error;
  }
}

async function validClosure(closure, identity, pins) {
  const marker = await readClosureMarker(closure);
  if (marker?.schemaVersion !== PI_CLOSURE_SCHEMA_VERSION || marker.identity !== identity) return false;
  try {
    await resolveInstalledPinnedPackages({ candidates: [{ root: closure, source: "closure" }], pins });
    return true;
  } catch {
    return false;
  }
}

async function pause(milliseconds) {
  await new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function acquireClosureLock(lock, { waitMs = PI_CLOSURE_LOCK_WAIT_MS, staleMs = PI_CLOSURE_STALE_MS, now = () => Date.now() } = {}) {
  const deadline = now() + waitMs;
  for (;;) {
    try {
      await writeFile(lock, JSON.stringify({ pid: process.pid, createdAtMs: now() }), { flag: "wx", mode: 0o600 });
      return async () => { await rm(lock, { force: true }); };
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
      const info = await lstatIfPresent(lock);
      if (info && now() - info.mtimeMs > staleMs) {
        await rm(lock, { force: true });
        continue;
      }
      if (now() >= deadline) throw new Error(`timed out waiting ${waitMs}ms for Pi package closure ${path.basename(lock, ".lock")}`);
      await pause(Math.min(100, Math.max(1, deadline - now())));
    }
  }
}

async function replaceWithClosureLink(localStore, closure) {
  const temporary = `${localStore}.next-${process.pid}-${Math.random().toString(16).slice(2)}`;
  await symlink(closure, temporary, "dir");
  await rename(temporary, localStore);
}

async function copyLegacyStore(legacyStore, stageStore) {
  const legacyNodeModules = path.join(legacyStore, "node_modules");
  const info = await lstatIfPresent(legacyNodeModules);
  if (!info?.isDirectory() || info.isSymbolicLink()) return false;
  await mkdir(stageStore, { recursive: true, mode: 0o700 });
  await cp(legacyNodeModules, path.join(stageStore, "node_modules"), { recursive: true, verbatimSymlinks: true });
  return true;
}

/**
 * Publish an exact immutable package closure and point only this worktree at it.
 * `install` receives an empty staging store when no verified legacy store exists.
 */
export async function ensureSharedPiPackageStore({
  cwd = process.cwd(), install, waitMs = PI_CLOSURE_LOCK_WAIT_MS, staleMs = PI_CLOSURE_STALE_MS, now = () => Date.now(),
} = {}) {
  const gitRoot = await findGitRoot(cwd);
  const commonRoot = await resolveCommonCheckoutRoot(gitRoot);
  for (const piRoot of new Set([path.join(gitRoot, ".pi"), path.join(commonRoot, ".pi")])) {
    const info = await lstat(piRoot);
    if (!info.isDirectory() || info.isSymbolicLink()) throw new Error(`Pi project root must be a real directory: ${piRoot}`);
  }
  const settings = await readJson(path.join(gitRoot, SETTINGS_PATH), "project Pi settings");
  const { identity, configuration, pins } = piPackageClosureIdentity(settings);
  const paths = closurePaths(commonRoot, identity);
  await Promise.all([mkdir(paths.closures, { recursive: true, mode: 0o700 }), mkdir(paths.staging, { recursive: true, mode: 0o700 }), mkdir(paths.locks, { recursive: true, mode: 0o700 })]);

  let published = false;
  if (!(await validClosure(paths.closure, identity, pins))) {
    const release = await acquireClosureLock(paths.lock, { waitMs, staleMs, now });
    try {
      if (!(await validClosure(paths.closure, identity, pins))) {
        const stage = path.join(paths.staging, `${identity}.${process.pid}.${Math.random().toString(16).slice(2)}`);
        const stageStore = path.join(stage, ".pi", "npm");
        try {
          await mkdir(stage, { recursive: false, mode: 0o700 });
          const legacy = path.join(gitRoot, ".pi", "npm");
          const legacyInfo = await lstatIfPresent(legacy);
          let legacySource = legacyInfo?.isSymbolicLink() ? null : legacy;
          if (legacyInfo?.isSymbolicLink()) {
            try {
              const target = await realpath(legacy);
              const commonLegacy = await realpath(path.join(commonRoot, ".pi", "npm"));
              if (gitRoot !== commonRoot && target === commonLegacy) legacySource = target;
            } catch { /* A broken or foreign link is never migration input. */ }
          }
          const migrated = legacySource !== null && await copyLegacyStore(legacySource, stageStore);
          if (!migrated) {
            if (typeof install !== "function") throw new Error(`missing Pi package closure ${identity}; enter the devshell to install the exact tracked pins`);
            await mkdir(stageStore, { recursive: true, mode: 0o700 });
            await install({ root: stage, store: stageStore, nodeModules: path.join(stageStore, "node_modules"), pins, configuration });
          }
          await resolveInstalledPinnedPackages({ candidates: [{ root: stage, source: "staging" }], pins });
          await writeFile(path.join(stage, "closure.json"), `${JSON.stringify({ schemaVersion: PI_CLOSURE_SCHEMA_VERSION, identity, configuration })}\n`, { mode: 0o600 });
          await rename(stage, paths.closure);
          published = true;
        } catch (error) {
          await rm(stage, { recursive: true, force: true }).catch(() => {});
          throw error;
        }
      }
    } finally {
      await release();
    }
  }
  const localStore = path.join(gitRoot, ".pi", "npm");
  const legacyBackup = `${localStore}.legacy`;
  const localInfo = await lstatIfPresent(localStore);
  let movedLegacy = false;
  if (localInfo?.isDirectory() && !localInfo.isSymbolicLink()) {
    if (gitRoot !== commonRoot) throw new Error(`linked worktree Pi package store must be absent or a managed closure symlink: ${localStore}`);
    if (await lstatIfPresent(legacyBackup)) throw new Error(`recoverable legacy Pi package store already exists: ${legacyBackup}`);
    await rename(localStore, legacyBackup);
    movedLegacy = true;
  }
  if (localInfo?.isSymbolicLink()) {
    let target = null;
    try { target = await realpath(localStore); } catch { /* A broken package link is not a managed closure. */ }
    let commonLegacy = null;
    try { commonLegacy = await realpath(path.join(commonRoot, ".pi", "npm")); } catch { /* No legacy store remains. */ }
    if (target !== null && !isContained(closureStateRoot(commonRoot), target) && !(gitRoot !== commonRoot && target === commonLegacy)) {
      throw new Error(`Pi package store symlink points outside the managed closure state: ${localStore}`);
    }
    await rm(localStore, { force: true });
  }
  try {
    await replaceWithClosureLink(localStore, path.join(paths.closure, ".pi", "npm"));
  } catch (error) {
    if (movedLegacy) await rename(legacyBackup, localStore).catch(() => {});
    throw error;
  }
  if (movedLegacy || await lstatIfPresent(legacyBackup)) await rm(legacyBackup, { recursive: true, force: true });
  return { mode: gitRoot === commonRoot ? "primary" : "linked", gitRoot, commonRoot, store: path.join(paths.closure, ".pi", "npm"), link: localStore, identity, published };
}

export async function auditPiPackageClosures({ cwd = process.cwd(), now = () => Date.now(), olderThanMs = PI_CLOSURE_RETENTION_MS } = {}) {
  const gitRoot = await findGitRoot(cwd);
  const commonRoot = await resolveCommonCheckoutRoot(gitRoot);
  const paths = closurePaths(commonRoot, "placeholder");
  const entries = await readdir(paths.closures, { withFileTypes: true }).catch((error) => error?.code === "ENOENT" ? [] : Promise.reject(error));
  const worktrees = execFileSync("git", ["-C", commonRoot, "worktree", "list", "--porcelain"], { encoding: "utf8" })
    .split(/\r?\n/).filter((line) => line.startsWith("worktree ")).map((line) => line.slice("worktree ".length));
  const referenced = new Set();
  for (const worktree of worktrees) {
    try { referenced.add(piPackageClosureIdentity(await readJson(path.join(worktree, SETTINGS_PATH), "project Pi settings")).identity); } catch { /* A malformed registered checkout retains every closure. */ return { closures: [], referenced: [], cleanupBlocked: true }; }
  }
  const closures = await Promise.all(entries.filter((entry) => entry.isDirectory()).map(async (entry) => {
    const info = await stat(path.join(paths.closures, entry.name));
    return { identity: entry.name, referenced: referenced.has(entry.name), ageMs: now() - info.mtimeMs };
  }));
  return { closures, referenced: [...referenced].sort(), cleanupBlocked: false, olderThanMs };
}

export async function cleanupPiPackageClosures({ cwd = process.cwd(), now = () => Date.now(), olderThanMs = PI_CLOSURE_RETENTION_MS, staleMs = PI_CLOSURE_STALE_MS } = {}) {
  const audit = await auditPiPackageClosures({ cwd, now, olderThanMs });
  if (audit.cleanupBlocked) return { ...audit, removed: [], reclaimedStaging: [], reclaimedLocks: [] };
  const gitRoot = await findGitRoot(cwd);
  const paths = closurePaths(await resolveCommonCheckoutRoot(gitRoot), "placeholder");
  const removed = [];
  for (const closure of audit.closures.filter((entry) => !entry.referenced && entry.ageMs >= olderThanMs)) {
    await rm(path.join(paths.closures, closure.identity), { recursive: true, force: true });
    removed.push(closure.identity);
  }
  const reclaim = async (directory) => {
    const entries = await readdir(directory, { withFileTypes: true }).catch((error) => error?.code === "ENOENT" ? [] : Promise.reject(error));
    const reclaimed = [];
    for (const entry of entries.slice(0, 32)) {
      const candidate = path.join(directory, entry.name);
      const info = await lstat(candidate);
      if (now() - info.mtimeMs < staleMs) continue;
      await rm(candidate, { recursive: entry.isDirectory(), force: true });
      reclaimed.push(entry.name);
    }
    return reclaimed;
  };
  return { ...audit, removed, reclaimedStaging: await reclaim(paths.staging), reclaimedLocks: await reclaim(paths.locks) };
}

const EXACT_SEMVER = "(?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const EXACT_NPM_PIN = new RegExp(`^npm:((?:@[A-Za-z0-9_.-]+/)?[A-Za-z0-9_.-]+)@(${EXACT_SEMVER})$`);

function parseExactNpmPins(settings) {
  const packages = settings?.packages;
  if (!Array.isArray(packages)) throw new Error(".pi/settings.json packages must be an array");
  return packages.flatMap((entry) => {
    const source = typeof entry === "string" ? entry : entry?.source;
    if (typeof source !== "string" || !source.startsWith("npm:")) {
      throw new Error("every repository Pi package must be an exact npm semantic-version pin");
    }
    const match = source.match(EXACT_NPM_PIN);
    if (!match) throw new Error(`repository Pi package must use an exact npm semantic-version pin: ${source}`);
    // autoload:false entries are project-local deltas over inherited global
    // packages. They constrain effective resources but do not claim that the
    // package is installed in the repository-owned store.
    if (typeof entry === "object" && entry.autoload === false) return [];
    return [{ name: match[1], version: match[2], spec: source }];
  });
}

function parseExactNpmPin(settings) {
  const pins = parseExactNpmPins(settings).filter(({ name }) => name === "dev-loops");
  if (pins.length !== 1) throw new Error(".pi/settings.json must contain exactly one dev-loops npm pin");
  return pins[0];
}

function npmPackagePath(root, name) {
  return path.join(root, ".pi", "npm", "node_modules", ...name.split("/"));
}

function isContained(parent, child) {
  const relative = path.relative(parent, child);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

async function readJson(file, description) {
  let source;
  try {
    source = await readFile(file, "utf8");
  } catch (error) {
    throw new Error(`could not read ${description} at ${file}: ${error.message}`, { cause: error });
  }
  try {
    return JSON.parse(source);
  } catch (error) {
    throw new Error(`invalid JSON in ${description} at ${file}: ${error.message}`, { cause: error });
  }
}

async function resolveInstalledPinnedPackages({ candidates, pins }) {
  const installed = [];
  const candidateRoots = await Promise.all(candidates.map(({ root }) => realpath(root)));
  for (const pin of pins) {
    let found = false;
    for (const candidate of candidates) {
      const requestedRoot = npmPackagePath(candidate.root, pin.name);
      if (!(await exists(requestedRoot))) continue;
      const packageRoot = await realpath(requestedRoot);
      const ownerIndex = candidateRoots.findIndex((root) => isContained(root, packageRoot));
      if (ownerIndex === -1) {
        throw new Error(`${pin.name} package escapes allowed project roots: ${requestedRoot}`);
      }
      const manifest = await readJson(path.join(packageRoot, "package.json"), `${pin.name} package manifest`);
      if (manifest.name !== pin.name || manifest.version !== pin.version) {
        throw new Error(
          `candidate checkout/package closure mismatch: expected ${pin.name}@${pin.version} at ${requestedRoot}, ` +
          `found ${manifest.name ?? "unknown"}@${manifest.version ?? "unknown"} from ${candidate.source}. ` +
          "Align the delivery branch's .pi/settings.json with an available exact package closure before dispatch; " +
          "do not overwrite a shared closure used by another session.",
        );
      }
      installed.push({ ...pin, packageRoot, source: candidates[ownerIndex].source });
      found = true;
      break;
    }
    if (!found) {
      throw new Error(
        `missing exact ${pin.name}@${pin.version}; checked only ${candidates.map(({ root }) => npmPackagePath(root, pin.name)).join(", ")}`,
      );
    }
  }
  return installed;
}

/**
 * Resolve only exact repository pins. Candidates are bounded to the active Git
 * root and, for a linked worktree, that worktree's common checkout root.
 */
export async function resolveDevLoopsPackageRoot({ cwd = process.cwd(), includeAllPinnedPackages = false } = {}) {
  const gitRoot = await findGitRoot(cwd);
  const commonRoot = await resolveCommonCheckoutRoot(gitRoot);
  const settingsPath = path.join(gitRoot, SETTINGS_PATH);
  const settings = await readJson(settingsPath, "project Pi settings");
  const pin = parseExactNpmPin(settings);
  const candidates = [
    { root: gitRoot, source: "git-root" },
    ...(path.resolve(commonRoot) === path.resolve(gitRoot) ? [] : [{ root: commonRoot, source: "git-common-root" }]),
  ];

  // Public CLI wrappers need only dev-loops itself. The provider preflight opts
  // into every repository pin because it inspects every installed agent set.
  const pins = includeAllPinnedPackages ? parseExactNpmPins(settings) : [pin];
  const packageRoots = await resolveInstalledPinnedPackages({ candidates, pins });
  const devLoops = packageRoots.find(({ name }) => name === pin.name);
  if (!(await exists(path.join(devLoops.packageRoot, "cli", "index.mjs")))) {
    throw new Error(`expected ${pin.name}@${pin.version} CLI at ${path.join(devLoops.packageRoot, "cli", "index.mjs")}`);
  }
  return {
    packageRoot: devLoops.packageRoot,
    packageRoots,
    version: pin.version,
    spec: pin.spec,
    source: devLoops.source,
    gitRoot,
    commonRoot,
    settingsPath,
    settings,
  };
}

function stripYamlComment(value, file) {
  let quote = null;
  let escaped = false;
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (quote === '"' && escaped) {
      escaped = false;
      continue;
    }
    if (quote === '"' && character === "\\") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") quote = character;
    else if (character === "#" && (index === 0 || /\s/.test(value[index - 1]))) return value.slice(0, index).trimEnd();
  }
  if (quote) throw new Error(`unmodelled YAML frontmatter in agent manifest ${file}: unmatched quote`);
  return value;
}

function unquoteFrontmatterScalar(value, file) {
  const trimmed = stripYamlComment(value, file).trim();
  if (!trimmed) return "";
  if ((trimmed.startsWith('"') && trimmed.endsWith('"')) || (trimmed.startsWith("'") && trimmed.endsWith("'"))) {
    return trimmed.slice(1, -1);
  }
  if (trimmed.startsWith('"') || trimmed.endsWith('"') || trimmed.startsWith("'") || trimmed.endsWith("'")) {
    throw new Error(`unmodelled YAML frontmatter in agent manifest ${file}: unmatched quote`);
  }
  return trimmed;
}

/**
 * Parse only root-level name/tools fields. Other valid YAML fields, including
 * nested lists and folded content, are deliberately ignored. An unmodelled
 * tools shape throws; Pi hooks report it advisory, while the mandatory tracked
 * pre-flight wrapper rejects it before routed actions or delegation.
 */
export function parseAgentFrontmatter(source, file) {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) throw new Error(`agent manifest has no YAML frontmatter: ${file}`);
  const lines = match[1].split(/\r?\n/);
  let name = "";
  let tools = null;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\s*(?:#.*)?$/.test(line) || /^\s/.test(line)) continue;
    const field = line.match(/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/);
    if (!field) continue;
    const [, key, raw = ""] = field;
    if (key === "name") name = unquoteFrontmatterScalar(raw, file).trim();
    if (key !== "tools") continue;
    const value = stripYamlComment(raw, file).trim();
    if (value === ">" || value === "|" || value.startsWith(">-") || value.startsWith("|-")) {
      throw new Error(`unmodelled YAML frontmatter in agent manifest ${file}: tools must be a scalar or sequence`);
    }
    if (value.startsWith("[")) {
      if (!value.endsWith("]")) throw new Error(`unmodelled YAML frontmatter in agent manifest ${file}: unterminated tools list`);
      tools = value.slice(1, -1).split(",").map((tool) => unquoteFrontmatterScalar(tool, file).trim()).filter(Boolean);
    } else if (value) {
      const scalar = unquoteFrontmatterScalar(value, file);
      tools = scalar.split(",").map((tool) => tool.trim()).filter(Boolean);
    } else {
      tools = [];
      while (index + 1 < lines.length) {
        const next = lines[index + 1];
        if (/^\s*(?:#.*)?$/.test(next)) {
          index += 1;
          continue;
        }
        const item = next.match(/^\s+-\s+(.+?)\s*$/);
        if (item) {
          const tool = unquoteFrontmatterScalar(item[1], file).trim();
          if (tool) tools.push(tool);
          index += 1;
          continue;
        }
        if (/^\s/.test(next)) {
          throw new Error(`unmodelled YAML frontmatter in agent manifest ${file}: unsupported tools sequence`);
        }
        break;
      }
    }
  }
  if (!name) throw new Error(`agent manifest requires a non-empty name: ${file}`);
  return { name, tools };
}

function assertSupportedProjectSettings(settings) {
  const subagents = settings?.subagents;
  if (!subagents || typeof subagents !== "object" || Array.isArray(subagents)) {
    throw new Error(".pi/settings.json must define subagents.projectRootResolution");
  }
  if (subagents.projectRootResolution !== "git-root") {
    throw new Error("subagents.projectRootResolution must be git-root for managed worktrees");
  }
  if (subagents.agentOverrides !== undefined) {
    throw new Error(
      "tracked agentOverrides are forbidden for tool repair: a custom agent's frontmatter tools remain authoritative; use tracked .pi/agents shadows",
    );
  }
}

async function readAgentDirectory(root, { requireTools = false } = {}) {
  if (!(await exists(root))) return [];
  const files = (await readdir(root)).filter((file) => file.endsWith(".agent.md")).sort();
  const agents = await Promise.all(files.map(async (file) => ({
    ...parseAgentFrontmatter(await readFile(path.join(root, file), "utf8"), path.join(root, file)),
    file: path.join(root, file),
  })));
  if (requireTools) {
    const inherited = agents.find(({ tools }) => tools === null);
    if (inherited) throw new Error(`tracked project agent must declare an explicit tools allowlist: ${inherited.file}`);
  }
  // A package manifest without tools inherits runtime defaults and declares no
  // allowlist for this preflight to validate.
  return agents.filter(({ tools }) => tools !== null);
}

/** Check every installed repository-pinned package after project shadows. */
export async function checkAgentToolAllowlists({
  packageRoot,
  packageRoots,
  settings,
  availableTools,
  activeAgent,
  activeTools,
  futureTools,
  projectRoot,
}) {
  const roots = packageRoots ?? (packageRoot ? [{ name: "dev-loops", packageRoot }] : []);
  if (!Array.isArray(roots) || roots.length === 0) throw new Error("at least one pinned packageRoot is required");
  if (!projectRoot) throw new Error("projectRoot is required");
  if (!Array.isArray(availableTools)) throw new Error("availableTools must be an array");
  if (activeAgent !== undefined && (typeof activeAgent !== "string" || activeAgent.trim() === "")) {
    throw new Error("activeAgent must be a non-empty string when provided");
  }
  if (activeTools !== undefined && !Array.isArray(activeTools)) throw new Error("activeTools must be an array");
  if (futureTools !== undefined && !Array.isArray(futureTools)) throw new Error("futureTools must be an array");
  assertSupportedProjectSettings(settings);
  const rootAvailable = new Set(availableTools);
  const activeAvailable = new Set(activeTools ?? availableTools);
  const futureAvailable = new Set(futureTools ?? availableTools);
  const packaged = (await Promise.all(roots.map(async ({ name = "unknown", packageRoot: root }) =>
    (await readAgentDirectory(path.join(root, "agents"))).map((agent) => ({ ...agent, packageName: name }))
  ))).flat();
  const project = await readAgentDirectory(path.join(projectRoot, PROJECT_AGENTS_PATH), { requireTools: true });
  const projectByName = new Map();
  for (const agent of project) {
    if (projectByName.has(agent.name)) throw new Error(`duplicate tracked project agent '${agent.name}'`);
    projectByName.set(agent.name, agent);
  }

  const agents = [];
  const shadowedNames = new Set();
  for (const packageAgent of packaged) {
    const shadow = projectByName.get(packageAgent.name);
    if (shadow) {
      shadowedNames.add(packageAgent.name);
      continue;
    }
    // Validate every duplicate-named package manifest. Settings order is not
    // assumed to match Pi's package-discovery precedence.
    const scope = activeAgent === undefined ? "root" : packageAgent.name === activeAgent ? "active" : "future-child";
    const scopeTools = scope === "active" ? activeAvailable : scope === "future-child" ? futureAvailable : rootAvailable;
    const missingTools = packageAgent.tools.filter((tool) => !scopeTools.has(tool));
    agents.push({
      name: packageAgent.name,
      file: packageAgent.file,
      source: `package:${packageAgent.packageName}`,
      scope,
      tools: [...packageAgent.tools],
      missingTools,
    });
  }
  for (const projectAgent of project) {
    const scope = activeAgent === undefined ? "root" : projectAgent.name === activeAgent ? "active" : "future-child";
    const scopeTools = scope === "active" ? activeAvailable : scope === "future-child" ? futureAvailable : rootAvailable;
    const missingTools = projectAgent.tools.filter((tool) => !scopeTools.has(tool));
    agents.push({
      name: projectAgent.name,
      file: projectAgent.file,
      source: "project",
      scope,
      shadowsPackages: shadowedNames.has(projectAgent.name),
      tools: [...projectAgent.tools],
      missingTools,
    });
  }

  return { ok: agents.every(({ missingTools }) => missingTools.length === 0), agents };
}

async function contentFingerprint(file) {
  const source = await readFile(file);
  return `${file}:sha256:${createHash("sha256").update(source).digest("hex")}`;
}

async function manifestFingerprints(root) {
  if (!(await exists(root))) return [];
  const files = (await readdir(root)).filter((file) => file.endsWith(".agent.md")).sort();
  return Promise.all(files.map((file) => contentFingerprint(path.join(root, file))));
}

/** Content-bound session cache key for the checkout, pins, manifests, and Pi tool scopes. */
export async function devLoopPreflightCacheKey({ resolved, availableTools, activeAgent, activeTools, futureTools }) {
  const roots = resolved.packageRoots ?? [{ name: "dev-loops", packageRoot: resolved.packageRoot }];
  const fingerprints = [
    `cwd:${resolved.gitRoot}`,
    `settings:${await contentFingerprint(resolved.settingsPath)}`,
    `root-tools:${[...availableTools].sort().join(",")}`,
    `active-agent:${activeAgent ?? "root"}`,
    `active-tools:${[...(activeTools ?? availableTools)].sort().join(",")}`,
    `future-tools:${[...(futureTools ?? availableTools)].sort().join(",")}`,
  ];
  for (const { name, version = "", packageRoot: root } of roots) {
    fingerprints.push(`package:${name}@${version}:${root}:${await contentFingerprint(path.join(root, "package.json"))}`);
    fingerprints.push(...await manifestFingerprints(path.join(root, "agents")));
  }
  fingerprints.push(...await manifestFingerprints(path.join(resolved.gitRoot, PROJECT_AGENTS_PATH)));
  return fingerprints.join("|");
}

export function formatAgentToolAllowlistFailure(result) {
  const invalid = result.agents.filter(({ missingTools }) => missingTools.length > 0);
  if (invalid.length === 0) return "";
  return `Pi dev-loop preflight failed: unavailable repository/package agent tools: ${invalid
    .map(({ name, source, scope, missingTools }) => `${name}@${source}:${scope ?? "root"}=[${missingTools.join(", ")}]`)
    .join("; ")}. Fix the tracked .pi/agents manifest or exact package installation before model execution. This preflight covers every installed repository-pinned package plus repository-local shadows; separately installed user agents are outside its claim.`;
}
