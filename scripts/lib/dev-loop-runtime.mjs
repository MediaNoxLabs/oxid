// SPDX-License-Identifier: Apache-2.0

import { readdir, readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

const SETTINGS_PATH = path.join(".pi", "settings.json");
const PACKAGE_RELATIVE_PATH = path.join(".pi", "npm", "node_modules", "dev-loops");

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

function parseFrontmatter(source, file) {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) throw new Error(`agent manifest has no YAML frontmatter: ${file}`);
  const fields = new Map();
  for (const line of match[1].split(/\r?\n/)) {
    const field = line.match(/^([A-Za-z][A-Za-z0-9_-]*):\s*(.*?)\s*$/);
    if (field) fields.set(field[1], field[2]);
  }
  const unquote = (value) => value?.replace(/^(?:"([\s\S]*)"|'([\s\S]*)')$/, (_all, double, single) => double ?? single);
  const name = unquote(fields.get("name"));
  const rawTools = fields.get("tools");
  if (!name || rawTools === undefined) throw new Error(`agent manifest requires name and tools: ${file}`);
  const normalizedTools = rawTools.trim();
  if (!normalizedTools) throw new Error(`agent manifest tools must use a non-empty inline allowlist: ${file}`);
  const toolsSource = normalizedTools.startsWith("[") && normalizedTools.endsWith("]")
    ? normalizedTools.slice(1, -1)
    : normalizedTools;
  const tools = toolsSource.split(",").map((tool) => unquote(tool.trim())).filter(Boolean);
  if (!Array.isArray(tools) || tools.some((tool) => typeof tool !== "string" || !tool)) {
    throw new Error(`agent manifest has an invalid tools allowlist: ${file}`);
  }
  return { name, tools };
}

/** Check effective packaged agent allowlists after tracked project overrides. */
export async function checkAgentToolAllowlists({ packageRoot, settings, availableTools }) {
  if (!packageRoot) throw new Error("packageRoot is required");
  if (!Array.isArray(availableTools)) throw new Error("availableTools must be an array");
  const available = new Set(availableTools);
  const agentsRoot = path.join(packageRoot, "agents");
  const files = (await readdir(agentsRoot)).filter((file) => file.endsWith(".agent.md")).sort();
  const overrides = settings?.subagents?.agentOverrides ?? {};
  const agents = [];

  for (const file of files) {
    const packaged = parseFrontmatter(await readFile(path.join(agentsRoot, file), "utf8"), file);
    const override = overrides[packaged.name]?.tools;
    if (override !== undefined && (!Array.isArray(override) || override.some((tool) => typeof tool !== "string" || !tool))) {
      throw new Error(`invalid tool override for agent ${packaged.name}`);
    }
    const tools = override ?? packaged.tools;
    const missingTools = tools.filter((tool) => !available.has(tool));
    agents.push({ name: packaged.name, file, tools: [...tools], missingTools });
  }

  return { ok: agents.every(({ missingTools }) => missingTools.length === 0), agents };
}

export function formatAgentToolAllowlistFailure(result) {
  const invalid = result.agents.filter(({ missingTools }) => missingTools.length > 0);
  if (invalid.length === 0) return "";
  return `Pi dev-loop preflight failed: unavailable agent tools: ${invalid
    .map(({ name, missingTools }) => `${name}=[${missingTools.join(", ")}]`)
    .join("; ")}. Update tracked .pi/settings.json overrides or the pinned packages before model execution.`;
}
