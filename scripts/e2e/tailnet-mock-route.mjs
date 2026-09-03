#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { pathToFileURL } from "node:url";

import { exactPublicOrigin } from "./tailnet-origin-policy.mjs";

export const MOCK_KYC_MOUNT_PATH = "/kyc";
export const MOCK_VERIFICATION_UPSTREAM_PATH = "/mock-verification";
export const MOCK_VERIFICATION_UPSTREAM = "http://127.0.0.1:9090";

function fail(message) {
  throw new Error(message);
}

function validPort(value) {
  return Number.isInteger(value) && value >= 1 && value <= 65_535;
}

/** Models Tailscale Serve's --set-path removal before an upstream request. */
export function stripTailnetMount(mountPath, requestPath) {
  if (mountPath !== MOCK_KYC_MOUNT_PATH || typeof requestPath !== "string"
      || !requestPath.startsWith(`${mountPath}/`)) {
    fail("invalid Tailnet mock route");
  }
  const upstreamRequestPath = requestPath.slice(mountPath.length);
  if (upstreamRequestPath !== MOCK_VERIFICATION_UPSTREAM_PATH) {
    fail("unexpected Smocker request path");
  }
  return upstreamRequestPath;
}

export function mockKycExternalUrl(origin) {
  if (!exactPublicOrigin(origin)) fail("invalid Tailnet origin");
  return `${origin}${MOCK_KYC_MOUNT_PATH}${MOCK_VERIFICATION_UPSTREAM_PATH}`;
}

/** Defines the one external request, Serve mount, and Smocker request path. */
export function mockKycRoute(origin, httpsPort) {
  if (!validPort(httpsPort)) fail("invalid HTTPS port");
  const externalRequest = mockKycExternalUrl(origin);
  const upstreamRequestPath = stripTailnetMount(
    MOCK_KYC_MOUNT_PATH,
    new URL(externalRequest).pathname,
  );
  return {
    externalRequest,
    mountPath: MOCK_KYC_MOUNT_PATH,
    upstream: MOCK_VERIFICATION_UPSTREAM,
    upstreamRequestPath,
  };
}

function usage() {
  process.stderr.write("tailnet-mock-route: FAIL phase=usage\n");
  process.exitCode = 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [operation, origin, port] = process.argv.slice(2);
    if (operation !== "--config" || !origin || !/^[0-9]+$/u.test(port ?? "")
        || process.argv.length !== 5) {
      usage();
    } else {
      const route = mockKycRoute(origin, Number(port));
      process.stdout.write(`${JSON.stringify({
        route: { path: route.mountPath, httpsPort: Number(port), upstream: route.upstream },
        externalRequestPath: new URL(route.externalRequest).pathname,
        upstreamRequestPath: route.upstreamRequestPath,
      })}\n`);
    }
  } catch {
    process.stderr.write("tailnet-mock-route: FAIL phase=validation\n");
    process.exitCode = 1;
  }
}
