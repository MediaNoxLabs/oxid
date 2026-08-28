#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for required_command in pi node jq; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "missing Pi devshell command: $required_command" >&2
    exit 1
  fi
done

expected_pi_version="0.84.0"
pi_version="$(pi --version)"
if [[ "$pi_version" != "$expected_pi_version" ]]; then
  echo "unexpected Pi version: expected $expected_pi_version, found $pi_version" >&2
  echo "start Pi only through ./bootstrap.sh --pi" >&2
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

echo "Pi devshell smoke passed: pi $pi_version, agent-review-pi 0.5.0 extension and skill available."
