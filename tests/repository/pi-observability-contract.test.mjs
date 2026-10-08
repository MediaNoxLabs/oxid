// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  observabilityEnvironment,
  observabilityTags,
  parseObservedArgs,
} from "../../scripts/factory/pi-observability.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("observed launcher accepts only the bounded tag vocabulary", () => {
  const parsed = parseObservedArgs([
    "--lane", "host-mobile",
    "--profile", "prototype",
    "--work-type", "test",
    "--delivery-target", "milestone",
    "--", "--model", "openai-codex/gpt-5.6-terra",
  ]);
  assert.deepEqual(parsed.piArgs, ["--model", "openai-codex/gpt-5.6-terra"]);
  assert.deepEqual(observabilityTags(parsed), [
    "project=oxid",
    "factory=pi-dev",
    "environment=local",
    "lane=host-mobile",
    "profile=prototype",
    "work_type=test",
    "delivery_target=milestone",
  ]);
  assert.throws(() => parseObservedArgs([
    "--lane", "issue-923", "--profile", "prototype", "--work-type", "test", "--delivery-target", "milestone",
  ]), /--lane must be one of/u);
  assert.throws(() => parseObservedArgs([
    "--lane", "docker", "--profile", "prototype", "--work-type", "test", "--delivery-target", "milestone", "--issue", "923",
  ]), /unknown or repeated option/u);
});

test("observed launcher is local metadata-only and does not forward", () => {
  const parsed = parseObservedArgs([
    "--lane", "docker", "--profile", "research", "--work-type", "chore", "--delivery-target", "develop",
  ]);
  const env = observabilityEnvironment(parsed, { KEEP: "yes", AGENTO11Y_CONTENT_CAPTURE_MODE: "full", AGENTO11Y_TAGS: "secret=bad" });
  assert.equal(env.KEEP, "yes");
  assert.equal(env.AGENTO11Y_LOCAL, "false");
  assert.equal(env.AGENTO11Y_LOCAL_FORWARD, "false");
  assert.equal(env.AGENTO11Y_ENDPOINT, "http://127.0.0.1:8765");
  assert.equal(env.AGENTO11Y_AUTH_TENANT_ID, "local");
  assert.equal(env.AGENTO11Y_AUTH_TOKEN, "local");
  assert.equal(env.AGENTO11Y_OTEL_EXPORTER_OTLP_ENDPOINT, "http://127.0.0.1:8765/otlp");
  assert.equal(env.AGENTO11Y_CONTENT_CAPTURE_MODE, "metadata_only");
  assert.equal(env.AGENTO11Y_GUARDS_ENABLED, "false");
  assert.equal(env.AGENTO11Y_AUTO_CODING_AGENT_TAGS, "false");
  assert.equal(env.AGENTO11Y_TAGS, "project=oxid,factory=pi-dev,environment=local,lane=docker,profile=research,work_type=chore,delivery_target=develop");
});

test("project package remains disabled and Grafana assets are bounded to loopback", async () => {
  const settings = JSON.parse(await readFile(path.join(repoRoot, ".pi", "settings.json"), "utf8"));
  assert.deepEqual(settings.packages.at(-1), {
    source: "npm:@grafana/agento11y-pi@0.25.0",
    extensions: [],
  });
  const launcher = await readFile(path.join(repoRoot, "scripts", "factory", "pi-observability.mjs"), "utf8");
  assert.match(launcher, /spawnSync\("agento11y", \["pi", "--no-local"/u);
  assert.match(launcher, /"--", "-e", extension/u);
  assert.match(launcher, /spawnSync\("pi", piArgs/u);
  const dashboard = JSON.parse(await readFile(path.join(repoRoot, "docs", "factory", "grafana", "oxid-pi-factory.json"), "utf8"));
  assert.equal(dashboard.uid, "oxid-pi-factory");
  assert.ok(dashboard.panels.length >= 5);
  for (const panel of dashboard.panels) {
    for (const target of panel.targets ?? []) assert.match(target.url, /^http:\/\/127\.0\.0\.1:8765\/api\/v1\/metrics\//u);
  }
  const installer = await readFile(path.join(repoRoot, "scripts", "factory", "grafana-pi-dashboard.mjs"), "utf8");
  assert.match(installer, /PLUGIN_VERSION = "4\.1\.0"/u);
  assert.match(installer, /allowedHosts/u);
  assert.match(installer, /remove/u);
});
