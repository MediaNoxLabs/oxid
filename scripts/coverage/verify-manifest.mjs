// SPDX-License-Identifier: Apache-2.0
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export function verifyEnforcedManifest(manifest, sourceHead) {
  if (!/^[0-9a-f]{40}$/u.test(sourceHead)) throw new Error("expected source HEAD must be a commit SHA");
  if (manifest?.sourceHead !== sourceHead) throw new Error("coverage manifest source HEAD does not match checkout");
  if (manifest.mode !== "coverage") throw new Error("coverage manifest is not a real coverage run");
  if (manifest.evaluationMode !== "enforce") throw new Error("coverage manifest was not enforced");
  if (manifest.evaluation?.status !== "pass") throw new Error("coverage manifest verdict did not pass");
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const sourceHead = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const manifestPath = path.join(repoRoot, "target/coverage", sourceHead, "reports/manifest.json");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  verifyEnforcedManifest(manifest, sourceHead);
  process.stdout.write(`[coverage] enforced verdict verified for ${sourceHead}\n`);
}
