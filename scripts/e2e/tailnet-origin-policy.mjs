#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { pathToFileURL } from "node:url";

const RESERVED_PORTS = new Set([443, 8443, 10000]);

export function exactMagicDnsName(value) {
  if (typeof value !== "string" || value.length === 0 || value.length > 253
      || !value.endsWith(".ts.net") || value === "ts.net") {
    return false;
  }
  return value.split(".").every((label) => label.length >= 1 && label.length <= 63
    && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/u.test(label));
}

export function exactPublicOrigin(value) {
  if (typeof value !== "string" || value.length > 512) return false;
  try {
    const parsed = new URL(value);
    const port = Number(parsed.port);
    return parsed.protocol === "https:"
      && parsed.username === "" && parsed.password === ""
      && Number.isInteger(port) && port >= 1024 && port <= 65535
      && !RESERVED_PORTS.has(port)
      && parsed.pathname === "/" && parsed.search === "" && parsed.hash === ""
      && exactMagicDnsName(parsed.hostname)
      && parsed.origin === value;
  } catch {
    return false;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const operation = process.argv[2];
  const value = process.env.OXID_TAILNET_ORIGIN_POLICY_INPUT;
  const accepted = operation === "--origin-env"
    ? exactPublicOrigin(value)
    : operation === "--host-env" && exactMagicDnsName(value);
  if (!accepted) process.exitCode = 1;
}
