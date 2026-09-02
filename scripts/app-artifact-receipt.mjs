#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function fail(message) {
  throw new Error(`app artifact receipt: ${message}`);
}

function git(...args) {
  return execFileSync("git", ["-C", repositoryRoot, ...args], { encoding: "utf8" }).trim();
}

function gitBuffer(...args) {
  return execFileSync("git", ["-C", repositoryRoot, ...args], { maxBuffer: 256 * 1024 * 1024 });
}

function hashEntry(hash, absolutePath, relativePath) {
  const metadata = lstatSync(absolutePath);
  const normalized = relativePath.split(path.sep).join("/");
  if (metadata.isSymbolicLink()) {
    hash.update(`link\0${normalized}\0${readlinkSync(absolutePath)}\0`);
    return;
  }
  if (metadata.isDirectory()) {
    hash.update(`directory\0${normalized}\0`);
    for (const child of readdirSync(absolutePath).sort()) {
      hashEntry(hash, path.join(absolutePath, child), path.join(relativePath, child));
    }
    return;
  }
  if (!metadata.isFile()) fail(`artifact contains unsupported entry ${absolutePath}`);
  hash.update(`file\0${normalized}\0${metadata.mode & 0o111}\0${metadata.size}\0`);
  hash.update(readFileSync(absolutePath));
  hash.update("\0");
}

export function artifactSha256(artifactPath) {
  const absolutePath = path.resolve(artifactPath);
  const hash = createHash("sha256");
  hashEntry(hash, absolutePath, path.basename(absolutePath));
  return hash.digest("hex");
}

export function sourceSha256() {
  const hash = createHash("sha256");
  hash.update("tracked-diff\0");
  hash.update(gitBuffer("diff", "--binary", "--no-ext-diff", "HEAD", "--"));
  hash.update("\0untracked\0");
  const untracked = gitBuffer("ls-files", "--others", "--exclude-standard", "-z")
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .sort();
  for (const relativePath of untracked) {
    const absolutePath = path.resolve(repositoryRoot, relativePath);
    if (absolutePath !== repositoryRoot && !absolutePath.startsWith(`${repositoryRoot}${path.sep}`)) {
      fail("untracked source path escapes the repository");
    }
    hashEntry(hash, absolutePath, relativePath);
  }
  return hash.digest("hex");
}

function parseArguments(argv) {
  const [operation, ...tokens] = argv;
  if (!new Set(["write", "verify"]).has(operation)) fail("operation must be write or verify");
  const values = { operation };
  for (let index = 0; index < tokens.length; index += 2) {
    const flag = tokens[index];
    const value = tokens[index + 1];
    if (!flag?.startsWith("--") || value === undefined) fail("arguments must be --name value pairs");
    const name = flag.slice(2);
    if (!new Set(["platform", "artifact", "target", "configuration", "receipt"]).has(name)) {
      fail(`unsupported argument ${flag}`);
    }
    if (values[name] !== undefined) fail(`duplicate argument ${flag}`);
    values[name] = value;
  }
  for (const name of ["platform", "artifact", "target", "configuration", "receipt"]) {
    if (!values[name]) fail(`--${name} is required`);
  }
  if (!new Set(["android", "ios-simulator"]).has(values.platform)) {
    fail("platform must be android or ios-simulator");
  }
  values.artifact = path.resolve(values.artifact);
  values.receipt = path.resolve(values.receipt);
  return values;
}

function expectedReceipt(options) {
  return {
    schema: "oxid-app-artifact-receipt-v1",
    repository: "MediaNoxLabs/oxid",
    head: git("rev-parse", "HEAD"),
    tree: git("rev-parse", "HEAD^{tree}"),
    sourceSha256: sourceSha256(),
    platform: options.platform,
    target: options.target,
    configuration: options.configuration,
    artifact: options.artifact,
    artifactSha256: artifactSha256(options.artifact),
  };
}

function writeReceipt(options) {
  const receipt = expectedReceipt(options);
  mkdirSync(path.dirname(options.receipt), { recursive: true, mode: 0o700 });
  const candidate = `${options.receipt}.tmp-${process.pid}`;
  try {
    writeFileSync(candidate, `${JSON.stringify(receipt, null, 2)}\n`, { mode: 0o600, flag: "wx" });
    chmodSync(candidate, 0o600);
    renameSync(candidate, options.receipt);
  } finally {
    rmSync(candidate, { force: true });
  }
  return receipt;
}

function verifyReceipt(options) {
  const metadata = lstatSync(options.receipt);
  if (!metadata.isFile() || metadata.isSymbolicLink() || (metadata.mode & 0o777) !== 0o600) {
    fail("receipt must be a private regular non-symlink file");
  }
  let actual;
  try {
    actual = JSON.parse(readFileSync(options.receipt, "utf8"));
  } catch {
    fail("receipt is not valid JSON");
  }
  const expected = expectedReceipt(options);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail("receipt does not match the current source, configuration, or artifact");
  }
  return actual;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArguments(argv);
  const receipt = options.operation === "write" ? writeReceipt(options) : verifyReceipt(options);
  process.stdout.write(`${options.operation === "write" ? "wrote" : "verified"} ${options.platform} artifact ${receipt.artifactSha256}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
