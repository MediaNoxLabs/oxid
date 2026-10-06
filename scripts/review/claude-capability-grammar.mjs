// SPDX-License-Identifier: Apache-2.0

export const MINIMUM_CLAUDE_VERSION = [2, 1, 228];
export const MAXIMUM_EXCLUSIVE_CLAUDE_VERSION = [2, 2, 0];
export const CLAUDE_CLI_EFFORTS = Object.freeze(["low", "medium", "high", "xhigh", "max"]);

const REQUIRED_CLAUDE_FLAGS = [
  "--print",
  "--output-format",
  "--json-schema",
  "--max-budget-usd",
  "--effort",
  "--safe-mode",
  "--tools",
  "--no-session-persistence",
  "--permission-mode",
  "--system-prompt",
];

// Commander renders option entries at shallow indentation. Wrapped prose uses
// a much deeper column; accepting arbitrary leading whitespace can mistake a
// sentence such as "unless --tools names them" for the authoritative option.
const HELP_FLAG_PATTERNS = Object.freeze({
  "--print": /(?:^|\r?\n)[ \t]{0,8}(?:-p,[ \t]*)?--print(?=\s|=|<|\[|$)/m,
  "--output-format": /(?:^|\r?\n)[ \t]{0,8}--output-format(?=\s|=|<|\[|$)/m,
  "--json": /(?:^|\r?\n)[ \t]{0,8}--json(?=\s|=|<|\[|$)/m,
  "--json-schema": /(?:^|\r?\n)[ \t]{0,8}--json-schema(?=\s|=|<|\[|$)/m,
  "--max-budget-usd": /(?:^|\r?\n)[ \t]{0,8}--max-budget-usd(?=\s|=|<|\[|$)/m,
  "--effort": /(?:^|\r?\n)[ \t]{0,8}(?:-[a-zA-Z0-9]+,[ \t]*)?--effort(?=\s|=|<|\[|$)/m,
  "--safe-mode": /(?:^|\r?\n)[ \t]{0,8}--safe-mode(?=\s|=|<|\[|$)/m,
  "--tools": /(?:^|\r?\n)[ \t]{0,8}--tools(?=\s|=|<|\[|$)/m,
  "--no-session-persistence": /(?:^|\r?\n)[ \t]{0,8}--no-session-persistence(?=\s|=|<|\[|$)/m,
  "--permission-mode": /(?:^|\r?\n)[ \t]{0,8}--permission-mode(?=\s|=|<|\[|$)/m,
  "--system-prompt": /(?:^|\r?\n)[ \t]{0,8}--system-prompt(?=\s|=|<|\[|$)/m,
});

export function parseClaudeVersion(output) {
  const match = String(output).match(/(?:^|[^0-9])(\d+)\.(\d+)\.(\d+)(?:[^0-9]|$)/);
  if (!match) throw new Error("could not parse Claude CLI version");
  return match.slice(1).map(Number);
}

function compareVersion(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

export function assertMinimumClaudeVersion(
  version,
  minimum = MINIMUM_CLAUDE_VERSION,
  maximumExclusive = MAXIMUM_EXCLUSIVE_CLAUDE_VERSION,
) {
  if (!Array.isArray(version) || version.length !== 3 || version.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error("Claude CLI version must be a semantic version triple");
  }
  if (compareVersion(version, minimum) < 0 || compareVersion(version, maximumExclusive) >= 0) {
    throw new Error(
      `Claude CLI ${version.join(".")} is unsupported; require >= ${minimum.join(".")} and < ${maximumExclusive.join(".")}`,
    );
  }
  return version;
}

function helpFlagPattern(flag) {
  const pattern = HELP_FLAG_PATTERNS[flag];
  if (!pattern) throw new Error(`unsupported Claude CLI help flag: ${flag}`);
  return pattern;
}

function exactHelpFlag(help, flag) {
  return helpFlagPattern(flag).test(help);
}

function helpWindow(help, flag, length = 600) {
  const line = helpFlagPattern(flag).exec(help);
  return line ? help.slice(line.index, line.index + length) : "";
}

function helpEntry(help, flag) {
  const option = helpFlagPattern(flag);
  const lines = help.split(/\r?\n/);
  const matches = lines.flatMap((line, index) => (option.test(line) ? [index] : []));
  if (matches.length === 0) return "";
  if (matches.length !== 1) throw new Error(`Claude CLI help exposes multiple ${flag} option blocks`);
  const [start] = matches;
  const entry = [lines[start]];
  for (const line of lines.slice(start + 1)) {
    if (/^\s*-/i.test(line) || !/^\s+\S/.test(line)) break;
    entry.push(line);
  }
  return entry.join("\n");
}

function documentedEffortLevels(entry) {
  const normalizedEntry = entry.replace(/\s*\r?\n\s*/g, " ");
  const choices = [...normalizedEntry.matchAll(/choices?\s*:\s*([^)]+)/gi)].map((match) => match[1]);
  const parseEnumeration = (candidate) => {
    const withoutDefault = candidate.replace(
      /,\s*(?:default|recommended)\s*:\s*["']?[a-z][a-z0-9-]*["']?\s*$/i,
      "",
    );
    if (!/[,|]/.test(withoutDefault)) return null;
    const tokens = withoutDefault.split(/[,|]/).map((token) => token.trim().replace(/^["']|["']$/g, ""));
    if (tokens.some((token) => !/^[a-z][a-z0-9-]*$/i.test(token))) return null;
    return [...new Set(tokens)];
  };
  const lines = entry.split(/\r?\n/);
  const optionSuffix = lines[0]?.match(/--effort(?:\s+|=)<[^>]+>(.*)$/)?.[1] ?? "";
  const leadingGroups = [];
  let remainingSuffix = optionSuffix.trim();
  while (remainingSuffix.startsWith("(")) {
    const group = remainingSuffix.match(/^\(([^()]*)\)\s*/);
    if (!group) break;
    leadingGroups.push(group[1]);
    remainingSuffix = remainingSuffix.slice(group[0].length);
  }
  const continuationGroups = lines.slice(1).flatMap((line) => {
    const group = line.match(/^\s*\(([^()]*)\)\s*$/);
    return group ? [group[1]] : [];
  });
  const bareGroups = [...leadingGroups, ...continuationGroups];
  const normalizeCandidate = (candidate) => candidate?.filter(
    (effort) => CLAUDE_CLI_EFFORTS.includes(effort.toLowerCase()),
  );
  const explicitChoices = choices
    .map(parseEnumeration)
    .map(normalizeCandidate)
    .filter((candidate) => Array.isArray(candidate) && candidate.length >= 2);
  const bareChoices = bareGroups
    .filter((group) => /[,|]/.test(group))
    .filter((group) => !/choices?\s*:/i.test(group))
    .map(parseEnumeration)
    .map(normalizeCandidate)
    .filter((candidate) => Array.isArray(candidate) && candidate.length >= 2);
  const candidates = [...explicitChoices, ...bareChoices];
  if (candidates.length === 0) {
    throw new Error("Claude CLI help does not expose a recognizable review effort choice list");
  }
  const distinct = new Map(candidates.map((candidate) => [
    [...candidate].map((effort) => effort.toLowerCase()).sort().join("\0"),
    candidate,
  ]));
  if (distinct.size !== 1) {
    throw new Error("Claude CLI help exposes multiple conflicting review effort choice lists");
  }
  const documented = distinct.values().next().value;
  if (documented.some(
    (effort) => CLAUDE_CLI_EFFORTS.includes(effort.toLowerCase())
      && !CLAUDE_CLI_EFFORTS.includes(effort),
  )) {
    throw new Error("Claude CLI help documents review effort levels with unsupported casing");
  }
  const supported = documented.filter((effort) => CLAUDE_CLI_EFFORTS.includes(effort));
  if (supported.length === 0) {
    throw new Error("Claude CLI help does not document a factory-supported review effort");
  }
  return supported;
}

export function assertClaudeHelpCapabilities(help, version) {
  if (typeof help !== "string") throw new Error("Claude CLI help output must be text");
  const supportedVersion = assertMinimumClaudeVersion(version);
  const missing = REQUIRED_CLAUDE_FLAGS.filter((flag) => !exactHelpFlag(help, flag));
  if (missing.length > 0) throw new Error(`Claude CLI does not expose required review flags: ${missing.join(", ")}`);
  const permissionHelp = helpWindow(help, "--permission-mode");
  if (!/choices:[\s\S]*["']dontAsk["']/.test(permissionHelp)) {
    throw new Error("Claude CLI help does not expose the required dontAsk permission mode");
  }
  const toolsHelp = helpWindow(help, "--tools");
  if (!/Use\s+["']{2}\s+to disable all\s+tools/i.test(toolsHelp)) {
    throw new Error('Claude CLI help does not document --tools "" as the no-tools form');
  }
  const effortHelpEntry = helpEntry(help, "--effort");
  const effortChoices = documentedEffortLevels(effortHelpEntry);
  return {
    flags: [...REQUIRED_CLAUDE_FLAGS],
    permissionMode: "dontAsk",
    emptyToolsDisabled: true,
    emptyToolsBasis: "captured-help-and-bounded-version-contract",
    effortLevels: effortChoices,
    effortHelpEntry,
    minimumVersion: [...MINIMUM_CLAUDE_VERSION],
    maximumExclusiveVersion: [...MAXIMUM_EXCLUSIVE_CLAUDE_VERSION],
    observedVersion: [...supportedVersion],
  };
}

export function assertClaudeAuthHelpCapabilities(help) {
  if (typeof help !== "string" || !/^Usage:\s+claude auth status\b/m.test(help)) {
    throw new Error("Claude CLI auth-status help does not identify the expected command");
  }
  const jsonHelp = helpWindow(help, "--json", 240);
  if (!/Output as JSON\s+\(default\)/i.test(jsonHelp)) {
    throw new Error("Claude CLI auth-status help does not expose default JSON output");
  }
  return { jsonOutput: true, jsonDefault: true };
}
