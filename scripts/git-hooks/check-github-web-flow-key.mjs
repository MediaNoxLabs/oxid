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

export const GITHUB_WEB_FLOW_SIGNING_KEY_OWNER_ACTION =
  "gpg --batch --import scripts/git-hooks/github-web-flow-signing-key.asc";

export function inspectPinnedGitHubWebFlowKey(listing) {
  const fingerprints = [...listing.matchAll(/^fpr:::::::::([A-F0-9]{40}):/gmu)].map((match) => match[1]);
  return {
    ok: fingerprints.length === 1 && fingerprints[0] === GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT,
    fingerprints,
    ownerAction: GITHUB_WEB_FLOW_SIGNING_KEY_OWNER_ACTION,
  };
}

function inspectKeyWithGpg(args) {
  try {
    return execFileSync("gpg", ["--batch", "--no-options", "--no-auto-key-retrieve", "--no-auto-key-locate", "--with-colons", ...args], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch {
    return "";
  }
}

export function inspectPinnedGitHubWebFlowKeyFile(keyFile = GITHUB_WEB_FLOW_SIGNING_KEY_FILE) {
  return inspectPinnedGitHubWebFlowKey(inspectKeyWithGpg(["--show-keys", keyFile]));
}

export function hasGitHubWebFlowSigningKey() {
  return inspectPinnedGitHubWebFlowKey(
    inspectKeyWithGpg(["--list-keys", GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT]),
  ).ok;
}

function main() {
  const pinnedKey = inspectPinnedGitHubWebFlowKeyFile();
  if (!pinnedKey.ok) {
    process.stderr.write(
      `Pinned GitHub web-flow signing key failed offline validation: expected exactly fingerprint ${GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT}. Refusing to use ${GITHUB_WEB_FLOW_SIGNING_KEY_FILE}.\n`,
    );
    process.exitCode = 1;
    return;
  }
  if (hasGitHubWebFlowSigningKey()) {
    process.stdout.write("GitHub web-flow signing key is available for local Update branch verification.\n");
    return;
  }
  process.stderr.write(
    `GitHub web-flow signing key ${GITHUB_WEB_FLOW_SIGNING_KEY_FINGERPRINT} is absent from the default keyring. Local Update branch verification remains fail-closed. Owner action (not run): ${pinnedKey.ownerAction}. Rebase remains the no-bypass recovery path.\n`,
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
