// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("root demo entrypoints are strict thin wrappers over one canonical lifecycle", async () => {
  for (const operation of ["start", "status", "stop"]) {
    const wrapper = await text(`demo/${operation}.sh`);
    assert.match(wrapper, /^#!\/usr\/bin\/env bash/m);
    assert.match(wrapper, /set -euo pipefail/);
    assert.match(wrapper, new RegExp(`scripts/demo-stack\\.sh" ${operation}$`, "m"));
    assert.doesNotMatch(wrapper, /docker|tailscale|adb|curl/);
  }
});

test("demo lifecycle records exact-head standalone ownership and delegates trust boundaries", async () => {
  const lifecycle = await text("scripts/demo-stack.sh");
  const query = lifecycle.indexOf("existing=\"$(query_standalone_containers)\"");
  const start = lifecycle.indexOf("standalone-phone-up");
  assert.ok(query >= 0 && query < start, "ownership query must precede standalone startup");
  assert.match(lifecycle, /oxid-portal-tailnet-demo-v1/);
  assert.match(lifecycle, /git -C "\$repository_root" rev-parse HEAD/);
  assert.match(lifecycle, /git -C "\$repository_root" rev-parse 'HEAD\^\{tree\}'/);
  assert.match(lifecycle, /chmod 600 "\$candidate"/);
  assert.match(lifecycle, /standaloneOwned/);
  assert.match(lifecycle, /portal-tailnet-manual-start/);
  assert.match(lifecycle, /portal-tailnet-manual-status/);
  assert.match(lifecycle, /portal-tailnet-manual-stop/);
  assert.match(lifecycle, /if \[ "\$standalone_owned" = true \]; then\s+just -f "\$repository_root\/Justfile" standalone-down/);
  assert.doesNotMatch(lifecycle, /tailscale serve|adb |docker compose|https:\/\//);
});

test("standalone status is read-only and checks local plus Tailnet readiness", async () => {
  const status = await text("scripts/standalone-status.sh");
  assert.match(status, /com\.docker\.compose\.project=oxid-standalone/);
  assert.match(status, /chain_getHeader/);
  assert.match(status, /StandaloneReadiness/);
  assert.match(status, /\.TCP\["443"\]\.HTTPS == true/);
  assert.match(status, /\.TCP\["8443"\]\.HTTPS == true/);
  assert.match(status, /\.TCP\["10000"\]\.HTTPS == true/);
  assert.match(status, /oxid standalone \(\$mode\): READY/);
  assert.doesNotMatch(status, /\b(up|down|start|stop|reset|rm)\b/);
});

test("operator runbook covers one-shot issuance and ownership-safe cleanup", async () => {
  const [runbook, runner, mainReadme, factoryIndex] = await Promise.all([
    text("demo/README.md"),
    text("run.sh"),
    text("README.md"),
    text("docs/factory/README.md"),
  ]);
  for (const phrase of [
    "demo/start.sh",
    "demo/status.sh",
    "demo/stop.sh",
    "Accept and issue credential",
    "Reveal locally",
    "do not reuse the QR",
    "physical Android\\s+only",
  ]) assert.match(runbook, new RegExp(phrase, "i"));
  assert.match(mainReadme, /demo\/README\.md/);
  assert.match(factoryIndex, /demo\/README\.md/);
  const registration = "node --test tests/repository/portal-tailnet-demo-kit-contract.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1);
});
