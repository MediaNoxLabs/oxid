// SPDX-License-Identifier: Apache-2.0
/**
 * Oxid compatibility evaluator for the pinned JS/TS-only size classifier.
 * Remove this adapter when the pinned upstream evaluator recognizes Rust,
 * Kotlin, and Swift with equivalent deterministic test-path handling.
 */
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { resolveDevLoopsPackageRoot } from "../lib/dev-loop-runtime.mjs";

async function loadUpstreamSizeBudget(repoRoot) {
  const packageRoot = (await resolveDevLoopsPackageRoot({ cwd: repoRoot })).packageRoot;
  const [sizeBudget, config, gateContext, output] = await Promise.all([
    import(pathToFileURL(path.join(packageRoot, "scripts/loop/check-size-budget.mjs")).href),
    import(pathToFileURL(path.join(packageRoot, "../@dev-loops/core/src/config/config.mjs")).href),
    import(pathToFileURL(path.join(packageRoot, "scripts/github/write-gate-context.mjs")).href),
    import(pathToFileURL(path.join(packageRoot, "scripts/lib/jq-output.mjs")).href),
  ]);
  return { ...sizeBudget, ...config, ...gateContext, ...output };
}

const SOURCE_EXTENSIONS = new Set([".rs", ".kt", ".swift"]);
const EXCLUDED_NAMES = new Set([".devloops", "cargo.lock", "cargo.toml", "deny.toml", "flake.lock", "justfile", "rust-toolchain.toml"]);
const EXCLUDED_EXTENSIONS = new Set([".gradle", ".json", ".kts", ".lock", ".md", ".mdx", ".nix", ".plist", ".properties", ".toml", ".xml", ".yaml", ".yml"]);
const EXCLUDED_PATH = /(?:^|\/)(?:\.cargo|\.github|\.pi|ci|config|configs|docs|fixtures|generated|nix|vendor)\//u;
const TEST_PATH = /(?:^|\/)(?:tests?|__tests__)\/|(?:_tests?|tests?)\.(?:rs|kt|swift)$/u;

export function classifyOxidSizePath(filePath) {
  const normalized = String(filePath).replaceAll("\\", "/").toLowerCase();
  const base = normalized.split("/").at(-1) ?? "";
  if (EXCLUDED_NAMES.has(base) || EXCLUDED_EXTENSIONS.has(base.slice(base.lastIndexOf("."))) || EXCLUDED_PATH.test(normalized)) return "excluded";
  const extension = base.slice(base.lastIndexOf("."));
  if (!SOURCE_EXTENSIONS.has(extension)) return "unknown";
  return TEST_PATH.test(normalized) ? "test" : "code";
}

function translateNativePath(filePath) {
  const category = classifyOxidSizePath(filePath);
  if (category === "code") return `${filePath}.js`;
  if (category === "test") return `${filePath}.test.js`;
  if (category === "excluded") return `${filePath}.md`;
  return filePath;
}

function translateTierPatterns(sizeConfig) {
  const extend = (patterns = []) => [...new Set(patterns.flatMap((pattern) => [
    pattern,
    ...[".rs", ".kt", ".swift"].filter((extension) => pattern.endsWith(extension)).map((extension) => `${pattern}.js`),
  ]))];
  const tiers = sizeConfig?.tiers ?? {};
  return {
    ...sizeConfig,
    tiers: {
      ...tiers,
      ...Object.fromEntries(["t1", "t3"].filter((tier) => tiers[tier]).map((tier) => [tier, { ...tiers[tier], patterns: extend(tiers[tier].patterns) }])),
    },
  };
}

export async function computeOxidSizeBudget({
  numstatOutput = "", sizeConfig = {}, configErrors = [], repoRoot = process.cwd(), ...rest
} = {}) {
  const { computeSizeBudget, parseNumstatZ } = await loadUpstreamSizeBudget(repoRoot);
  const translated = parseNumstatZ(numstatOutput)
    .map((file) => `${file.added}\t${file.deleted}\t${translateNativePath(file.path)}\0`)
    .join("");
  // Only native code/test extensions are translated. All other paths retain
  // upstream classification, including its majority-unclassified hard block.
  return computeSizeBudget({ ...rest, numstatOutput: translated, sizeConfig: translateTierPatterns(sizeConfig), configErrors });
}

export async function evaluateOxidPrSizeBudget({ base, head = "HEAD", repoRoot = process.cwd(), waived = false, approvedBy = null } = {}) {
  const { DIFF_ISOLATION_FLAGS, gitEnvWithoutDirOverrides, loadDevLoopConfig } = await loadUpstreamSizeBudget(repoRoot);
  const range = `${base}...${head}`;
  const git = (args) => execFileSync("git", [...DIFF_ISOLATION_FLAGS, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    env: gitEnvWithoutDirOverrides(),
    stdio: ["ignore", "pipe", "pipe"],
  });
  const { config, errors } = await loadDevLoopConfig({ repoRoot });
  try {
    return await computeOxidSizeBudget({
      nameStatusOutput: git(["diff", "--no-ext-diff", "--name-status", range]),
      diffOutput: git(["diff", "--no-ext-diff", range]),
      numstatOutput: git(["diff", "--no-ext-diff", "--numstat", "-z", range]),
      sizeConfig: config?.gates?.size ?? {}, configErrors: errors, waived, approvedBy, repoRoot,
    });
  } catch (error) {
    throw new Error(`git diff against --base ${JSON.stringify(base)} failed: ${error?.message ?? error}`);
  }
}

export async function main(argv = process.argv.slice(2), { repoRoot = process.cwd() } = {}) {
  const { parseCheckSizeBudgetCliArgs, runCli: runUpstreamSizeBudgetCli, emitResult } = await loadUpstreamSizeBudget(repoRoot);
  const options = parseCheckSizeBudgetCliArgs(argv);
  if (options.help) {
    await runUpstreamSizeBudgetCli(argv, { repoRoot });
    return 0;
  }
  const result = await evaluateOxidPrSizeBudget({
    base: options.base,
    head: options.head ?? "HEAD",
    repoRoot,
    waived: options.waived,
    approvedBy: options.approvedBy,
  });
  return emitResult(result, { jq: options.jq, silent: options.silent });
}
