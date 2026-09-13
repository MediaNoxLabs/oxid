#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import {
  chmodSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { inspectSigningConfiguration } from "./local-policy.mjs";

export const HOOK_NAMES = Object.freeze(["pre-commit", "commit-msg", "pre-push"]);
export const BUNDLE_FILES = Object.freeze([
  "scripts/git-hooks/local-policy.mjs",
  "scripts/ci/contribution-policy.mjs",
  "scripts/lib/delivery-target.mjs",
  ".github/contribution-policy.json",
]);

function git(repository, args) {
  return execFileSync("git", args, {
    cwd: repository,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function config(repository, key) {
  try {
    return git(repository, ["config", "--get", key]);
  } catch {
    return "";
  }
}

export function hookLayout(repository) {
  const repoRoot = git(repository, ["rev-parse", "--show-toplevel"]);
  const commonDir = git(repository, ["rev-parse", "--path-format=absolute", "--git-common-dir"]);
  return {
    repoRoot,
    commonDir,
    sourceDir: path.join(repoRoot, ".githooks"),
    installedDir: path.join(commonDir, "oxid-factory", "hooks"),
    bundleDir: path.join(commonDir, "oxid-factory", "hooks", "policy-root"),
  };
}

function configuredHookPath(layout, configured) {
  if (!configured) return null;
  return path.resolve(layout.commonDir, configured);
}

function isRealDirectory(candidate) {
  try {
    return lstatSync(candidate).isDirectory() && realpathSync(candidate) === path.resolve(candidate);
  } catch {
    return false;
  }
}

function isExecutableRegularFile(candidate) {
  try {
    const metadata = lstatSync(candidate);
    return metadata.isFile() && !metadata.isSymbolicLink() && (metadata.mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

function isRegularFile(candidate) {
  try {
    const metadata = lstatSync(candidate);
    return metadata.isFile() && !metadata.isSymbolicLink();
  } catch {
    return false;
  }
}

function hasSameContents(left, right) {
  try {
    return readFileSync(left).equals(readFileSync(right));
  } catch {
    return false;
  }
}

function dispatcherIsBound(contents) {
  return /hook_dir=.*dirname/u.test(contents)
    && /exec node "\$hook_dir\/policy-root\/scripts\/git-hooks\/local-policy\.mjs"/u.test(contents);
}

/**
 * Inspect only the repository-owned Git-common bundle. A configured path that
 * merely has a similar name, is symlinked, or has an absent dispatcher is not
 * ours and must never become a refresh target.
 */
export function inspectManagedHookBundle(repository) {
  const layout = hookLayout(repository);
  const configured = config(repository, "core.hooksPath");
  if (configuredHookPath(layout, configured) !== path.resolve(layout.installedDir)) {
    return { ok: false, managed: false, configured, reason: "core.hooksPath is not the canonical repository-managed hook directory", ...layout };
  }
  if (!isRealDirectory(layout.installedDir) || !isRealDirectory(layout.bundleDir)) {
    return { ok: false, managed: false, configured, reason: "canonical hook directory or policy root is missing or symlinked", ...layout };
  }
  const dispatchers = HOOK_NAMES.map((name) => {
    const installed = path.join(layout.installedDir, name);
    if (!isExecutableRegularFile(installed)) return { name, valid: false, reason: "missing, non-executable, or symlinked dispatcher" };
    const contents = readFileSync(installed, "utf8");
    if (!dispatcherIsBound(contents)) return { name, valid: false, reason: "dispatcher is not bound to policy-root" };
    return { name, valid: true, stale: !hasSameContents(path.join(layout.sourceDir, name), installed) };
  });
  const invalid = dispatchers.find((dispatcher) => !dispatcher.valid);
  if (invalid) return { ok: false, managed: false, configured, reason: `${invalid.name} is ${invalid.reason}`, dispatchers, ...layout };
  const invalidBundle = BUNDLE_FILES.find((relative) => !isRegularFile(path.join(layout.bundleDir, relative)));
  if (invalidBundle) {
    return { ok: false, managed: false, configured, reason: `${invalidBundle} is missing, non-regular, or symlinked`, dispatchers, ...layout };
  }
  const bundle = BUNDLE_FILES.map((relative) => ({
    relative,
    stale: !hasSameContents(path.join(layout.repoRoot, relative), path.join(layout.bundleDir, relative)),
  }));
  if (dispatchers.some((dispatcher) => dispatcher.stale) || bundle.some((entry) => entry.stale)) {
    return {
      ok: false,
      managed: true,
      stale: true,
      configured,
      reason: "canonical repository-managed hook bundle is stale; run the explicit bootstrap repair",
      dispatchers,
      bundle,
      ...layout,
    };
  }
  return {
    ok: true,
    managed: true,
    stale: false,
    configured,
    dispatchers,
    bundle,
    preMergeCommitRequired: false,
    ...layout,
  };
}

function requiredIdentity(repository) {
  const errors = [];
  for (const key of ["user.name", "user.email", "user.signingkey"]) {
    if (!config(repository, key)) errors.push(`${key} must be configured before installing Oxid hooks`);
  }
  return errors;
}

export function checkGitHooks(repository) {
  const layout = hookLayout(repository);
  const errors = [];
  if (configuredHookPath(layout, config(repository, "core.hooksPath")) !== path.resolve(layout.installedDir)) {
    errors.push(`core.hooksPath must be ${layout.installedDir}`);
  }
  for (const name of HOOK_NAMES) {
    const source = path.join(layout.sourceDir, name);
    const installed = path.join(layout.installedDir, name);
    try {
      if (!readFileSync(source).equals(readFileSync(installed))) errors.push(`${name} installation is stale`);
      if ((statSync(installed).mode & 0o111) === 0) errors.push(`${name} installation is not executable`);
    } catch (error) {
      errors.push(`${name} installation is unavailable: ${error.message}`);
    }
  }
  for (const relative of BUNDLE_FILES) {
    const source = path.join(layout.repoRoot, relative);
    const installed = path.join(layout.bundleDir, relative);
    try {
      if (!readFileSync(source).equals(readFileSync(installed))) errors.push(`${relative} installation is stale`);
    } catch (error) {
      errors.push(`${relative} installation is unavailable: ${error.message}`);
    }
  }
  errors.push(...inspectSigningConfiguration(repository).errors);
  return { ok: errors.length === 0, errors, ...layout };
}

export function applyGitHooks(repository, { execute = false } = {}) {
  if (!execute) throw new Error("Refusing to modify repository-local Git configuration without --execute");
  const layout = hookLayout(repository);
  const identityErrors = requiredIdentity(repository);
  if (identityErrors.length) throw new Error(identityErrors.join("; "));
  const existing = config(repository, "core.hooksPath");
  if (existing && configuredHookPath(layout, existing) !== path.resolve(layout.installedDir)) {
    throw new Error(`core.hooksPath already points to ${existing}; refusing to replace another hook manager`);
  }
  mkdirSync(layout.installedDir, { recursive: true, mode: 0o700 });
  for (const name of HOOK_NAMES) {
    const contents = readFileSync(path.join(layout.sourceDir, name));
    const temporary = path.join(layout.installedDir, `.${name}.${process.pid}.tmp`);
    writeFileSync(temporary, contents, { flag: "wx", mode: 0o700 });
    renameSync(temporary, path.join(layout.installedDir, name));
    chmodSync(path.join(layout.installedDir, name), 0o700);
  }
  for (const relative of BUNDLE_FILES) {
    const destination = path.join(layout.bundleDir, relative);
    mkdirSync(path.dirname(destination), { recursive: true, mode: 0o700 });
    const contents = readFileSync(path.join(layout.repoRoot, relative));
    const temporary = `${destination}.${process.pid}.tmp`;
    writeFileSync(temporary, contents, { flag: "wx", mode: 0o600 });
    renameSync(temporary, destination);
    chmodSync(destination, relative.endsWith(".mjs") ? 0o700 : 0o600);
  }
  git(repository, ["config", "--local", "core.hooksPath", layout.installedDir]);
  git(repository, ["config", "--local", "commit.gpgSign", "true"]);
  git(repository, ["config", "--local", "gpg.format", "openpgp"]);
  const checked = checkGitHooks(repository);
  if (!checked.ok) throw new Error(checked.errors.join("; "));
  return checked;
}

function usage() {
  return [
    "Usage:",
    "  node scripts/git-hooks/configure.mjs check [--json]",
    "  node scripts/git-hooks/configure.mjs apply --execute [--json]",
    "",
    "Installation writes only repository-local Git configuration and",
    "<git-common-dir>/oxid-factory/hooks. It never modifies identity, keys,",
    "credentials, global Git configuration, or GitHub state.",
  ].join("\n");
}

function report(result, json) {
  if (json) process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  else if (result.ok) process.stdout.write(`Local Git contribution hooks are aligned: ${result.installedDir}\n`);
  else for (const problem of result.errors) process.stderr.write(`[git-hooks] ${problem}\n`);
}

function main(argv = process.argv.slice(2)) {
  const command = argv[0];
  const json = argv.includes("--json");
  if (command === "check") {
    const result = checkGitHooks(process.cwd());
    report(result, json);
    return result.ok ? 0 : 1;
  }
  if (command === "apply") {
    const result = applyGitHooks(process.cwd(), { execute: argv.includes("--execute") });
    report(result, json);
    return 0;
  }
  process.stderr.write(`${usage()}\n`);
  return 2;
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (error) {
    process.stderr.write(`[git-hooks] ${error.message}\n`);
    process.exitCode = 2;
  }
}
