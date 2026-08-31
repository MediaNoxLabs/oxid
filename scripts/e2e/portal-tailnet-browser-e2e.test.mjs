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
    '--create "$SOURCE" "$MOCK_STATE" "$public_origin"',
    'PORTAL_TAILNET_MOCK_STATE_DIR="$MOCK_STATE"',
    '{path:"/mock-verification",httpsPort:$port,upstream:"http://127.0.0.1:9090"}',
    'tailscale serve status --json',
    '[ "$after_cleanup" = "$baseline" ]',
    'chmod 600 "$EVIDENCE_ROOT/evidence.json"',
    'Google Chrome.app',
    'mock-page.html',
    'id="approve-btn"',
  ]) assert.ok(script.includes(required), required);
  assert.doesNotMatch(script, /\badb\b/u);
  assert.doesNotMatch(script, /android-portal-tailnet-physical/u);
});

test("Tailnet browser E2E is an explicit browser-only Just target and repository contract", () => {
  assert.match(justfile, /^portal-tailnet-browser-e2e:\n/mu);
  const runner = fs.readFileSync(path.join(root, "run.sh"), "utf8");
  assert.equal(
    runner.split("node --test scripts/e2e/portal-tailnet-browser-e2e.test.mjs").length - 1,
    1,
  );
});
