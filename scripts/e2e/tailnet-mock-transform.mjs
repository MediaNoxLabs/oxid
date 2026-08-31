#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const PORTAL_COMMIT = "22ae5369b6f939e6b20648f4b85dd993527748ef";
const PORTAL_TREE = "74d8d1a5b87c160ea554006e47d5f3edc3cd3e10";
const PINNED_MOCK_SHA256 = "68613b89b01ca53d1d9c33f6c14393ac310459aded26b2d337c05bd2f95113c6";
const MOCK_FILE = "didit-tailnet.yml";
const RECEIPT_FILE = "didit-tailnet-receipt.json";
const MAX_MOCK_BYTES = 1024 * 1024;

const MOCK_VERIFICATION = "http://localhost:9090/mock-verification";
const PENDING_CALLBACK = "http://localhost:8090/issue/pending.html";

function fail(message) {
  throw new Error(message);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function occurrenceCount(value, needle) {
  return value.split(needle).length - 1;
}

function exactPrivateDirectory(directory) {
  const metadata = fs.lstatSync(directory);
  return metadata.isDirectory() && !metadata.isSymbolicLink()
    && (metadata.mode & 0o777) === 0o700;
}

function exactPrivateRegularFile(file) {
  const metadata = fs.lstatSync(file);
  return metadata.isFile() && !metadata.isSymbolicLink()
    && (metadata.mode & 0o777) === 0o600;
}

function navigationUrls(value) {
  const urls = [];
  for (const match of value.matchAll(/"(?:url|session_url)"\s*:\s*"([^"]+)"/gu)) {
    urls.push(match[1]);
  }
  for (const match of value.matchAll(/window\.location\.href\s*=\s*'([^']+)'/gu)) {
    urls.push(match[1]);
  }
  return urls;
}

/**
 * Rewrites the three browser-facing values in the immutable Lace mock. The
 * pinned source check belongs to `createTransformedMock`; this pure operation
 * is independently testable against the exact occurrence contract.
 */
export function transformBrowserUrls(source, origin) {
  if (typeof source !== "string" || !exactPublicOrigin(origin)) fail("invalid input");
  if (occurrenceCount(source, MOCK_VERIFICATION) !== 2
      || occurrenceCount(source, PENDING_CALLBACK) !== 1) {
    fail("pinned browser URL occurrences drifted");
  }
  const transformed = source
    .replaceAll(MOCK_VERIFICATION, `${origin}/mock-verification`)
    .replace(PENDING_CALLBACK, `${origin}/issue/pending.html`);
  validateBrowserUrls(transformed, origin);
  return transformed;
}

/** Reject browser navigation that is not the exact one authenticated origin. */
export function validateBrowserUrls(transformed, origin) {
  if (typeof transformed !== "string" || !exactPublicOrigin(origin)) fail("invalid input");
  const expected = [
    `${origin}/mock-verification`,
    `${origin}/mock-verification`,
    `${origin}/issue/pending.html`,
  ].sort();
  const actual = navigationUrls(transformed).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) fail("browser navigation drifted");
  for (const value of actual) {
    let parsed;
    try {
      parsed = new URL(value);
    } catch {
      fail("invalid browser navigation");
    }
    if (parsed.origin !== origin || parsed.protocol !== "https:"
        || parsed.username !== "" || parsed.password !== ""
        || parsed.search !== "" || parsed.hash !== "") {
      fail("unsafe browser navigation");
    }
  }
}

function checkedCommand(directory, args) {
  return execFileSync("git", ["-C", directory, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
}

function checkedPinnedSource(sourceDirectory) {
  if (!path.isAbsolute(sourceDirectory) || !exactPrivateDirectory(sourceDirectory)) {
    fail("invalid source directory");
  }
  if (checkedCommand(sourceDirectory, ["rev-parse", "HEAD"]) !== PORTAL_COMMIT
      || checkedCommand(sourceDirectory, ["rev-parse", "HEAD^{tree}"]) !== PORTAL_TREE
      || checkedCommand(sourceDirectory, ["status", "--porcelain", "--untracked-files=all"]) !== "") {
    fail("pinned source drifted");
  }
  const sourceMock = path.join(sourceDirectory, "mock", "didit.yml");
  const metadata = fs.lstatSync(sourceMock);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size <= 0
      || metadata.size > MAX_MOCK_BYTES) fail("invalid pinned mock");
  const bytes = fs.readFileSync(sourceMock);
  if (sha256(bytes) !== PINNED_MOCK_SHA256) fail("pinned mock digest drifted");
  return { bytes, sourceMock };
}

function statePaths(stateDirectory) {
  if (!path.isAbsolute(stateDirectory) || !exactPrivateDirectory(stateDirectory)) {
    fail("invalid private state directory");
  }
  return {
    mock: path.join(stateDirectory, MOCK_FILE),
    receipt: path.join(stateDirectory, RECEIPT_FILE),
  };
}

function readReceipt(receiptPath) {
  if (!exactPrivateRegularFile(receiptPath)) fail("invalid mock receipt");
  const bytes = fs.readFileSync(receiptPath);
  if (bytes.length === 0 || bytes.length > 16 * 1024) fail("invalid mock receipt");
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    fail("invalid mock receipt");
  }
}

/** Creates one private, receipt-bound transformed copy without touching the pin. */
export function createTransformedMock(sourceDirectory, stateDirectory, origin) {
  if (!exactPublicOrigin(origin)) fail("invalid Tailnet origin");
  const { bytes } = checkedPinnedSource(sourceDirectory);
  const { mock, receipt } = statePaths(stateDirectory);
  if (fs.existsSync(mock) || fs.existsSync(receipt)) fail("stale mock state");

  const transformed = Buffer.from(transformBrowserUrls(bytes.toString("utf8"), origin), "utf8");
  if (transformed.length === 0 || transformed.length > MAX_MOCK_BYTES) fail("invalid transformed mock");
  const transformedSha256 = sha256(transformed);
  const receiptValue = {
    schema: "oxid-tailnet-didit-mock-v1",
    source: { commit: PORTAL_COMMIT, tree: PORTAL_TREE, mockSha256: PINNED_MOCK_SHA256 },
    transformed: { bytes: transformed.length, sha256: transformedSha256 },
    browserOrigin: origin,
  };

  fs.writeFileSync(mock, transformed, { flag: "wx", mode: 0o600 });
  fs.chmodSync(mock, 0o600);
  fs.writeFileSync(receipt, `${JSON.stringify(receiptValue)}\n`, { flag: "wx", mode: 0o600 });
  fs.chmodSync(receipt, 0o600);
  transformed.fill(0);
  validateTransformedMock(stateDirectory, origin);
}

/** Verifies all receipt, pin, permission, and browser-origin invariants. */
export function validateTransformedMock(stateDirectory, origin) {
  if (!exactPublicOrigin(origin)) fail("invalid Tailnet origin");
  const { mock, receipt } = statePaths(stateDirectory);
  if (!exactPrivateRegularFile(mock)) fail("invalid transformed mock");
  const bytes = fs.readFileSync(mock);
  if (bytes.length === 0 || bytes.length > MAX_MOCK_BYTES) fail("invalid transformed mock");
  const receiptValue = readReceipt(receipt);
  if (receiptValue?.schema !== "oxid-tailnet-didit-mock-v1"
      || JSON.stringify(receiptValue.source) !== JSON.stringify({
        commit: PORTAL_COMMIT,
        tree: PORTAL_TREE,
        mockSha256: PINNED_MOCK_SHA256,
      })
      || receiptValue?.browserOrigin !== origin
      || receiptValue?.transformed?.bytes !== bytes.length
      || receiptValue?.transformed?.sha256 !== sha256(bytes)) {
    fail("mock receipt drifted");
  }
  validateBrowserUrls(bytes.toString("utf8"), origin);
}

function usage() {
  process.stderr.write("tailnet-mock-transform: FAIL phase=usage\n");
  process.exitCode = 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [operation, first, second, third] = process.argv.slice(2);
    if (operation === "--create" && !third && first && second) {
      // --create <source-directory> <state-directory> <origin>
      usage();
    } else if (operation === "--create" && first && second && third && process.argv.length === 6) {
      createTransformedMock(first, second, third);
    } else if (operation === "--validate" && first && second && !third && process.argv.length === 5) {
      validateTransformedMock(first, second);
    } else {
      usage();
    }
  } catch {
    process.stderr.write("tailnet-mock-transform: FAIL phase=validation\n");
    process.exitCode = 1;
  }
}
