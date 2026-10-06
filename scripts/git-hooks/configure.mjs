#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  chmodSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
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
const MAX_PUBLISHED_BUNDLES = 64;
const MAX_STAGING_ATTEMPTS = 16;
const MAX_LOCK_QUARANTINES = 16;

function sourceBundleIdentity(repoRoot, sourceDir) {
  try {
    const hash = createHash("sha256");
    hash.update("oxid:git-hook-bundle:v2\0");
    for (const name of HOOK_NAMES) {
      hash.update(`hook\0${name}\0`);
      hash.update(readFileSync(path.join(sourceDir, name)));
      hash.update("\0");
    }
    for (const relative of BUNDLE_FILES) {
      hash.update(`policy\0${relative}\0`);
      hash.update(readFileSync(path.join(repoRoot, relative)));
      hash.update("\0");
    }
    return hash.digest("hex");
  } catch {
    return null;
  }
}

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
  const sourceDir = path.join(repoRoot, ".githooks");
  const hookRoot = path.join(commonDir, "oxid-factory", "hooks");
  const identity = sourceBundleIdentity(repoRoot, sourceDir);
  const bundlesDir = path.join(hookRoot, "bundles");
  const installedDir = path.join(bundlesDir, identity ?? "unavailable");
  return {
    repoRoot,
    commonDir,
    sourceDir,
    hookRoot,
    bundlesDir,
    stagingDir: path.join(hookRoot, "staging"),
    lockDir: path.join(hookRoot, "selection.lock"),
    identity,
    installedDir,
    bundleDir: path.join(installedDir, "policy-root"),
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

function isManagedSelectionPath(layout, selected) {
  return typeof selected === "string"
    && path.dirname(selected) === path.resolve(layout.bundlesDir)
    && /^[0-9a-f]{64}$/u.test(path.basename(selected));
}

function inspectBundleDirectory(layout, installedDir, { compareSource = true } = {}) {
  const bundleDir = path.join(installedDir, "policy-root");
  if (!isRealDirectory(installedDir) || !isRealDirectory(bundleDir)) {
    return { ok: false, structurallyValid: false, reason: "hook bundle directory or policy root is missing or symlinked" };
  }
  const dispatchers = HOOK_NAMES.map((name) => {
    const installed = path.join(installedDir, name);
    if (!isExecutableRegularFile(installed)) return { name, valid: false, reason: "missing, non-executable, or symlinked dispatcher" };
    const contents = readFileSync(installed, "utf8");
    if (!dispatcherIsBound(contents)) return { name, valid: false, reason: "dispatcher is not bound to policy-root" };
    return {
      name,
      valid: true,
      stale: compareSource && !hasSameContents(path.join(layout.sourceDir, name), installed),
    };
  });
  const invalid = dispatchers.find((dispatcher) => !dispatcher.valid);
  if (invalid) return { ok: false, structurallyValid: false, reason: `${invalid.name} is ${invalid.reason}`, dispatchers };
  const invalidBundle = BUNDLE_FILES.find((relative) => !isRegularFile(path.join(bundleDir, relative)));
  if (invalidBundle) {
    return { ok: false, structurallyValid: false, reason: `${invalidBundle} is missing, non-regular, or symlinked`, dispatchers };
  }
  const bundle = BUNDLE_FILES.map((relative) => ({
    relative,
    stale: compareSource && !hasSameContents(path.join(layout.repoRoot, relative), path.join(bundleDir, relative)),
  }));
  return {
    ok: !dispatchers.some((dispatcher) => dispatcher.stale) && !bundle.some((entry) => entry.stale),
    structurallyValid: true,
    reason: dispatchers.some((dispatcher) => dispatcher.stale) || bundle.some((entry) => entry.stale)
      ? "hook bundle content differs from this checkout"
      : null,
    dispatchers,
    bundle,
    bundleDir,
  };
}

function processIsLive(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code !== "ESRCH";
  }
}

function quarantine(candidate, suffix) {
  const quarantined = `${candidate}.${suffix}.${Date.now()}.${process.pid}`;
  renameSync(candidate, quarantined);
  return quarantined;
}

function countOwnedEntries(directory, pattern) {
  try {
    return readdirSync(directory).filter((entry) => pattern.test(entry)).length;
  } catch (error) {
    if (error.code === "ENOENT") return 0;
    throw error;
  }
}

function requireStoreCapacity(layout) {
  const published = countOwnedEntries(layout.bundlesDir, /^[0-9a-f]{64}$/u);
  const staging = countOwnedEntries(layout.stagingDir, /^[0-9a-f]{64}\.[0-9]+\.[0-9a-f-]+$/u);
  const quarantines = countOwnedEntries(layout.hookRoot, /^selection\.lock\.stale\.[0-9]+\.[0-9]+$/u);
  if (published >= MAX_PUBLISHED_BUNDLES && !isRealDirectory(layout.installedDir)) {
    throw new Error(`Git hook bundle store reached its ${MAX_PUBLISHED_BUNDLES}-bundle safety bound`);
  }
  if (staging >= MAX_STAGING_ATTEMPTS) {
    throw new Error(`Git hook staging store reached its ${MAX_STAGING_ATTEMPTS}-attempt safety bound`);
  }
  if (quarantines >= MAX_LOCK_QUARANTINES) {
    throw new Error(`Git hook lock quarantine reached its ${MAX_LOCK_QUARANTINES}-entry safety bound`);
  }
}

function acquireSelectionLock(layout, { timeoutMillis = 5_000 } = {}) {
  mkdirSync(layout.hookRoot, { recursive: true, mode: 0o700 });
  const token = randomUUID();
  const started = Date.now();
  while (true) {
    const candidate = `${layout.lockDir}.candidate.${process.pid}.${randomUUID()}`;
    mkdirSync(candidate, { mode: 0o700 });
    writeFileSync(
      path.join(candidate, "owner.json"),
      `${JSON.stringify({ schemaVersion: 1, pid: process.pid, token, startedAt: new Date().toISOString() })}\n`,
      { flag: "wx", mode: 0o600 },
    );
    try {
      renameSync(candidate, layout.lockDir);
      return { token, release() {
        try {
          const owner = JSON.parse(readFileSync(path.join(layout.lockDir, "owner.json"), "utf8"));
          if (owner.token !== token || owner.pid !== process.pid) return false;
          rmSync(layout.lockDir, { recursive: true });
          return true;
        } catch {
          return false;
        }
      } };
    } catch (error) {
      rmSync(candidate, { recursive: true, force: true });
      if (error.code !== "EEXIST" && error.code !== "ENOTEMPTY") throw error;
      let owner;
      try {
        owner = JSON.parse(readFileSync(path.join(layout.lockDir, "owner.json"), "utf8"));
      } catch {
        owner = null;
      }
      if (!owner || !processIsLive(owner.pid)) {
        try {
          quarantine(layout.lockDir, "stale");
          continue;
        } catch (quarantineError) {
          if (quarantineError.code === "ENOENT") continue;
          throw quarantineError;
        }
      }
      if (Date.now() - started >= timeoutMillis) {
        throw new Error(`Git hook selection lock is held by live process ${owner.pid}`);
      }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 25);
    }
  }
}

function publishBundle(layout) {
  mkdirSync(layout.bundlesDir, { recursive: true, mode: 0o700 });
  mkdirSync(layout.stagingDir, { recursive: true, mode: 0o700 });
  const staging = path.join(layout.stagingDir, `${layout.identity}.${process.pid}.${randomUUID()}`);
  mkdirSync(staging, { mode: 0o700 });
  try {
    for (const name of HOOK_NAMES) {
      const destination = path.join(staging, name);
      writeFileSync(destination, readFileSync(path.join(layout.sourceDir, name)), { flag: "wx", mode: 0o700 });
      chmodSync(destination, 0o700);
    }
    for (const relative of BUNDLE_FILES) {
      const destination = path.join(staging, "policy-root", relative);
      mkdirSync(path.dirname(destination), { recursive: true, mode: 0o700 });
      writeFileSync(destination, readFileSync(path.join(layout.repoRoot, relative)), {
        flag: "wx",
        mode: relative.endsWith(".mjs") ? 0o700 : 0o600,
      });
      chmodSync(destination, relative.endsWith(".mjs") ? 0o700 : 0o600);
    }
    const staged = inspectBundleDirectory(layout, staging);
    if (!staged.ok) throw new Error(`staged Git hook bundle is invalid: ${staged.reason}`);
    if (isRealDirectory(layout.installedDir)) {
      const existing = inspectBundleDirectory(layout, layout.installedDir);
      if (existing.ok) return;
      throw new Error("published Git hook bundle is corrupt; refusing to replace an actively selected immutable directory");
    }
    try {
      renameSync(staging, layout.installedDir);
    } catch (error) {
      if (error.code !== "EEXIST" && error.code !== "ENOTEMPTY") throw error;
      const winner = inspectBundleDirectory(layout, layout.installedDir);
      if (!winner.ok) throw new Error(`concurrently published Git hook bundle is invalid: ${winner.reason}`);
    }
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

/**
 * Inspect only the repository-owned Git-common bundle. A configured path that
 * merely has a similar name, is symlinked, or has an absent dispatcher is not
 * ours and must never become a refresh target.
 */
export function inspectManagedHookBundle(repository) {
  const layout = hookLayout(repository);
  const configured = config(repository, "core.hooksPath");
  if (!layout.identity) {
    return { ok: false, managed: false, configured, reason: "repository hook policy sources are unavailable", ...layout };
  }
  const selected = configuredHookPath(layout, configured);
  if (!selected || !isManagedSelectionPath(layout, selected)) {
    return { ok: false, managed: false, configured, reason: "core.hooksPath is not the canonical repository-managed hook directory", ...layout };
  }
  const inspected = inspectBundleDirectory(layout, selected);
  if (!inspected.structurallyValid) {
    return { ...layout, ...inspected, ok: false, managed: false, configured };
  }
  if (!inspected.ok || selected !== path.resolve(layout.installedDir)) {
    return {
      ...layout,
      ...inspected,
      ok: false,
      managed: true,
      stale: true,
      configured,
      reason: "canonical repository-managed hook bundle is stale; run the explicit bootstrap repair",
    };
  }
  return {
    ...layout,
    ...inspected,
    ok: true,
    managed: true,
    stale: false,
    configured,
    preMergeCommitRequired: false,
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
  if (!layout.identity) throw new Error("repository hook policy sources are unavailable");
  const identityErrors = requiredIdentity(repository);
  if (identityErrors.length) throw new Error(identityErrors.join("; "));
  const existing = config(repository, "core.hooksPath");
  const existingPath = configuredHookPath(layout, existing);
  const legacyPath = path.resolve(layout.hookRoot);
  if (existing && (!existingPath || (existingPath !== legacyPath && !isManagedSelectionPath(layout, existingPath)))) {
    throw new Error(`core.hooksPath already points to ${existing}; refusing to replace another hook manager`);
  }
  const lock = acquireSelectionLock(layout);
  let checked;
  try {
    requireStoreCapacity(layout);
    publishBundle(layout);
    git(repository, ["config", "--local", "core.hooksPath", layout.installedDir]);
    git(repository, ["config", "--local", "commit.gpgSign", "true"]);
    git(repository, ["config", "--local", "gpg.format", "openpgp"]);
    checked = checkGitHooks(repository);
    if (!checked.ok) throw new Error(checked.errors.join("; "));
  } finally {
    lock.release();
  }
  return checked;
}

function usage() {
  return [
    "Usage:",
    "  node scripts/git-hooks/configure.mjs check [--json]",
    "  node scripts/git-hooks/configure.mjs apply --execute [--json]",
    "",
    "Installation atomically selects one content-addressed bundle through",
    "repository-local Git configuration and <git-common-dir>/oxid-factory/hooks.",
    "It never modifies identity, keys,",
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
