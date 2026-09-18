#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

// Layer 1 of the audit framework: deterministic, read-only collection of the
// facts every audit needs, per docs/factory/audit/README.md.
//
// The inclusion rule is that anything two runs must agree on belongs here. No
// language model runs in this file, nothing is written outside the requested
// output path, and no command that mutates anything is invoked.
//
// Every collector reports its own status. A collector that cannot run says
// `unavailable` with a reason rather than emitting empty facts, because an
// empty result and an absent collector are different conclusions and an audit
// that confuses them reports "nothing found" when it means "did not look".
//
// Each collector is exported and takes its inputs explicitly, so the contract
// tests exercise the real detection logic against planted fixtures rather than
// asserting on a mock.

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";

export const COLLECTOR_KEYS = [
  "branch.protection",
  "pr.census",
  "gate.branchCoverage",
  "gate.cannotFail",
  "coverage.policyDrift",
  "adr.collisions",
  "facade.headroom",
  "advisory.state",
  "mainline.divergence",
  "issue.closureGap",
];

const ok = (facts, source) => ({ status: "ok", ...(source ? { source } : {}), facts });
const degraded = (reason, facts, source) => ({ status: "degraded", reason, ...(source ? { source } : {}), facts });
// An unavailable collector omits `facts` entirely. Setting it to null cannot
// validate against a typed facts shape, and "no facts" is clearer than a
// present key holding nothing.
const unavailable = (reason, source) => ({ status: "unavailable", reason, ...(source ? { source } : {}) });

// --- host access -------------------------------------------------------------

/** Read-only command runner. Throws on failure; callers degrade rather than crash. */
export function runner(command, args, { cwd = process.cwd(), input } = {}) {
  const output = execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    ...(input === undefined ? { stdio: ["ignore", "pipe", "pipe"] } : { input }),
  });
  return input === undefined ? output.trim() : output;
}

/**
 * Read many blobs from one branch in a single process.
 *
 * A `git show` per file per branch is hundreds of spawns on a real decision
 * corpus, and reading the working tree instead is wrong: the checkout's copy
 * of a record can differ from the branch under examination, which would yield
 * a confident, wrong status verdict. `cat-file --batch` is both correct and
 * one spawn.
 */
export function readTreeFiles(run, branch, paths, cwd) {
  const contents = new Map();
  if (paths.length === 0) return contents;
  const request = `${paths.map((file) => `${branch}:${file}`).join("\n")}\n`;
  const batch = tryRun(run, "git", ["cat-file", "--batch"], { cwd, input: request });
  if (!batch.ok) return contents;

  let cursor = 0;
  const raw = batch.out;
  for (const file of paths) {
    const newline = raw.indexOf("\n", cursor);
    if (newline === -1) break;
    const header = raw.slice(cursor, newline);
    const size = Number(header.split(" ")[2]);
    if (!Number.isFinite(size)) {
      // A missing object reports `<oid> missing`; skip it and continue.
      cursor = newline + 1;
      continue;
    }
    contents.set(file, raw.slice(newline + 1, newline + 1 + size));
    cursor = newline + 1 + size + 1;
  }
  return contents;
}

function tryRun(run, command, args, options) {
  try {
    return { ok: true, out: run(command, args, options) };
  } catch (error) {
    return {
      ok: false,
      error: (error.stderr || error.message || "").toString().trim().split("\n")[0],
      out: (error.stdout || "").toString().trim(),
      status: error.status,
    };
  }
}

function readIfPresent(root, relative) {
  const absolute = path.join(root, relative);
  return existsSync(absolute) ? readFileSync(absolute, "utf8") : null;
}

function countLines(root, relative) {
  const absolute = path.join(root, relative);
  if (!existsSync(absolute) || !statSync(absolute).isFile()) return null;
  const content = readFileSync(absolute, "utf8");
  const lines = content.split("\n");
  // A trailing newline does not add a line, matching `wc -l` on a POSIX file.
  return lines[lines.length - 1] === "" ? lines.length - 1 : lines.length;
}

// --- collectors --------------------------------------------------------------

/**
 * `branch.protection` — posture per branch.
 *
 * The heaviest single fact an audit collects: an unprotected branch that takes
 * product work disables review, status checks, and force-push defence at once.
 */
export function collectBranchProtection({ repository, branches, run = runner, cwd }) {
  const source = [];
  const facts = [];
  let anyFailure = null;

  const repoQuery = tryRun(run, "gh", ["api", `repos/${repository}`, "--jq", ".delete_branch_on_merge"], { cwd });
  source.push(`gh api repos/${repository}`);
  const deleteBranchOnMerge = repoQuery.ok ? repoQuery.out === "true" : undefined;
  if (!repoQuery.ok) anyFailure = repoQuery.error;

  for (const branch of branches) {
    const protection = tryRun(run, "gh", ["api", `repos/${repository}/branches/${branch}/protection`], { cwd });
    const rules = tryRun(run, "gh", ["api", `repos/${repository}/rules/branches/${branch}`], { cwd });
    source.push(`gh api repos/${repository}/branches/${branch}/protection`);

    if (!protection.ok && !/404|not protected/iu.test(protection.error)) {
      anyFailure = protection.error;
      continue;
    }

    const entry = { branch, protected: protection.ok };
    if (protection.ok) {
      let parsed = {};
      try {
        parsed = JSON.parse(protection.out);
      } catch {
        anyFailure = `unparseable protection payload for ${branch}`;
      }
      entry.requiredChecks = parsed.required_status_checks?.contexts ?? [];
      entry.requiredApprovals = parsed.required_pull_request_reviews?.required_approving_review_count ?? 0;
      entry.allowsForcePush = parsed.allow_force_pushes?.enabled ?? false;
      entry.allowsDeletion = parsed.allow_deletions?.enabled ?? false;
      entry.requiresSignatures = parsed.required_signatures?.enabled ?? false;
    }
    if (rules.ok) {
      try {
        entry.rulesetCount = JSON.parse(rules.out).length;
      } catch {
        entry.rulesetCount = 0;
      }
    }
    if (deleteBranchOnMerge !== undefined) entry.deleteBranchOnMerge = deleteBranchOnMerge;
    facts.push(entry);
  }

  if (facts.length === 0) return unavailable(anyFailure ?? "no branch protection facts obtained", source);
  if (anyFailure) return degraded(`partial: ${anyFailure}`, facts, source);
  return ok(facts, source);
}

/**
 * `gate.cannotFail` — declared-critical checks that cannot report failure.
 *
 * Detects the shape recorded in OXA-PRC-01: a workflow publishes a commit
 * status with a literal success state while another file lists that context
 * among its critical checks. The two files are individually defensible, which
 * is why nothing else notices.
 */
export function collectGateCannotFail({ root, criticalSource = "scripts/github/merge-milestone-pr.mjs", workflowDir = ".github/workflows" }) {
  const source = [criticalSource, workflowDir];
  const declaring = readIfPresent(root, criticalSource);
  if (declaring === null) {
    return unavailable(`${criticalSource} is absent, so no critical-check list can be resolved`, source);
  }

  // The list may be wrapped: `= [...]` and `= Object.freeze([...])` are both
  // in use, and requiring the bare form made this collector report
  // `unavailable` against the real repository.
  const block = declaring.match(/CRITICAL_CHECKS\s*=\s*(?:Object\.freeze\(\s*)?\[([\s\S]*?)\]/u);
  if (!block) return unavailable(`${criticalSource} declares no CRITICAL_CHECKS array`, source);
  const declared = Array.from(block[1].matchAll(/["'`]([^"'`]+)["'`]/gu), (match) => match[1]);

  const workflowRoot = path.join(root, workflowDir);
  const workflows = existsSync(workflowRoot)
    ? readdirSync(workflowRoot).filter((name) => /\.ya?ml$/u.test(name)).sort()
    : [];

  const facts = [];
  for (const context of declared) {
    let publishedBy;
    let reason;
    for (const workflow of workflows) {
      const relative = path.join(workflowDir, workflow);
      const content = readIfPresent(root, relative);
      if (content === null || !content.includes(context)) continue;

      const lines = content.split("\n");

      // A context is typically published twice: a `pending` seed near the top
      // and the real status later. Examining only the first occurrence misses
      // the defect entirely, so every occurrence is checked.
      //
      // Only a literal *success* is evidence. A literal `pending` is the normal
      // seeding publish that a later step overwrites, and flagging it falsely
      // accuses working gates. Likewise a nearby `continue-on-error` is not
      // evidence on its own, because the step's outcome may be converted into a
      // failure elsewhere; a wrong `canReportFailure: false` puts a false claim
      // into an audit, which costs more than a missed detection. Structural
      // cases beyond a literal state belong to OXA-PRC-01's reader.
      const occurrences = lines.reduce((found, line, index) => {
        if (line.includes(context)) found.push(index);
        return found;
      }, []);
      if (occurrences.length === 0) continue;
      publishedBy = `${relative}:${occurrences[0] + 1}`;

      for (const occurrence of occurrences) {
        const window = lines.slice(Math.max(0, occurrence - 12), occurrence + 24).join("\n");
        if (/state:\s*['"]success['"]/u.test(window)) {
          publishedBy = `${relative}:${occurrence + 1}`;
          reason = 'state is the literal "success"; the computed result does not reach it';
          break;
        }
      }
      if (reason) break;
    }
    facts.push({
      context,
      declaredCriticalBy: criticalSource,
      canReportFailure: !reason,
      ...(publishedBy ? { publishedBy } : {}),
      ...(reason ? { reason } : {}),
    });
  }

  return ok(facts, source);
}

/**
 * `gate.branchCoverage` — which gate workflows run on which branches.
 *
 * Reads the `branches:` filters under a workflow's trigger block by line
 * structure rather than by parsing YAML, since the repository carries no YAML
 * dependency. The scan is deterministic but shallow: it sees literal branch
 * lists and reports `degraded` for a workflow whose trigger it cannot read, so
 * an unreadable filter is never silently treated as full coverage.
 */
export function collectGateBranchCoverage({ root, branches, workflowDir = ".github/workflows" }) {
  const workflowRoot = path.join(root, workflowDir);
  if (!existsSync(workflowRoot)) return unavailable(`${workflowDir} is absent`, [workflowDir]);

  const facts = [];
  const unreadable = [];
  for (const name of readdirSync(workflowRoot).filter((entry) => /\.ya?ml$/u.test(entry)).sort()) {
    const relative = path.join(workflowDir, name);
    const content = readIfPresent(root, relative);
    if (content === null) continue;

    const included = new Set();
    const excluded = new Set();
    let sawTrigger = false;
    const lines = content.split("\n");
    for (let index = 0; index < lines.length; index += 1) {
      const declaration = lines[index].match(/^\s*branches(-ignore)?:/u);
      if (!declaration) continue;
      sawTrigger = true;
      const filters = declaration[1] ? excluded : included;
      const inline = lines[index].match(/\[(.*)\]/u);
      if (inline) {
        for (const item of inline[1].split(",")) {
          const value = item.trim().replace(/^['"]|['"]$/gu, "");
          if (value) filters.add(value);
        }
        continue;
      }
      for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
        const item = lines[cursor].match(/^\s*-\s*['"]?([^'"#\s]+)['"]?/u);
        if (!item) break;
        filters.add(item[1]);
      }
    }

    const includePatterns = Array.from(included).sort();
    const excludePatterns = Array.from(excluded).sort();
    const covers = (branch) => {
      const selected = includePatterns.length === 0
        || includePatterns.some((pattern) => branchPatternMatches(pattern, branch));
      return selected && !excludePatterns.some((pattern) => branchPatternMatches(pattern, branch));
    };

    if (!sawTrigger) unreadable.push(relative);
    facts.push({
      workflow: relative,
      branches: includePatterns,
      branchesIgnore: excludePatterns,
      coversDefault: covers(branches.default),
      missingFrom: branches.examined.filter((branch) => !covers(branch)),
    });
  }

  const source = [workflowDir];
  if (unreadable.length > 0) {
    return degraded(`no literal branch filter found in ${unreadable.length} workflow(s): ${unreadable.slice(0, 5).join(", ")}`, facts, source);
  }
  return ok(facts, source);
}

/** Match the `*` and `**` subset used by literal GitHub branch filters. */
function branchPatternMatches(pattern, branch) {
  const memo = new Map();
  const visit = (patternIndex, branchIndex) => {
    const key = `${patternIndex}:${branchIndex}`;
    if (memo.has(key)) return memo.get(key);
    let result;
    if (patternIndex === pattern.length) {
      result = branchIndex === branch.length;
    } else if (pattern[patternIndex] !== "*") {
      result = branchIndex < branch.length
        && pattern[patternIndex] === branch[branchIndex]
        && visit(patternIndex + 1, branchIndex + 1);
    } else {
      const double = pattern[patternIndex + 1] === "*";
      const nextPattern = patternIndex + (double ? 2 : 1);
      result = visit(nextPattern, branchIndex)
        || (branchIndex < branch.length
          && (double || branch[branchIndex] !== "/")
          && visit(patternIndex, branchIndex + 1));
    }
    memo.set(key, result);
    return result;
  };
  return visit(0, 0);
}

/**
 * `coverage.policyDrift` — enforced floors versus documented claims.
 *
 * Two independent defects: a floor nothing enforces, and a documented figure
 * that disagrees with the enforced one. Both are reported because they have
 * different fixes.
 */
export function collectCoveragePolicyDrift({
  root,
  policyPath = "scripts/coverage/policy.json",
  enforcementCandidates = ["run.sh", "justfile", "Justfile", ".github/workflows/quality.yml", ".github/workflows/coverage.yml"],
  claimPaths = ["CONTRIBUTING.md", "docs/site/src/testing-strategy.md", "docs/site/src/quality-constitution.md"],
}) {
  const source = [policyPath, ...enforcementCandidates, ...claimPaths];
  const raw = readIfPresent(root, policyPath);
  if (raw === null) return unavailable(`${policyPath} is absent`, source);

  let policy;
  try {
    policy = JSON.parse(raw);
  } catch (error) {
    return unavailable(`${policyPath} is not valid JSON: ${error.message}`, source);
  }

  const scopes = Object.entries(policy)
    .filter(([key]) => /Floor(Percent)?$/u.test(key))
    .map(([key, value]) => ({
      scope: key.replace(/Floor(Percent)?$/u, ""),
      hasFloor: typeof value === "number" && value > 0,
      ...(typeof value === "number" ? { floorPercent: value } : {}),
    }));

  let enforcementPath;
  for (const candidate of enforcementCandidates) {
    const content = readIfPresent(root, candidate);
    if (content && /--enforce\b/u.test(content)) {
      enforcementPath = candidate;
      break;
    }
  }

  const documentedClaims = [];
  for (const candidate of claimPaths) {
    const content = readIfPresent(root, candidate);
    if (content === null) continue;
    content.split("\n").forEach((line, index) => {
      const match = line.match(/(\d{2,3})\s*%\s*(?:line\s+)?coverage/iu);
      if (match) {
        documentedClaims.push({ path: candidate, line: index + 1, claimedPercent: Number(match[1]) });
      }
    });
  }

  return ok(
    {
      enforced: Boolean(enforcementPath),
      ...(enforcementPath ? { enforcementPath } : {}),
      scopes,
      documentedClaims,
    },
    source,
  );
}

/**
 * `adr.collisions` — duplicate numbers and status-blind backlinks.
 *
 * Evaluated against the union of every examined branch, not one tree: two
 * records can take the same number under distinct filenames, so the merge
 * produces no conflict and a per-branch check finds both corpora clean.
 */
export function collectAdrCollisions({ branches, adrDir = "docs/adr", run = runner, cwd }) {
  const source = [adrDir];
  const byNumber = new Map();
  const seen = new Map();
  let degradedReason = null;

  const record = (number, file, branch) => {
    if (!byNumber.has(number)) byNumber.set(number, new Map());
    const paths = byNumber.get(number);
    if (!paths.has(file)) paths.set(file, new Set());
    paths.get(file).add(branch);
  };

  for (const branch of branches) {
    const listing = tryRun(run, "git", ["ls-tree", "-r", "--name-only", branch, "--", adrDir], { cwd });
    if (!listing.ok) {
      degradedReason = `cannot list ${adrDir} on ${branch}: ${listing.error}`;
      continue;
    }
    source.push(`git ls-tree -r --name-only ${branch} -- ${adrDir}`);
    const numbered = listing.out.split("\n").filter(Boolean).filter((file) => {
      const match = path.basename(file).match(/^(\d{4})-/u);
      if (!match) return false;
      record(match[1], file, branch);
      return true;
    });
    for (const [file, content] of readTreeFiles(run, branch, numbered.filter((file) => !seen.has(file)), cwd)) {
      seen.set(file, content);
    }
  }

  if (byNumber.size === 0) {
    return degradedReason ? unavailable(degradedReason, source) : ok({ duplicateNumbers: [], statusBlindBacklinks: [], mergedCorpusLintFailures: 0 }, source);
  }

  const duplicateNumbers = [];
  for (const [number, paths] of [...byNumber].sort(([a], [b]) => a.localeCompare(b))) {
    if (paths.size < 2) continue;
    duplicateNumbers.push({
      number,
      paths: [...paths.keys()].sort(),
      branches: [...new Set([...paths.values()].flatMap((set) => [...set]))].sort(),
    });
  }

  const statusOf = (content) => content?.match(/^\s*(?:[-*]\s*)?(?:\*\*)?Status(?:\*\*)?\s*[::]\s*(\w+)/imu)?.[1] ?? null;
  const pathForNumber = (number) => [...byNumber.get(number)?.keys() ?? []][0] ?? null;

  const statusBlindBacklinks = [];
  for (const [file, content] of [...seen].sort(([a], [b]) => a.localeCompare(b))) {
    const fromStatus = statusOf(content);
    for (const match of content.matchAll(/Amended by:?\s*ADR-(\d{4})/giu)) {
      const targetPath = pathForNumber(match[1]);
      const targetStatus = targetPath ? statusOf(seen.get(targetPath)) : null;
      if (fromStatus && targetStatus && /^accepted$/iu.test(fromStatus) && !/^accepted$/iu.test(targetStatus)) {
        statusBlindBacklinks.push({
          from: file,
          fromStatus,
          to: targetPath ?? `ADR-${match[1]}`,
          toStatus: targetStatus,
        });
      }
    }
  }

  const facts = {
    duplicateNumbers,
    statusBlindBacklinks,
    mergedCorpusLintFailures: duplicateNumbers.length + statusBlindBacklinks.length,
  };
  return degradedReason ? degraded(degradedReason, facts, source) : ok(facts, source);
}

/**
 * `facade.headroom` — governed files against their ceilings, plus coverage.
 *
 * Reports the largest ungoverned sibling per governed crate, because a ceiling
 * on one file while its siblings are unbounded governs one file and tells the
 * mass where to move.
 */
export function collectFacadeHeadroom({ root, configPath = "scripts/architecture/capability-facades.json", manifestPath = "Cargo.toml" }) {
  const source = [configPath, manifestPath];
  const raw = readIfPresent(root, configPath);
  if (raw === null) return unavailable(`${configPath} is absent`, source);

  let config;
  try {
    config = JSON.parse(raw);
  } catch (error) {
    return unavailable(`${configPath} is not valid JSON: ${error.message}`, source);
  }

  // The config declares ceilings per crate, either as a path->ceiling map or as
  // a file list sharing one crate-wide ceiling. `sourceRoot` is what makes the
  // ungoverned-sibling question answerable, so it is carried through.
  const entries = [];
  for (const crate of config.crates ?? []) {
    const byPath = crate.facadeMaximumPhysicalLinesByPath ?? {};
    const shared = crate.facadeMaximumPhysicalLines;
    const files = new Set([...Object.keys(byPath), ...(crate.facadeFiles ?? [])]);
    for (const file of files) {
      const ceiling = byPath[file] ?? shared;
      if (typeof ceiling !== "number") continue;
      entries.push({ path: file, ceiling, sourceRoot: crate.sourceRoot ?? path.dirname(file), crate: crate.name });
    }
  }

  const governed = [];
  const missing = [];
  for (const entry of entries.sort((a, b) => a.path.localeCompare(b.path))) {
    const actual = countLines(root, entry.path);
    if (actual === null) {
      missing.push(entry.path);
      continue;
    }
    const record = { path: entry.path, ceiling: entry.ceiling, actual, headroom: entry.ceiling - actual };
    const directory = entry.sourceRoot;
    const absoluteDirectory = path.join(root, directory);
    if (existsSync(absoluteDirectory)) {
      const governedInDirectory = new Set(entries.map((candidate) => candidate.path));
      let largest = null;
      for (const sibling of readdirSync(absoluteDirectory, { withFileTypes: true })
        .filter((child) => child.isFile() && /\.(rs|kt|swift|ts|tsx|mjs)$/u.test(child.name))
        .map((child) => child.name)
        .sort()) {
        const relative = path.join(directory, sibling);
        if (governedInDirectory.has(relative)) continue;
        const lines = countLines(root, relative);
        if (lines === null) continue;
        if (!largest || lines > largest.lines) largest = { path: relative, lines };
      }
      if (largest) record.largestUngovernedSibling = largest;
    }
    governed.push(record);
  }

  const manifest = readIfPresent(root, manifestPath) ?? "";
  const membersBlock = manifest.match(/members\s*=\s*\[([\s\S]*?)\]/u);
  const workspaceMemberCount = membersBlock
    ? Array.from(membersBlock[1].matchAll(/["']([^"']+)["']/gu)).length
    : 0;

  const facts = {
    governed,
    governedCrateCount: new Set(entries.map((entry) => entry.crate)).size,
    workspaceMemberCount,
  };
  if (missing.length > 0) {
    return degraded(`${missing.length} governed path(s) absent from the tree: ${missing.slice(0, 3).join(", ")}`, facts, source);
  }
  return ok(facts, source);
}

/**
 * `advisory.state` — advisory classes, allowlist hygiene, unpinned actions.
 *
 * Never runs an upgrade. An allowlist entry without a rationale or a review
 * date is reported because an exception with no expiry becomes permanent.
 */
export function collectAdvisoryState({
  root,
  exceptionsPath = "docs/security/advisory-exceptions.md",
  gatePath = "scripts/check-advisories.sh",
  workflowDir = ".github/workflows",
  run = runner,
  cwd,
  offline = false,
}) {
  const source = [exceptionsPath, gatePath, workflowDir];
  const gate = readIfPresent(root, gatePath) ?? "";
  // Terminate on a line that is exactly ")": the block's rationale comments
  // contain parentheses, and matching the first ")" anywhere truncated the
  // block before its entries, reporting an empty allowlist.
  const allowlistBlock = gate.match(/allowed_yanked=\(([\s\S]*?)^\)/mu);
  const exceptions = readIfPresent(root, exceptionsPath) ?? "";

  const allowlist = [];
  if (allowlistBlock) {
    // The block carries its rationale as shell comments, and those comments
    // contain quoted prose. Strip them before extracting, and require a
    // `name@version` shape so a quoted phrase cannot masquerade as an entry.
    const body = allowlistBlock[1];
    const uncommented = body
      .split("\n")
      .map((line) => line.replace(/(^|\s)#.*$/u, ""))
      .join("\n");
    const rationale = body
      .split("\n")
      .filter((line) => /^\s*#/u.test(line))
      .join("\n");
    for (const match of uncommented.matchAll(/"([A-Za-z0-9_-]+@[0-9][^"]*)"/gu)) {
      const entry = match[1];
      const crate = entry.split("@")[0];
      const context = `${rationale}\n${exceptions.includes(crate) ? exceptions : ""}`;
      allowlist.push({
        entry,
        hasRationale: rationale.toLocaleLowerCase("en-US").includes(crate.toLocaleLowerCase("en-US"))
          || exceptions.includes(crate),
        hasReviewDate: /(?:review|revisit|expires?)\D{0,24}\d{4}-\d{2}-\d{2}/iu.test(context),
      });
    }
  }

  const unpinnedActions = [];
  const workflowRoot = path.join(root, workflowDir);
  if (existsSync(workflowRoot)) {
    for (const name of readdirSync(workflowRoot).filter((entry) => /\.ya?ml$/u.test(entry)).sort()) {
      const content = readIfPresent(root, path.join(workflowDir, name)) ?? "";
      // `uses:` appears both as a step key and as the first key of a list
      // item (`- uses: ...`), so the leading dash is optional.
      for (const match of content.matchAll(/^\s*(?:-\s*)?uses:\s*([^\s#]+)/gmu)) {
        const reference = match[1];
        if (reference.startsWith("./") || reference.startsWith(".github/")) continue;
        const [, version] = reference.split("@");
        if (!version || !/^[0-9a-f]{40}$/u.test(version)) {
          unpinnedActions.push({ workflow: path.join(workflowDir, name), uses: reference });
        }
      }
    }
  }

  const base = { classes: {}, allowlist, unpinnedActions };
  if (offline) {
    return degraded("cargo audit not run; advisory classes unresolved", { vulnerabilitiesFound: 0, ...base }, source);
  }

  const audit = tryRun(run, "cargo", ["audit", "--json"], { cwd });
  let report = {};
  try {
    report = JSON.parse(audit.out);
  } catch {
    const reason = audit.ok
      ? "cargo audit output is not valid JSON"
      : `cargo audit unavailable: ${audit.error}`;
    return degraded(reason, { vulnerabilitiesFound: 0, ...base }, source);
  }
  const warnings = report.warnings ?? {};
  return ok(
    {
      vulnerabilitiesFound: report.vulnerabilities?.count ?? 0,
      classes: Object.fromEntries(Object.entries(warnings).map(([key, value]) => [key, Array.isArray(value) ? value.length : 0])),
      allowlist,
      unpinnedActions,
    },
    [...source, "cargo audit --json"],
  );
}

/**
 * `mainline.divergence` — what differs between mainlines.
 *
 * `bothSidesModified` is the dangerous set: those paths merge textually with no
 * conflict, so nothing warns and the merged tree may not compile.
 */
export function collectMainlineDivergence({ branches, run = runner, cwd }) {
  const source = [];
  const pairs = [];
  let mergeBase = null;
  let failure = null;

  for (let index = 0; index < branches.length; index += 1) {
    for (let other = index + 1; other < branches.length; other += 1) {
      const left = branches[index];
      const right = branches[other];
      const base = tryRun(run, "git", ["merge-base", left, right], { cwd });
      if (!base.ok) {
        failure = `cannot resolve merge-base of ${left} and ${right}: ${base.error}`;
        continue;
      }
      if (!mergeBase) mergeBase = base.out;
      source.push(`git merge-base ${left} ${right}`);

      const leftOnly = tryRun(run, "git", ["rev-list", "--count", `${base.out}..${left}`], { cwd });
      const rightOnly = tryRun(run, "git", ["rev-list", "--count", `${base.out}..${right}`], { cwd });
      const leftPaths = tryRun(run, "git", ["diff", "--name-only", `${base.out}..${left}`], { cwd });
      const rightPaths = tryRun(run, "git", ["diff", "--name-only", `${base.out}..${right}`], { cwd });

      const leftSet = new Set(leftPaths.ok ? leftPaths.out.split("\n").filter(Boolean) : []);
      const rightSet = new Set(rightPaths.ok ? rightPaths.out.split("\n").filter(Boolean) : []);
      const bothSidesModified = [...leftSet].filter((file) => rightSet.has(file)).sort();
      const changedFiles = new Set([...leftSet, ...rightSet]).size;

      pairs.push({
        left,
        right,
        changedFiles,
        ...(leftOnly.ok ? { leftOnlyCommits: Number(leftOnly.out) } : {}),
        ...(rightOnly.ok ? { rightOnlyCommits: Number(rightOnly.out) } : {}),
        bothSidesModified,
      });
    }
  }

  if (pairs.length === 0 || !mergeBase) {
    return unavailable(failure ?? "no branch pair could be compared", source);
  }
  const facts = { mergeBase, pairs };
  return failure ? degraded(failure, facts, source) : ok(facts, source);
}

/**
 * `pr.census` — merged pull requests and churn in the window.
 */
export function collectPrCensus({ repository, since, until, run = runner, cwd }) {
  const source = [`gh pr list --repo ${repository} --state merged`];
  const query = tryRun(
    run,
    "gh",
    ["pr", "list", "--repo", repository, "--state", "merged", "--limit", "500",
      "--json", "number,title,baseRefName,additions,deletions,mergedAt,reviews"],
    { cwd },
  );
  if (!query.ok) return unavailable(`gh pr list failed: ${query.error}`, source);

  let list;
  try {
    list = JSON.parse(query.out);
  } catch (error) {
    return unavailable(`gh pr list output is not valid JSON: ${error.message}`, source);
  }

  const lowerBound = since ? Date.parse(since) : Number.NEGATIVE_INFINITY;
  const upperBound = until ? Date.parse(until) : Number.POSITIVE_INFINITY;
  const inWindow = list.filter((entry) => {
    const merged = Date.parse(entry.mergedAt);
    return Number.isFinite(merged) && merged >= lowerBound && merged <= upperBound;
  });

  const byType = {};
  const byTargetBranch = {};
  let additions = 0;
  let deletions = 0;
  let withApprovingReview = 0;
  for (const entry of inWindow) {
    const type = entry.title?.match(/^([a-z]+)(?:\([a-z0-9-]+\))?!?:/u)?.[1] ?? "other";
    byType[type] = (byType[type] ?? 0) + 1;
    byTargetBranch[entry.baseRefName] = (byTargetBranch[entry.baseRefName] ?? 0) + 1;
    additions += entry.additions ?? 0;
    deletions += entry.deletions ?? 0;
    if ((entry.reviews ?? []).some((review) => review.state === "APPROVED")) withApprovingReview += 1;
  }

  const facts = {
    merged: inWindow.length,
    additions,
    deletions,
    byType: Object.fromEntries(Object.entries(byType).sort(([a], [b]) => a.localeCompare(b))),
    byTargetBranch: Object.fromEntries(Object.entries(byTargetBranch).sort(([a], [b]) => a.localeCompare(b))),
    withApprovingReview,
  };
  // 200 is the page limit; a full page means the window may be truncated.
  if (list.length >= 500) {
    return degraded("pull request listing hit the 500-item page limit; the window may be truncated", facts, source);
  }
  return ok(facts, source);
}

/**
 * `issue.closureGap` — fixes that merged without closing their issue.
 *
 * Closing keywords fire only on merges into the default branch, so a train
 * merge never closes anything. The default branch is collected rather than
 * assumed, because it has changed and a stale premise here produces confident,
 * wrong findings.
 */
export function collectIssueClosureGap({ repository, defaultBranch, defaultBranchCollected = true, since, run = runner, cwd }) {
  const source = [`gh pr list --repo ${repository} --state merged`];
  // Every verdict in this collector turns on which branch is default. An
  // assumed value produces confident, wrong findings — the exact failure the
  // charter records — so refuse rather than guess.
  if (!defaultBranchCollected) {
    return unavailable("the default branch could not be collected, and every closure verdict depends on it", source);
  }
  const query = tryRun(
    run,
    "gh",
    ["pr", "list", "--repo", repository, "--state", "merged", "--limit", "500",
      "--json", "number,body,baseRefName,mergedAt"],
    { cwd },
  );
  if (!query.ok) return unavailable(`gh pr list failed: ${query.error}`, source);

  let list;
  try {
    list = JSON.parse(query.out);
  } catch (error) {
    return unavailable(`gh pr list output is not valid JSON: ${error.message}`, source);
  }

  const lowerBound = since ? Date.parse(since) : Number.NEGATIVE_INFINITY;
  const closing = /(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#(\d+)/giu;
  const candidates = new Map();
  for (const entry of list) {
    if (Date.parse(entry.mergedAt) < lowerBound) continue;
    for (const match of (entry.body ?? "").matchAll(closing)) {
      const issue = Number(match[1]);
      if (!candidates.has(issue)) {
        candidates.set(issue, { issue, closedBy: entry.number, mergedInto: entry.baseRefName });
      }
    }
  }

  // One listing rather than one request per candidate. A per-issue query makes
  // the collector's cost scale with the backlog, which on a real window means
  // hundreds of serial round trips.
  const openQuery = tryRun(
    run,
    "gh",
    ["issue", "list", "--repo", repository, "--state", "open", "--limit", "800", "--json", "number"],
    { cwd },
  );
  if (!openQuery.ok) return degraded(`open issue listing failed: ${openQuery.error}`, [], source);
  source.push(`gh issue list --repo ${repository} --state open`);

  let openIssues;
  try {
    openIssues = new Set(JSON.parse(openQuery.out).map((entry) => entry.number));
  } catch (error) {
    return degraded(`open issue listing is not valid JSON: ${error.message}`, [], source);
  }

  const facts = [...candidates.values()]
    .filter((candidate) => openIssues.has(candidate.issue))
    .sort((a, b) => a.issue - b.issue)
    .map((candidate) => ({
      ...candidate,
      stillOpen: true,
      keywordWouldFire: candidate.mergedInto === defaultBranch,
    }));

  return ok(facts, source);
}

// --- orchestration -----------------------------------------------------------

export function resolveBranches({ repository, branches, run = runner, cwd }) {
  const defaultQuery = tryRun(run, "gh", ["api", `repos/${repository}`, "--jq", ".default_branch"], { cwd });
  const defaultBranch = defaultQuery.ok && defaultQuery.out ? defaultQuery.out : null;
  const resolved = [];
  for (const branch of branches) {
    const sha = tryRun(run, "git", ["rev-parse", `refs/remotes/origin/${branch}`], { cwd });
    if (sha.ok && /^[0-9a-f]{40}$/u.test(sha.out)) resolved.push({ name: branch, sha: sha.out });
  }
  return { defaultBranch, resolved, defaultResolved: defaultQuery.ok };
}

export function collect({
  repository,
  primary,
  comparisons = [],
  since = null,
  until = null,
  root = process.cwd(),
  run = runner,
  offline = false,
  now = () => new Date().toISOString(),
}) {
  validateWindowBound("since", since);
  validateWindowBound("until", until);
  if (since && until && Date.parse(since) > Date.parse(until)) {
    throw new Error("--since must not be later than --until");
  }
  const branchNames = [primary, ...comparisons].filter(Boolean);
  const { defaultBranch, resolved, defaultResolved } = resolveBranches({ repository, branches: branchNames, run, cwd: root });
  const refs = branchNames.map((name) => `refs/remotes/origin/${name}`);

  // A collector that throws unexpectedly degrades to `unavailable` rather than
  // killing the run: losing nine collectors to one bug is worse, and a crash
  // before the write would leave the previous artifact in place looking
  // current, which is the most dangerous failure this file has.
  const guard = (key, thunk) => {
    try {
      return thunk();
    } catch (error) {
      return unavailable(`collector threw: ${error.message}`, [key]);
    }
  };

  const collectors = {
    "branch.protection": guard("branch.protection", () => collectBranchProtection({ repository, branches: branchNames, run, cwd: root })),
    "pr.census": guard("pr.census", () => collectPrCensus({ repository, since, until, run, cwd: root })),
    "gate.branchCoverage": guard("gate.branchCoverage", () => defaultResolved
      ? collectGateBranchCoverage({ root, branches: { default: defaultBranch, examined: branchNames } })
      : unavailable("the default branch could not be collected", ["gh api repository default branch"])),
    "gate.cannotFail": guard("gate.cannotFail", () => collectGateCannotFail({ root })),
    "coverage.policyDrift": guard("coverage.policyDrift", () => collectCoveragePolicyDrift({ root })),
    "adr.collisions": guard("adr.collisions", () => collectAdrCollisions({ branches: refs, run, cwd: root })),
    "facade.headroom": guard("facade.headroom", () => collectFacadeHeadroom({ root })),
    "advisory.state": guard("advisory.state", () => collectAdvisoryState({ root, run, cwd: root, offline })),
    "mainline.divergence": guard("mainline.divergence", () => collectMainlineDivergence({ branches: refs, run, cwd: root })),
    "issue.closureGap": guard("issue.closureGap", () => collectIssueClosureGap({
      repository,
      defaultBranch,
      defaultBranchCollected: defaultResolved,
      since,
      run,
      cwd: root,
    })),
  };

  return {
    schemaVersion: 1,
    collectedAt: now(),
    repository,
    defaultBranch,
    branches: resolved.map((entry) => ({
      ...entry,
      role: entry.name === primary ? "primary" : entry.name === defaultBranch ? "comparison" : "baseline",
    })),
    collectors,
  };
}

function validateWindowBound(name, value) {
  if (value !== null && !Number.isFinite(Date.parse(value))) {
    throw new Error(`--${name} must be an ISO-8601 date-time, received ${JSON.stringify(value)}`);
  }
}

function parseArguments(argv) {
  const options = {
    repository: "MediaNoxLabs/oxid",
    primary: null,
    comparisons: [],
    since: null,
    until: null,
    out: null,
    offline: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--repository") options.repository = argv[++index];
    else if (argument === "--branch") options.primary = argv[++index];
    else if (argument === "--compare") options.comparisons.push(argv[++index]);
    else if (argument === "--since") options.since = argv[++index];
    else if (argument === "--until") options.until = argv[++index];
    else if (argument === "--out") options.out = argv[++index];
    else if (argument === "--offline") options.offline = true;
    else if (argument === "--type") index += 1; // accepted for symmetry with the skill; collectors are type-independent
    else throw new Error(`unknown option ${argument}`);
  }
  if (!options.primary) throw new Error("usage: collect.mjs --branch <branch> [--compare <branch>]... [--since <iso-date-time>] [--out <file>] [--offline]");
  return options;
}

function main(argv) {
  const options = parseArguments(argv);
  const evidence = collect(options);

  const serialized = `${JSON.stringify(evidence, null, 2)}\n`;
  if (options.out) {
    mkdirSync(path.dirname(options.out), { recursive: true });
    writeFileSync(options.out, serialized);
  } else {
    process.stdout.write(serialized);
  }

  const summary = Object.entries(evidence.collectors)
    .map(([key, value]) => `${key}=${value.status}`)
    .join(" ");
  process.stderr.write(`${summary}\n`);

  // An audit can proceed on degraded evidence provided it says so; it cannot
  // proceed on evidence it does not know is missing. Exit non-zero only when
  // nothing was collected at all.
  const anyOk = Object.values(evidence.collectors).some((value) => value.status !== "unavailable");
  return anyOk ? 0 : 1;
}

if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exit(2);
  }
}
