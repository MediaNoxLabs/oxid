// SPDX-License-Identifier: Apache-2.0

import { readdir, readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

const SETTINGS_PATH = path.join(".pi", "settings.json");
const PROJECT_AGENTS_PATH = path.join(".pi", "agents");

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
  let current = path.resolve(cwd);
  while (true) {
    if (await exists(path.join(current, ".git"))) return current;
    const parent = path.dirname(current);
    if (parent === current) throw new Error(`not inside a Git checkout: ${cwd}`);
    current = parent;
  }
}

async function resolveCommonCheckoutRoot(gitRoot) {
  const dotGit = path.join(gitRoot, ".git");
  const dotGitStat = await stat(dotGit);
  if (dotGitStat.isDirectory()) return gitRoot;
  if (!dotGitStat.isFile()) throw new Error(`unsupported Git metadata at ${dotGit}`);

  const marker = (await readFile(dotGit, "utf8")).trim().match(/^gitdir:\s*(.+)$/i);
  if (!marker) throw new Error(`could not parse linked-worktree metadata at ${dotGit}`);
  const gitDir = path.resolve(gitRoot, marker[1]);
  const normalized = path.normalize(gitDir);
  const worktreesParent = path.dirname(path.dirname(normalized));
  if (path.basename(worktreesParent) !== ".git" || path.basename(path.dirname(normalized)) !== "worktrees") {
    throw new Error(`linked worktree does not identify a bounded common checkout: ${dotGit}`);
  }
  return path.dirname(worktreesParent);
}

const EXACT_SEMVER = "(?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const EXACT_NPM_PIN = new RegExp(`^npm:((?:@[A-Za-z0-9_.-]+/)?[A-Za-z0-9_.-]+)@(${EXACT_SEMVER})$`);

function parseExactNpmPins(settings) {
  const packages = settings?.packages;
  if (!Array.isArray(packages)) throw new Error(".pi/settings.json packages must be an array");
  return packages.map((entry) => {
    if (typeof entry !== "string" || !entry.startsWith("npm:")) {
      throw new Error("every repository Pi package must be an exact npm semantic-version pin");
    }
    const match = entry.match(EXACT_NPM_PIN);
    if (!match) throw new Error(`repository Pi package must use an exact npm semantic-version pin: ${entry}`);
    return { name: match[1], version: match[2], spec: entry };
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
  for (const pin of pins) {
    let found = false;
    for (const candidate of candidates) {
      const requestedRoot = npmPackagePath(candidate.root, pin.name);
      if (!(await exists(requestedRoot))) continue;
      const [realCandidateRoot, packageRoot] = await Promise.all([realpath(candidate.root), realpath(requestedRoot)]);
      if (!isContained(realCandidateRoot, packageRoot)) {
        throw new Error(`${pin.name} package escapes allowed project roots: ${requestedRoot}`);
      }
      const manifest = await readJson(path.join(packageRoot, "package.json"), `${pin.name} package manifest`);
      if (manifest.name !== pin.name || manifest.version !== pin.version) {
        throw new Error(
          `expected ${pin.name}@${pin.version} at ${requestedRoot}, found ${manifest.name ?? "unknown"}@${manifest.version ?? "unknown"}`,
        );
      }
      installed.push({ ...pin, packageRoot, source: candidate.source });
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
 * tools shape throws so input can warn while agent/provider launch fails closed.
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
      "tracked agentOverrides are forbidden for tool repair: pi-subagents 0.42.1 does not replace a custom agent's frontmatter tools; use tracked .pi/agents shadows",
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
export async function checkAgentToolAllowlists({ packageRoot, packageRoots, settings, availableTools, projectRoot }) {
  const roots = packageRoots ?? (packageRoot ? [{ name: "dev-loops", packageRoot }] : []);
  if (!Array.isArray(roots) || roots.length === 0) throw new Error("at least one pinned packageRoot is required");
  if (!projectRoot) throw new Error("projectRoot is required");
  if (!Array.isArray(availableTools)) throw new Error("availableTools must be an array");
  assertSupportedProjectSettings(settings);
  const available = new Set(availableTools);
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
    const missingTools = packageAgent.tools.filter((tool) => !available.has(tool));
    agents.push({
      name: packageAgent.name,
      file: packageAgent.file,
      source: `package:${packageAgent.packageName}`,
      tools: [...packageAgent.tools],
      missingTools,
    });
  }
  for (const projectAgent of project) {
    const missingTools = projectAgent.tools.filter((tool) => !available.has(tool));
    agents.push({
      name: projectAgent.name,
      file: projectAgent.file,
      source: "project",
      shadowsPackages: shadowedNames.has(projectAgent.name),
      tools: [...projectAgent.tools],
      missingTools,
    });
  }

  return { ok: agents.every(({ missingTools }) => missingTools.length === 0), agents };
}

async function manifestFingerprints(root) {
  if (!(await exists(root))) return [];
  const files = (await readdir(root)).filter((file) => file.endsWith(".agent.md")).sort();
  return Promise.all(files.map(async (file) => {
    const candidate = path.join(root, file);
    const info = await stat(candidate);
    return `${candidate}:${info.mtimeMs}:${info.size}`;
  }));
}

/** Cheap cache key: cwd/settings/package/manifest identity plus Pi's tool set. */
export async function devLoopPreflightCacheKey({ resolved, availableTools }) {
  const settingsInfo = await stat(resolved.settingsPath);
  const roots = resolved.packageRoots ?? [{ name: "dev-loops", packageRoot: resolved.packageRoot }];
  const fingerprints = [
    `cwd:${resolved.gitRoot}`,
    `settings:${resolved.settingsPath}:${settingsInfo.mtimeMs}:${settingsInfo.size}`,
    `tools:${[...availableTools].sort().join(",")}`,
  ];
  for (const { name, version = "", packageRoot: root } of roots) {
    fingerprints.push(`package:${name}@${version}:${root}`);
    fingerprints.push(...await manifestFingerprints(path.join(root, "agents")));
  }
  fingerprints.push(...await manifestFingerprints(path.join(resolved.gitRoot, PROJECT_AGENTS_PATH)));
  return fingerprints.join("|");
}

export function formatAgentToolAllowlistFailure(result) {
  const invalid = result.agents.filter(({ missingTools }) => missingTools.length > 0);
  if (invalid.length === 0) return "";
  return `Pi dev-loop preflight failed: unavailable repository/package agent tools: ${invalid
    .map(({ name, source, missingTools }) => `${name}@${source}=[${missingTools.join(", ")}]`)
    .join("; ")}. Fix the tracked .pi/agents manifest or exact package installation before model execution. This preflight covers every installed repository-pinned package plus repository-local shadows; separately installed user agents are outside its claim.`;
}
