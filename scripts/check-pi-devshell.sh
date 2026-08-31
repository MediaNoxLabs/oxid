#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for required_command in pi node jq realpath awk; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "missing Pi devshell command: $required_command" >&2
    exit 1
  fi
done

pi_executable="$(realpath "$(command -v pi)")"
if [[ "$pi_executable" != /nix/store/*/bin/pi ]]; then
  echo "Pi is not supplied by the pinned Nix development shell: $pi_executable" >&2
  echo "run this check through ./bootstrap.sh --check" >&2
  exit 1
fi

pi_version="$(pi --version)"
if [[ ! "$pi_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  echo "unexpected Pi version output: $pi_version" >&2
  echo "start Pi only through ./bootstrap.sh --pi" >&2
  exit 1
fi

model_policy="$(node --input-type=module <<'NODE'
import { readFile } from "node:fs/promises";

const settings = JSON.parse(await readFile(".pi/settings.json", "utf8"));
if (!/^[a-z0-9-]+$/u.test(settings.defaultProvider ?? "") || !/^[a-z0-9.-]+$/u.test(settings.defaultModel ?? "")) {
  throw new Error("tracked defaultProvider/defaultModel is malformed");
}
if (settings.subagents?.defaultModel !== `${settings.defaultProvider}/${settings.defaultModel}`) {
  throw new Error("parent and subagent default models are not aligned");
}
process.stdout.write(`${settings.defaultProvider}\t${settings.defaultModel}`);
NODE
)"
IFS=$'\t' read -r expected_provider expected_model <<< "$model_policy"
if ! model_catalog="$(pi --list-models "$expected_provider/$expected_model")"; then
  echo "Pi model catalog query failed for $expected_provider/$expected_model" >&2
  exit 1
fi
if ! awk -v provider="$expected_provider" -v model="$expected_model" \
  'NR > 1 && $1 == provider && $2 == model { found = 1 } END { exit !found }' <<< "$model_catalog"; then
  echo "tracked Pi model is absent from the Nix-pinned catalog: $expected_provider/$expected_model" >&2
  exit 1
fi

review_package_root="$(node --input-type=module <<'NODE'
import { resolveDevLoopsPackageRoot } from "./scripts/lib/dev-loop-runtime.mjs";

const expectedName = "@input-output-hk/agent-review-pi";
const resolved = await resolveDevLoopsPackageRoot({
  cwd: process.cwd(),
  includeAllPinnedPackages: true,
});
const reviewPackage = resolved.packageRoots.find(({ name }) => name === expectedName);
if (!reviewPackage) {
  throw new Error(`project Pi settings do not pin ${expectedName}`);
}
process.stdout.write(reviewPackage.packageRoot);
NODE
)"

subagent_package_root="$(node --input-type=module <<'NODE'
import { resolveDevLoopsPackageRoot } from "./scripts/lib/dev-loop-runtime.mjs";

const expectedName = "pi-subagents";
const resolved = await resolveDevLoopsPackageRoot({ cwd: process.cwd(), includeAllPinnedPackages: true });
const subagents = resolved.packageRoots.find(({ name }) => name === expectedName);
if (!subagents) throw new Error(`project Pi settings do not pin ${expectedName}`);
process.stdout.write(subagents.packageRoot);
NODE
)"

node --input-type=module - "$subagent_package_root" <<'NODE'
import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.argv[2];
const [types, agents, turnBudget] = await Promise.all([
  readFile(path.join(root, "src", "shared", "types.ts"), "utf8"),
  readFile(path.join(root, "src", "agents", "agents.ts"), "utf8"),
  readFile(path.join(root, "src", "runs", "shared", "turn-budget.ts"), "utf8"),
]);
for (const field of [
  "asyncByDefault", "forceTopLevelAsync", "maxSubagentDepth",
  "maxSubagentSpawnsPerSession", "glo" + "balConcurrencyLimit", "turnBudget",
  "usageBudget", "parallel", "chain", "dynamicFanout", "maxItems", "artifactDir",
]) {
  if (!types.includes(`${field}?`)) throw new Error(`pi-subagents schema does not declare ${field}`);
}
for (const field of ["frontmatter.timeoutMs", "frontmatter.turnBudget", "frontmatter.maxSubagentDepth"]) {
  if (!agents.includes(field)) throw new Error(`pi-subagents agent parser does not consume ${field}`);
}
for (const field of ["maxTurns", "graceTurns"]) {
  if (!turnBudget.includes(field)) throw new Error(`pi-subagents turn budget does not consume ${field}`);
}
NODE

review_package_json="$review_package_root/package.json"
if [[ ! -f "$review_package_json" ]]; then
  echo "missing exact project @input-output-hk/agent-review-pi@0.5.0" >&2
  echo "enter nix develop with a GitHub token that can read packages" >&2
  exit 1
fi

node - "$review_package_json" <<'NODE'
const fs = require("node:fs");

const packagePath = process.argv[2];
const manifest = JSON.parse(fs.readFileSync(packagePath, "utf8"));
const expected = {
  name: "@input-output-hk/agent-review-pi",
  version: "0.5.0",
  extension: "./dist/extension.js",
  skill: "./skills",
};

if (manifest.name !== expected.name || manifest.version !== expected.version) {
  throw new Error(`unexpected review package ${manifest.name}@${manifest.version}`);
}
if (!manifest.pi?.extensions?.includes(expected.extension)) {
  throw new Error(`review package does not declare ${expected.extension}`);
}
if (!manifest.pi?.skills?.includes(expected.skill)) {
  throw new Error(`review package does not declare ${expected.skill}`);
}
NODE

node --input-type=module - "$review_package_root/dist/extension.js" <<'NODE'
import { pathToFileURL } from "node:url";

const extensionPath = process.argv[2];
const extension = await import(pathToFileURL(extensionPath).href);
const registered = [];
extension.registerTools({
  registerTool(tool) {
    registered.push(tool.name);
  },
});

const expected = [
  "labels_bootstrap",
  "pr_approve_dep_upgrade",
  "pr_expedite",
  "pr_request_review",
  "pr_stabilize",
  "pr_watch",
  "review_claim",
  "review_complete",
  "review_create",
  "review_enrich",
  "review_list",
].sort();
registered.sort();

if (JSON.stringify(registered) !== JSON.stringify(expected)) {
  throw new Error(`unexpected review tools: ${registered.join(",")}`);
}
NODE

pi_rpc_output="$({
  printf '%s\n' '{"type":"get_commands"}'
} | pi --approve --offline --mode rpc --no-session)"

loader_path="$repo_root/.pi/skills/agent-review/SKILL.md"
if ! jq -s -e --arg loader_path "$loader_path" '
  map(select(.type == "response" and .command == "get_commands"))[0]
  | .data.commands
  | any(
      .name == "skill:agent-review"
      and .source == "skill"
      and .sourceInfo.path == $loader_path
    )
' <<<"$pi_rpc_output" >/dev/null; then
  echo "Pi did not expose the tracked agent-review compatibility skill" >&2
  exit 1
fi

echo "Pi devshell smoke passed: pi $pi_version, $expected_provider/$expected_model, agent-review-pi 0.5.0 extension and skill available."
