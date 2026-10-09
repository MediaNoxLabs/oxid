#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = path.join(root, "config", "midnight-integration-compatibility.json");

function fail(message) {
  throw new Error(`midnight integration compatibility: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function git(checkout, args) {
  return execFileSync("git", ["-C", checkout, ...args], { encoding: "utf8" }).trim();
}

function packageVersion(specification) {
  const fileMatch = /midnight-did(?:-[a-z-]+)?-(\d+\.\d+\.\d+)\.tgz$/u.exec(specification);
  return fileMatch?.[1] ?? specification;
}

export function validatePortalDidPackages(packageManifest, expectedRelease) {
  const didDependencies = Object.entries(packageManifest.dependencies ?? {})
    .filter(([name]) => name.startsWith("@midnight-ntwrk/midnight-did"));
  if (didDependencies.length !== 5) fail("Portal DID package set is incomplete");
  for (const [name, specification] of didDependencies) {
    if (packageVersion(specification) !== expectedRelease) fail(`${name} differs from the reviewed DID release`);
  }
}

export function auditCompatibility({ portalSource, environment = process.env } = {}) {
  const manifestBytes = readFileSync(manifestPath);
  const manifest = JSON.parse(manifestBytes);
  if (manifest.schema !== "oxid-midnight-integration-compatibility-v1") fail("unsupported manifest schema");
  if (!/^\d+\.\d+\.\d+$/u.test(manifest.contractRelease)) fail("invalid contract release");

  const cargo = readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const ledgerDependencies = cargo.split("\n")
    .filter((line) => line.includes('git = "https://github.com/MediaNoxLabs/midnight-ledger.git"'));
  if (ledgerDependencies.length === 0) fail("Midnight Ledger dependency set is empty");
  for (const line of ledgerDependencies) {
    const revision = /\brev = "([0-9a-f]{40})"/u.exec(line)?.[1];
    if (revision !== manifest.wallet.ledgerRevision) fail("ledger revision differs from the reviewed manifest");
  }
  const ledgerCrate = ledgerDependencies.find((line) => /^midnight-ledger\s*=/u.test(line));
  if (!ledgerCrate || !ledgerCrate.includes(`version = "=${manifest.wallet.ledgerVersion}"`)) {
    fail("ledger crate version does not match the reviewed manifest");
  }

  const flake = readFileSync(path.join(root, "flake.nix"), "utf8");
  if (!flake.includes(`midnight-did/${manifest.wallet.didToolchainRevision}`)) {
    fail("DID toolchain input does not match the reviewed manifest");
  }
  const flakeLock = JSON.parse(readFileSync(path.join(root, "flake.lock"), "utf8"));
  if (flakeLock.nodes?.["midnight-did-toolchain"]?.locked?.rev !== manifest.wallet.didToolchainRevision) {
    fail("DID toolchain lock differs from flake input");
  }
  if (manifest.wallet.didToolchainPackageVersion !== manifest.contractRelease) {
    fail("wallet DID package release differs from the reviewed contract release");
  }
  const nativeDid = manifest.wallet.nativeDidRuntime;
  if (!nativeDid || nativeDid.repository !== "https://github.com/MediaNoxLabs/midnight-identity.git") {
    fail("native DID runtime repository differs from the reviewed manifest");
  }
  const nativeDidDependencies = cargo.split("\n")
    .filter((line) => line.includes(`git = "${nativeDid.repository}"`));
  if (nativeDidDependencies.length !== 3) fail("native DID runtime dependency set is incomplete");
  for (const line of nativeDidDependencies) {
    if (!line.includes(`version = "=${nativeDid.packageVersion}"`)
      || !line.includes(`rev = "${nativeDid.revision}"`)) {
      fail("native DID runtime dependency differs from the reviewed manifest");
    }
  }
  const didArtifacts = readFileSync(path.join(root, "nix", "packages", "midnight-did-compact-artifacts.nix"), "utf8");
  for (const expected of [
    `version = "${nativeDid.artifactRelease}"`,
    `/v${nativeDid.artifactRelease}/midnight-did-zk-artifacts-${nativeDid.artifactRelease}.tar.gz`,
    `test "$(jq -r .gitSha "$out/manifest.json")" = ${nativeDid.artifactGitSha}`,
  ]) {
    if (!didArtifacts.includes(expected)) fail("native DID artifact release differs from the reviewed manifest");
  }

  const standalone = readFileSync(path.join(root, "scripts", "standalone-stack.yml"), "utf8");
  for (const [label, image] of [
    ["node image", manifest.standalone.nodeImage],
    ["indexer image", manifest.standalone.indexerImage],
    ["proof-server image", manifest.standalone.proofServerImage],
  ]) {
    if (!standalone.includes(image)) fail(`${label} does not match the reviewed manifest`);
  }
  if (environment.PROOF_SERVER_IMAGE && environment.PROOF_SERVER_IMAGE !== manifest.standalone.proofServerImage) {
    fail("proof-server image override differs from the reviewed manifest");
  }

  const lifecycle = readFileSync(path.join(root, "scripts", "portal-consumer-lifecycle.sh"), "utf8");
  for (const [label, expected] of [
    ["Portal commit", `PORTAL_COMMIT="${manifest.portal.commit}"`],
    ["Portal tree", `PORTAL_TREE="${manifest.portal.tree}"`],
    ["Portal repository", `PORTAL_REMOTE="${manifest.portal.repository}"`],
  ]) {
    if (!lifecycle.includes(expected)) fail(`${label} does not match the reviewed manifest`);
  }
  if (manifest.portal.didPackageVersion !== manifest.contractRelease) {
    fail("Portal DID package release differs from the reviewed contract release");
  }
  if (manifest.demoReadiness.outOfBandHolderPublicationAllowed !== false) {
    fail("out-of-band holder publication must remain forbidden");
  }

  if (portalSource) {
    if (git(portalSource, ["remote", "get-url", "origin"]) !== manifest.portal.repository) fail("Portal repository differs");
    if (git(portalSource, ["rev-parse", "HEAD"]) !== manifest.portal.commit) fail("Portal commit differs");
    if (git(portalSource, ["rev-parse", "HEAD^{tree}"]) !== manifest.portal.tree) fail("Portal tree differs");
    if (git(portalSource, ["status", "--porcelain", "--untracked-files=all"]) !== "") fail("Portal checkout is dirty");
    const packageBytes = execFileSync("git", [
      "-C", portalSource, "show", "HEAD:sidecar/did-manager-bridge/package.json",
    ]);
    if (sha256(packageBytes) !== manifest.portal.didManagerPackageManifestSha256) fail("Portal DID package manifest digest differs");
    const packageManifest = JSON.parse(packageBytes.toString("utf8"));
    validatePortalDidPackages(packageManifest, manifest.portal.didPackageVersion);
    const portalFlakeLock = JSON.parse(execFileSync("git", ["-C", portalSource, "show", "HEAD:flake.lock"], { encoding: "utf8" }));
    if (portalFlakeLock.nodes?.["midnight-did"]?.locked?.rev !== manifest.portal.didSourceRevision) {
      fail("Portal Midnight DID source revision differs");
    }
  }

  return {
    schema: "oxid-midnight-integration-compatibility-result-v1",
    state: "compatible",
    contractRelease: manifest.contractRelease,
    manifestSha256: sha256(manifestBytes),
    demoReady: manifest.demoReadiness.nativeHolderDidImplemented === true
      && manifest.demoReadiness.portalUnderstandsNativeDidRelease === true
      && manifest.demoReadiness.exactTailnetIssuanceQualified === true,
  };
}

function parsePortalSource(args) {
  if (args.length === 0) return undefined;
  if (args.length === 2 && args[0] === "--portal-source") return path.resolve(args[1]);
  fail("usage: check-midnight-integration-compatibility.mjs [--portal-source PATH]");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.stdout.write(`${JSON.stringify(auditCompatibility({ portalSource: parsePortalSource(process.argv.slice(2)) }))}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
