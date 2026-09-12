#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT = "968479A1AFF927E37D1A566BB5690EEEBB952194";
export const GITHUB_WEB_FLOW_SIGNING_KEY_FILE = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "github-web-flow-signing-key.asc",
);

export function inspectPinnedGitHubWebFlowKey(listing) {
  const fingerprintPattern = new RegExp(`^fpr:::::::::${GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT}:`, "mu");
  const ownerAction = `gpg --batch --import ${GITHUB_WEB_FLOW_SIGNING_KEY_FILE}`;
  return {
    ok: fingerprintPattern.test(listing),
    ownerAction,
  };
}

function main() {
  let listing = "";
  try {
    listing = execFileSync("gpg", ["--batch", "--with-colons", "--list-keys", GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch {}
  const result = inspectPinnedGitHubWebFlowKey(listing);
  if (result.ok) {
    process.stdout.write("GitHub web-flow signing key is available for local Update branch verification.\n");
    return;
  }
  process.stderr.write(
    `GitHub web-flow signing key ${GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT} is missing. Owner action: ${result.ownerAction}; then rerun ./bootstrap.sh --check before using Update branch.\n`,
  );
  process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
