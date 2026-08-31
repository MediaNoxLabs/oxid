// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  mockKycRoute,
  stripTailnetMount,
} from "./tailnet-mock-route.mjs";

const origin = "https://oxid-demo.tail1234.ts.net:11443";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("Tailnet mock route keeps the external KYC request on HTTPS and preserves Smocker's exact path", () => {
  const route = mockKycRoute(origin, 11443);

  assert.deepEqual(route, {
    externalRequest: `${origin}/kyc/mock-verification`,
    mountPath: "/kyc",
    upstream: "http://127.0.0.1:9090",
    upstreamRequestPath: "/mock-verification",
  });
  assert.equal(
    stripTailnetMount(route.mountPath, new URL(route.externalRequest).pathname),
    route.upstreamRequestPath,
  );
});

test("Tailnet mock route CLI emits the exact receipt-owned Serve route", () => {
  const emitted = JSON.parse(execFileSync(process.execPath, [
    path.join(root, "scripts/e2e/tailnet-mock-route.mjs"),
    "--config",
    origin,
    "11443",
  ], { encoding: "utf8" }));

  assert.deepEqual(emitted, {
    route: { path: "/kyc", httpsPort: 11443, upstream: "http://127.0.0.1:9090" },
    externalRequestPath: "/kyc/mock-verification",
    upstreamRequestPath: "/mock-verification",
  });
});

test("Tailnet mock route is available through the focused and repository contracts", () => {
  const [justfile, runner] = [
    fs.readFileSync(path.join(root, "Justfile"), "utf8"),
    fs.readFileSync(path.join(root, "run.sh"), "utf8"),
  ];
  assert.match(justfile, /^portal-tailnet-route-contract:\n/mu);
  const registration = "node --test scripts/e2e/tailnet-mock-route.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1);
});

test("Tailnet mock route rejects requests that cannot become Smocker's exact mock path", () => {
  assert.throws(() => stripTailnetMount("/kyc", "/mock-verification"));
  assert.throws(() => stripTailnetMount("/kyc", "/kyc/"));
  assert.throws(() => mockKycRoute("http://oxid-demo.tail1234.ts.net:11443", 11443));
});
