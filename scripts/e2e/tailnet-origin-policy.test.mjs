// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";

import { exactMagicDnsName, exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const validOrigins = [
  "https://oxid-demo.tail1234.ts.net:9443",
  "https://wallet.tailabcd.ts.net:12001",
];
const invalidOrigins = [
  "http://oxid-demo.tail1234.ts.net:9443",
  "https://oxid-demo.tail1234.ts.net",
  "https://oxid-demo.tail1234.ts.net:443",
  "https://oxid-demo.tail1234.ts.net:8443",
  "https://oxid-demo.tail1234.ts.net:10000",
  "https://oxid-demo.tail1234.ts.net:9443/offer",
  "https://user@oxid-demo.tail1234.ts.net:9443",
  "https://127.0.0.1:9443",
  "https://Oxid-demo.tail1234.ts.net:9443",
  "https://-oxid.tail1234.ts.net:9443",
  "https://oxid.example:9443",
];

test("script origin policy matches the Rust authority vectors", () => {
  for (const origin of validOrigins) assert.equal(exactPublicOrigin(origin), true, origin);
  for (const origin of invalidOrigins) assert.equal(exactPublicOrigin(origin), false, origin);
});

test("bash callers can validate a discovered canonical MagicDNS name without parsing it", () => {
  assert.equal(exactMagicDnsName("oxid-demo.tail1234.ts.net"), true);
  for (const host of ["ts.net", "Oxid.tail1234.ts.net", "-oxid.tail1234.ts.net", "oxid.example"]) {
    assert.equal(exactMagicDnsName(host), false, host);
  }
});
