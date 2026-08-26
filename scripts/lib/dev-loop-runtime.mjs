// SPDX-License-Identifier: Apache-2.0

import { readdir, readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

const SETTINGS_PATH = path.join(".pi", "settings.json");
const PACKAGE_RELATIVE_PATH = path.join(".pi", "npm", "node_modules", "dev-loops");
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

function parseExactNpmPin(settings) {
  const pins = (settings?.packages ?? []).filter((entry) => {
    if (typeof entry !== "string" || !entry.startsWith("npm:")) return false;
    const reference = entry.slice(4);
    return reference === "dev-loops" || reference.startsWith("dev-loops@");
  });
  if (pins.length !== 1) throw new Error(".pi/settings.json must contain exactly one dev-loops npm pin");
  const match = pins[0].match(
    /^npm:dev-loops@((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?)$/,
  );
  if (!match) throw new Error("dev-loops must use an exact npm semantic-version pin");
  return { name: "dev-loops", version: match[1], spec: pins[0] };
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

/**
 * Resolve only the exact repository pin. Candidates are bounded to the active
 * Git root and, for a linked worktree, that worktree's common checkout root.
 */
export async function resolveDevLoopsPackageRoot({ cwd = process.cwd() } = {}) {
  const gitRoot = await findGitRoot(cwd);
  const commonRoot = await resolveCommonCheckoutRoot(gitRoot);
  const settingsPath = path.join(gitRoot, SETTINGS_PATH);
  const settings = await readJson(settingsPath, "project Pi settings");
  const pin = parseExactNpmPin(settings);
  const candidates = [
    { root: gitRoot, source: "git-root" },
    ...(path.resolve(commonRoot) === path.resolve(gitRoot) ? [] : [{ root: commonRoot, source: "git-common-root" }]),
  ];

  for (const candidate of candidates) {
    const packageRoot = path.join(candidate.root, PACKAGE_RELATIVE_PATH);
    if (!(await exists(packageRoot))) continue;

    const [realCandidateRoot, realPackageRoot] = await Promise.all([
      realpath(candidate.root),
      realpath(packageRoot),
    ]);
    if (!isContained(realCandidateRoot, realPackageRoot)) {
      throw new Error(`dev-loops package escapes allowed project roots: ${packageRoot}`);
    }

    const manifestPath = path.join(realPackageRoot, "package.json");
    const manifest = await readJson(manifestPath, "dev-loops package manifest");
    if (manifest.name !== pin.name || manifest.version !== pin.version) {
      throw new Error(
        `expected ${pin.name}@${pin.version} at ${packageRoot}, found ${manifest.name ?? "unknown"}@${manifest.version ?? "unknown"}`,
      );
    }
    if (!(await exists(path.join(realPackageRoot, "cli", "index.mjs")))) {
      throw new Error(`expected ${pin.name}@${pin.version} CLI at ${path.join(packageRoot, "cli", "index.mjs")}`);
    }
    return {
      packageRoot: realPackageRoot,
      version: pin.version,
      spec: pin.spec,
      source: candidate.source,
      gitRoot,
      commonRoot,
      settingsPath,
      settings,
    };
  }

  throw new Error(
    `missing exact ${pin.name}@${pin.version}; checked only ${candidates.map(({ root }) => path.join(root, PACKAGE_RELATIVE_PATH)).join(", ")}`,
  );
}

function unquoteFrontmatterScalar(value, file) {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if ((trimmed.startsWith('"') && trimmed.endsWith('"')) || (trimmed.startsWith("'") && trimmed.endsWith("'"))) {
    return trimmed.slice(1, -1);
  }
  if (trimmed.startsWith('"') || trimmed.endsWith('"') || trimmed.startsWith("'") || trimmed.endsWith("'")) {
    throw new Error(`invalid YAML frontmatter in agent manifest ${file}: unmatched quote`);
  }
  return trimmed;
}

/**
 * Parse only the name/tools subset used by the preflight. Keeping this parser
 * local makes the mandatory public-CI contract independent of installed Pi
 * packages while still accepting the pinned runtime's inline and block lists.
 */
export function parseAgentFrontmatter(source, file) {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) throw new Error(`agent manifest has no YAML frontmatter: ${file}`);
  const lines = match[1].split(/\r?\n/);
  let name = "";
  let tools;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\s*(?:#.*)?$/.test(line)) continue;
    const field = line.match(/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/);
    if (!field) {
      if (/^\s+-\s+/.test(line)) throw new Error(`invalid YAML frontmatter in agent manifest ${file}: unexpected list item`);
      continue;
    }
    const [, key, raw = ""] = field;
    if (key === "name") name = unquoteFrontmatterScalar(raw, file).trim();
    if (key !== "tools") continue;
    const value = raw.trim();
    if (value.startsWith("[")) {
      if (!value.endsWith("]")) throw new Error(`invalid YAML frontmatter in agent manifest ${file}: unterminated tools list`);
      tools = value.slice(1, -1).split(",").map((tool) => unquoteFrontmatterScalar(tool, file).trim()).filter(Boolean);
    } else if (value) {
      tools = value.split(",").map((tool) => unquoteFrontmatterScalar(tool, file).trim()).filter(Boolean);
    } else {
      tools = [];
      while (index + 1 < lines.length) {
        const item = lines[index + 1].match(/^\s+-\s+(.+?)\s*$/);
        if (!item) break;
        tools.push(unquoteFrontmatterScalar(item[1], file).trim());
        index += 1;
      }
    }
  }
  if (!name || !Array.isArray(tools) || tools.length === 0 || tools.some((tool) => !tool)) {
    throw new Error(`agent manifest requires a non-empty name and tools allowlist: ${file}`);
  }
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

async function readAgentDirectory(root) {
  if (!(await exists(root))) return [];
  const files = (await readdir(root)).filter((file) => file.endsWith(".md")).sort();
  return Promise.all(files.map(async (file) => ({
    ...parseAgentFrontmatter(await readFile(path.join(root, file), "utf8"), path.join(root, file)),
    file: path.join(root, file),
  })));
}

/** Check package agents after supported repository-local project shadows. */
export async function checkAgentToolAllowlists({ packageRoot, settings, availableTools, projectRoot }) {
  if (!packageRoot) throw new Error("packageRoot is required");
  if (!projectRoot) throw new Error("projectRoot is required");
  if (!Array.isArray(availableTools)) throw new Error("availableTools must be an array");
  assertSupportedProjectSettings(settings);
  const available = new Set(availableTools);
  const packaged = await readAgentDirectory(path.join(packageRoot, "agents"));
  const project = await readAgentDirectory(path.join(projectRoot, PROJECT_AGENTS_PATH));
  const projectByName = new Map();
  for (const agent of project) {
    if (projectByName.has(agent.name)) throw new Error(`duplicate tracked project agent '${agent.name}'`);
    projectByName.set(agent.name, agent);
  }

  const agents = [];
  const covered = new Set();
  for (const packageAgent of packaged) {
    const effective = projectByName.get(packageAgent.name) ?? packageAgent;
    covered.add(effective.name);
    const missingTools = effective.tools.filter((tool) => !available.has(tool));
    agents.push({ name: effective.name, file: effective.file, source: effective === packageAgent ? "package" : "project", tools: [...effective.tools], missingTools });
  }
  for (const projectAgent of project) {
    if (covered.has(projectAgent.name)) continue;
    const missingTools = projectAgent.tools.filter((tool) => !available.has(tool));
    agents.push({ name: projectAgent.name, file: projectAgent.file, source: "project", tools: [...projectAgent.tools], missingTools });
  }

  return { ok: agents.every(({ missingTools }) => missingTools.length === 0), agents };
}

export function formatAgentToolAllowlistFailure(result) {
  const invalid = result.agents.filter(({ missingTools }) => missingTools.length > 0);
  if (invalid.length === 0) return "";
  return `Pi dev-loop preflight failed: unavailable repository/package agent tools: ${invalid
    .map(({ name, missingTools }) => `${name}=[${missingTools.join(", ")}]`)
    .join("; ")}. Fix the tracked .pi/agents manifest or exact package installation before model execution. This preflight covers the exact repository package plus repository-local shadows; separately installed user agents are outside its claim.`;
}
