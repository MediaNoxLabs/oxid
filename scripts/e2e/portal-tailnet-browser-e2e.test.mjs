// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const script = fs.readFileSync(path.join(root, "scripts/e2e/portal-tailnet-browser-e2e.sh"), "utf8");
const justfile = fs.readFileSync(path.join(root, "Justfile"), "utf8");

test("Tailnet browser E2E owns a private transformed mock and exact Serve cleanup", () => {
  for (const required of [
    'tailnet-mock-transform.mjs',
    'tailnet-mock-route.mjs',
    '--create "$SOURCE" "$MOCK_STATE" "$public_origin"',
    'PORTAL_TAILNET_MOCK_STATE_DIR="$MOCK_STATE"',
    'node "$MOCK_ROUTE" --config "$public_origin" "$listener"',
    'externalRequestPath:"/kyc/mock-verification"',
    'upstreamRequestPath:"/mock-verification"',
    '$mock_route.route',
    '"$public_origin$mock_external_path"',
    'tailscale serve status --json',
    '[ "$after_cleanup" = "$baseline" ]',
    'chmod 600 "$EVIDENCE_ROOT/evidence.json"',
    'Google Chrome.app',
    'mock-page.html',
    'id="approve-btn"',
  ]) assert.ok(script.includes(required), required);
  assert.match(script, /mock_route_config=.*--config "\$public_origin" "\$listener"/u);
  assert.doesNotMatch(script, /path:"\/mock-verification",httpsPort:\$port/u);
  assert.doesNotMatch(script, /"\$public_origin\/mock-verification"/u);
  assert.doesNotMatch(script, /\badb\b/u);
  assert.doesNotMatch(script, /android-portal-tailnet-physical/u);
});

test("Tailnet browser evidence keeps the QR/copy agreement payload-free under its redaction scan", () => {
  assert.match(script, /qrAndCopyUriAgree:true/u);
  assert.doesNotMatch(script, /qrAndCopyOfferAgree/u);
});

test("Tailnet browser E2E removes rejected evidence during failure cleanup", () => {
  assert.match(script, /if \[ "\$incoming" -ne 0 \]; then\n    rm -f -- "\$EVIDENCE_ROOT\/evidence\.json"/u);
});

test("Tailnet browser E2E reports only sanitized navigation timing and path classes on failure", () => {
  assert.match(script, /browser-navigation=%s/u);
  assert.match(script, /navigation elapsed_ms=\[0-9\]\+ path_class=\(index\|mock\|pending\|complete\)/u);
  assert.doesNotMatch(script, /browser-navigation=.*url/u);
});

test("Tailnet browser E2E is an explicit browser-only Just target and repository contract", () => {
  assert.match(justfile, /^portal-tailnet-browser-e2e:\n/mu);
  const runner = fs.readFileSync(path.join(root, "run.sh"), "utf8");
  assert.equal(
    runner.split("node --test scripts/e2e/portal-tailnet-browser-e2e.test.mjs").length - 1,
    1,
  );
});
