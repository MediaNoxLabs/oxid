// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import {
  transformBrowserUrls,
  validateBrowserUrls,
} from "./tailnet-mock-transform.mjs";

const origin = "https://oxid-demo.tail1234.ts.net:11443";
const source = `# pinned mock\nresponses:\n  - body: |\n      {\n        "url": "http://localhost:9090/mock-verification"\n      }\n  - body: |\n      {\n        "session_url": "http://localhost:9090/mock-verification"\n      }\n<script>window.location.href = 'http://localhost:8090/issue/pending.html';</script>\n`;

test("Tailnet mock transformation changes only the three pinned browser navigation values", () => {
  const transformed = transformBrowserUrls(source, origin);

  assert.equal((transformed.match(new RegExp(`${origin}/kyc/mock-verification`, "g")) ?? []).length, 2);
  assert.equal((transformed.match(new RegExp(`${origin}/issue/pending\\.html`, "g")) ?? []).length, 1);
  assert.doesNotMatch(transformed, /localhost|127\.0\.0\.1/u);
  assert.doesNotMatch(transformed, /http:\/\//u);
  assert.doesNotThrow(() => validateBrowserUrls(transformed, origin));
});

test("Tailnet mock transformation fails closed on pinned occurrence or origin drift", () => {
  assert.throws(() => transformBrowserUrls(source.replace("session_url", "session"), origin));
  assert.throws(() => transformBrowserUrls(source, "http://oxid-demo.tail1234.ts.net:11443"));
  assert.throws(() => validateBrowserUrls(
    source.replaceAll("http://localhost:9090/mock-verification", `${origin}/kyc/mock-verification`)
      .replace("http://localhost:8090/issue/pending.html", `${origin}/issue/pending.html?drift=1`),
    origin,
  ));
});
