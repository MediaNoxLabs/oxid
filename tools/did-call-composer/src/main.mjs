#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { ComposerError, composeDidCall } from "./compose.mjs";

const MAX_REQUEST_BYTES = 40 * 1024 * 1024;

async function readRequest() {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of process.stdin) {
    bytes += chunk.length;
    if (bytes > MAX_REQUEST_BYTES) {
      throw new ComposerError("request_too_large", "DID composer request is too large");
    }
    chunks.push(chunk);
  }
  if (bytes === 0) throw new ComposerError("invalid_request", "DID composer request is invalid");
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new ComposerError("invalid_request", "DID composer request is invalid");
  }
}

try {
  process.stdout.write(`${JSON.stringify(await composeDidCall(await readRequest()))}\n`);
} catch (error) {
  const safe = error instanceof ComposerError
    ? error
    : new ComposerError("unavailable", "DID composer is unavailable");
  process.stdout.write(`${JSON.stringify({
    schemaVersion: 1,
    ok: false,
    error: { code: safe.code, message: safe.message },
  })}\n`);
  process.exitCode = 1;
}
