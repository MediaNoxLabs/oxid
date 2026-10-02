#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { lstat, readFile, realpath, readdir } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const MAX_BOUNDED_LOG_LINES = 200;
const MAX_BOUNDED_LOG_BYTES = 256 * 1024;

function fail(message) {
  throw new Error(`invalid iOS Maestro evidence: ${message}`);
}

function assertPlainRelative(candidate, label) {
  if (typeof candidate !== "string" || candidate.length === 0 || path.isAbsolute(candidate)) {
    fail(`${label} must be a non-empty relative path`);
  }
  const normalized = path.posix.normalize(candidate);
  if (normalized !== candidate || normalized === ".." || normalized.startsWith("../")) {
    fail(`${label} escapes or is not normalized: ${candidate}`);
  }
  return normalized;
}

async function assertPublicFile(runRoot, relative, label) {
  const normalized = assertPlainRelative(relative, label);
  if (!normalized.startsWith("scenarios/")) fail(`${label} is outside scenarios/: ${relative}`);
  const root = await realpath(runRoot);
  const candidate = path.join(root, ...normalized.split("/"));
  let metadata;
  try {
    metadata = await lstat(candidate);
  } catch {
    fail(`${label} is dangling: ${relative}`);
  }
  if (!metadata.isFile() || metadata.isSymbolicLink()) fail(`${label} is not a regular file: ${relative}`);
  const resolved = await realpath(candidate);
  if (!resolved.startsWith(`${root}${path.sep}`)) fail(`${label} resolves outside the evidence root: ${relative}`);
  return { path: candidate, bytes: metadata.size };
}

export async function validateIosMaestroEvidence(runRoot) {
  const root = await realpath(runRoot);
  const receipt = JSON.parse(await readFile(path.join(root, "receipt.json"), "utf8"));
  if (receipt.schema !== "oxid-ios-maestro-evidence-v2") fail("unexpected receipt schema");
  if (receipt.artifacts?.manifest !== "scenarios/manifest.jsonl") fail("unexpected manifest path");
  if (receipt.cleanup?.receiptOwnedSimulator !== true
      || receipt.cleanup?.privateDiagnosticsRemoved !== true
      || receipt.cleanup?.rawArtifactsRemoved !== true) {
    fail("cleanup receipt is incomplete");
  }
  try {
    await lstat(path.join(root, "private"));
    fail("private diagnostics remain under the public evidence root");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  const manifestFile = await assertPublicFile(root, receipt.artifacts.manifest, "receipt manifest");
  const manifestText = await readFile(manifestFile.path, "utf8");
  const entries = manifestText.split("\n").filter(Boolean).map((line, index) => {
    try {
      return JSON.parse(line);
    } catch {
      fail(`manifest line ${index + 1} is not JSON`);
    }
  });
  const seen = new Set();
  let publicBytes = manifestFile.bytes;
  let screenshots = 0;
  for (const [index, entry] of entries.entries()) {
    if (typeof entry?.scenario !== "string" || !entry.scenario) fail(`manifest line ${index + 1} has no scenario`);
    if (!new Set(["screenshot", "bounded-log"]).has(entry.kind)) fail(`manifest line ${index + 1} has an invalid kind`);
    if (seen.has(entry.artifact)) fail(`manifest reuses artifact path: ${entry.artifact}`);
    seen.add(entry.artifact);
    const artifact = await assertPublicFile(root, entry.artifact, `manifest line ${index + 1} artifact`);
    publicBytes += artifact.bytes;
    if (entry.kind === "screenshot") screenshots += 1;
    if (entry.kind === "bounded-log") {
      if (artifact.bytes > MAX_BOUNDED_LOG_BYTES) fail(`bounded log exceeds ${MAX_BOUNDED_LOG_BYTES} bytes`);
      const lines = (await readFile(artifact.path, "utf8")).split("\n");
      if (lines.at(-1) === "") lines.pop();
      if (lines.length > MAX_BOUNDED_LOG_LINES) fail(`bounded log exceeds ${MAX_BOUNDED_LOG_LINES} lines`);
    }
  }
  if (receipt.artifacts.screenshotCount !== screenshots) fail("screenshot count does not match the manifest");
  if (receipt.artifacts.publicBytes !== publicBytes) fail("public byte count does not match retained artifacts");

  const scenarioIds = receipt.outcome?.scenarios?.map((entry) => entry.id) ?? [];
  if (scenarioIds.length !== new Set(scenarioIds).size) fail("scenario outcomes contain duplicate identifiers");
  const unexpected = (await readdir(root)).filter((name) => !["receipt.json", "scenarios"].includes(name));
  if (unexpected.length > 0) fail(`unexpected public-root entries: ${unexpected.join(", ")}`);
  return { artifacts: entries.length, screenshots, publicBytes };
}

async function main() {
  const [runRoot] = process.argv.slice(2);
  if (!runRoot) throw new Error("Usage: validate-ios-maestro-evidence.mjs RUN_ROOT");
  const result = await validateIosMaestroEvidence(runRoot);
  process.stdout.write(`${JSON.stringify({ ok: true, ...result })}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
