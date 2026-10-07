// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import {
  assertClaudeAuthHelpCapabilities,
  assertClaudeHelpCapabilities,
  assertMinimumClaudeVersion,
  MAXIMUM_EXCLUSIVE_CLAUDE_VERSION,
  parseClaudeVersion,
} from "../../scripts/review/claude-capability-grammar.mjs";

const baseHelp = [
  "  --print",
  "  --output-format <format>",
  "  --json-schema <schema>",
  "  --max-budget-usd <amount>",
  "  --effort <level> (low, medium, high, xhigh, max)",
  "  --safe-mode",
  '  --tools <tools...> Specify tools. Use "" to disable all tools.',
  "  --no-session-persistence",
  '  --permission-mode <mode> (choices: "acceptEdits", "dontAsk", "plan")',
  "  --system-prompt <prompt>",
].join("\n");
const knownEfforts = ["low", "medium", "high", "xhigh", "max"];
const version = [2, 1, 228];

test("Claude version grammar preserves the bounded 2.1.x contract", () => {
  assert.deepEqual(parseClaudeVersion("2.1.228 (Claude Code)"), version);
  assert.deepEqual(assertMinimumClaudeVersion(version), version);
  assert.throws(() => assertMinimumClaudeVersion([2, 1, 227]), /unsupported; require >= 2\.1\.228 and < 2\.2\.0/u);
  assert.throws(() => assertMinimumClaudeVersion(MAXIMUM_EXCLUSIVE_CLAUDE_VERSION), /unsupported.*< 2\.2\.0/u);
});

test("Claude option grammar accepts reviewed aliases, indentation, and captured layouts", async (t) => {
  const captured = [
    "  --effort <level>                      Effort level for the current session",
    "                                        (low, medium, high, xhigh, max)",
  ].join("\n");
  const accepted = [
    ["captured 2.1.228 entry", baseHelp.replace(/  --effort.*$/m, captured), knownEfforts],
    ["CRLF and shallow indentation", baseHelp.replace("  --safe-mode", "    --safe-mode").replaceAll("\n", "\r\n"), knownEfforts],
    ["split effort alias", baseHelp.replace("  --effort", "  -E,\n  --effort"), knownEfforts],
    ["inline alias and explicit choices", baseHelp.replace(
      "  --effort <level> (low, medium, high, xhigh, max)",
      '  -E, --effort <level> (choices: "low", "medium", "high", "xhigh", "max", default: "medium")',
    ), knownEfforts],
    ["wrapped explicit choices", baseHelp.replace(
      "  --effort <level> (low, medium, high, xhigh, max)",
      '  --effort <level> (choices: "low", "medium",\n      "high", "xhigh", "max")',
    ), knownEfforts],
    ["enumeration before default", baseHelp.replace(
      "(low, medium, high, xhigh, max)",
      '(low, medium, high, xhigh, max) (default: "medium")',
    ), knownEfforts],
    ["future unknown level", baseHelp.replace("max)", "max, ultra)"), knownEfforts],
  ];
  for (const [name, help, expected] of accepted) {
    await t.test(name, () => {
      const capabilities = assertClaudeHelpCapabilities(help, version);
      assert.deepEqual(capabilities.effortLevels, expected);
      if (name === "captured 2.1.228 entry") assert.equal(capabilities.effortHelpEntry, captured);
    });
  }
});

test("Claude option grammar fails closed on drift, prose, and ambiguity", async (t) => {
  const rejected = [
    ["duplicate blocks", baseHelp.replace(
      "  --effort <level> (low, medium, high, xhigh, max)",
      "  --effort <level> (low, high)\n  --effort <level> (low, medium, high, xhigh, max)",
    ), /multiple --effort option blocks/u],
    ["conflicting choice groups", baseHelp.replace(
      "(low, medium, high, xhigh, max)",
      "(low, medium, high) (low, medium, xhigh, max)",
    ), /multiple conflicting review effort choice lists/u],
    ["unsupported casing", baseHelp.replace("low", "Low"), /unsupported casing/u],
    ["descriptive prose", baseHelp.replace(
      "(low, medium, high, xhigh, max)",
      "Effort profile (low, high) latency",
    ), /recognizable review effort choice list/u],
    ["comma prose", baseHelp.replace(
      "(low, medium, high, xhigh, max)",
      "(level for the session, see docs)",
    ), /recognizable review effort choice list/u],
    ["default without enumeration", baseHelp.replace(
      "(low, medium, high, xhigh, max)",
      "(medium)",
    ), /recognizable review effort choice list/u],
    ["missing required flag", baseHelp.replace(/^\s*--effort.*\n/m, ""), /required review flags: --effort/u],
    ["foreign safe-mode alias", baseHelp.replace("  --safe-mode", "  -s, --safe-mode"), /required review flags: --safe-mode/u],
    ["option name in wrapped prose", baseHelp.replace(
      '  --tools <tools...> Specify tools. Use "" to disable all tools.',
      "                                        unless --tools names them.",
    ), /required review flags: --tools/u],
  ];
  for (const [name, help, expected] of rejected) {
    await t.test(name, () => assert.throws(() => assertClaudeHelpCapabilities(help, version), expected));
  }
});

test("Claude safety flag semantics remain strict", () => {
  const capabilities = assertClaudeHelpCapabilities(baseHelp, version);
  assert.equal(capabilities.emptyToolsDisabled, true);
  assert.equal(capabilities.emptyToolsBasis, "captured-help-and-bounded-version-contract");
  assert.equal(capabilities.permissionMode, "dontAsk");
  assert.throws(
    () => assertClaudeHelpCapabilities(baseHelp.replace('Use "" to disable all tools.', "Use defaults."), version),
    /no-tools form/u,
  );
  assert.throws(
    () => assertClaudeHelpCapabilities(baseHelp.replace('"dontAsk", ', ""), version),
    /dontAsk permission mode/u,
  );
  assert.equal(
    assertClaudeAuthHelpCapabilities("Usage: claude auth status [options]\n  --json Output as JSON (default)\n").jsonOutput,
    true,
  );
  assert.throws(() => assertClaudeAuthHelpCapabilities("Usage: claude auth status\n"), /default JSON output/u);
});
