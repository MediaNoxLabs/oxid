// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

import { exactMagicDnsName, exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const { validOrigins, invalidOrigins } = JSON.parse(
  fs.readFileSync(new URL("./tailnet-origin-vectors.json", import.meta.url), "utf8"),
);

test("script origin policy enforces the shared Rust and JavaScript vectors", () => {
  for (const origin of validOrigins) assert.equal(exactPublicOrigin(origin), true, origin);
  for (const origin of invalidOrigins) assert.equal(exactPublicOrigin(origin), false, origin);
});

test("bash callers can validate a discovered canonical MagicDNS name without parsing it", () => {
  assert.equal(exactMagicDnsName("oxid-demo.tail1234.ts.net"), true);
  for (const host of ["ts.net", "Oxid.tail1234.ts.net", "-oxid.tail1234.ts.net", "oxid.example"]) {
    assert.equal(exactMagicDnsName(host), false, host);
  }
});
