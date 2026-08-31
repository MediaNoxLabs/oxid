// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

import {
  assertSameOriginJourney,
  formatNavigationDiagnostic,
} from "./portal-tailnet-browser-flow.mjs";

const source = fs.readFileSync(new URL("./portal-tailnet-browser-flow.mjs", import.meta.url), "utf8");

const origin = "https://oxid-demo.tail1234.ts.net:11443";

test("browser Tailnet journey accepts only the reviewed HTTPS origin and one private offer", () => {
  assert.doesNotThrow(() => assertSameOriginJourney({
    origin,
    locations: [
      `${origin}/issue/`,
      `${origin}/kyc/mock-verification`,
      `${origin}/issue/pending.html`,
      `${origin}/issue/complete.html`,
    ],
    copyOffer: "openid-credential-offer://private-test-value",
    qrRendered: true,
    sessionOffer: "openid-credential-offer://private-test-value",
  }));
});

test("browser Tailnet journey rejects any navigation order other than index, mock, pending, complete", () => {
  assert.throws(() => assertSameOriginJourney({
    origin,
    locations: [
      `${origin}/issue/`,
      `${origin}/kyc/mock-verification`,
      `${origin}/issue/complete.html`,
      `${origin}/issue/pending.html`,
    ],
    copyOffer: "openid-credential-offer://private-test-value",
    qrRendered: true,
    sessionOffer: "openid-credential-offer://private-test-value",
  }));
});

test("browser navigation diagnostics expose only elapsed time and an enum path class", () => {
  assert.equal(
    formatNavigationDiagnostic({ elapsedMs: 42, pathClass: "pending" }),
    "portal-tailnet-browser-flow: navigation elapsed_ms=42 path_class=pending",
  );
  assert.throws(() => formatNavigationDiagnostic({ elapsedMs: 42, pathClass: "/issue/pending.html" }));
});

test("browser waits for the index, mock, and complete documents before it acts", () => {
  assert.equal((source.match(/document\.readyState === "complete"/gu) ?? []).length, 3);
});

test("browser follows the pinned pending page's explicit continuation to completion", () => {
  assert.match(source, /textContent === 'Continue'/u);
  assert.match(source, /document\.getElementById\('action-button'\)\?\.click\(\) \?\? false/u);
});

test("browser CDP attaches to a page target rather than the browser target", () => {
  assert.match(source, /\$\{endpoint\}\/json\/list/u);
  assert.doesNotMatch(source, /\$\{endpoint\}\/json\/version/u);
});

test("browser failures identify only payload-free phases and navigation classes", () => {
  assert.match(source, /let phase = "connect";/u);
  assert.match(source, /waitForRecordedNavigation\("pending"\)/u);
  assert.match(source, /FAIL phase=\$\{phase\}/u);
});

test("browser Tailnet journey rejects localhost, insecure navigation, and offer disagreement", () => {
  const shared = {
    origin,
    locations: [`${origin}/issue/`, "http://127.0.0.1:9090/mock-verification"],
    copyOffer: "openid-credential-offer://private-test-value",
    qrRendered: true,
    sessionOffer: "openid-credential-offer://private-test-value",
  };
  assert.throws(() => assertSameOriginJourney(shared));
  assert.throws(() => assertSameOriginJourney({
    ...shared,
    locations: [`${origin}/issue/`, `${origin}/kyc/mock-verification`, `${origin}/issue/pending.html`, `${origin}/issue/complete.html`],
    copyOffer: "openid-credential-offer://other-private-test-value",
  }));
});
