// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const skillPath = path.join(repoRoot, ".pi", "skills", "oxid-portal-e2e", "SKILL.md");

function frontmatter(markdown) {
  const match = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n/u);
  assert.ok(match, "skill must begin with YAML frontmatter");
  const entries = Object.fromEntries(match[1].split(/\r?\n/u).map((line) => {
    const entry = line.match(/^([a-z][a-z-]*):\s+"?([^"\n]+?)"?$/u);
    assert.ok(entry, `invalid frontmatter entry: ${line}`);
    return [entry[1], entry[2]];
  }));
  return entries;
}

function machineContract(markdown) {
  const match = markdown.match(/```json oxid-portal-e2e-contract-v1\r?\n([\s\S]*?)\r?\n```/u);
  assert.ok(match, "skill must carry its machine-readable operational contract");
  return JSON.parse(match[1]);
}

test("Lace ID Portal E2E skill preserves its bounded operational contract", async () => {
  const skill = await readFile(skillPath, "utf8");
  const metadata = frontmatter(skill);
  assert.equal(metadata.name, "oxid-portal-e2e");
  assert.match(metadata.description, /Lace ID Portal E2E/u);

  const contract = machineContract(skill);
  assert.equal(contract.schema, "oxid-portal-e2e-skill-contract-v1");
  assert.deepEqual(contract.commands.localLadder, [
    "just portal-macos-laptop-e2e",
    "just portal-mobile-simulators-e2e",
  ]);
  assert.equal(contract.commands.physicalTailnet, "just android-portal-tailnet-physical-smoke");
  assert.deepEqual(contract.local.virtualSelectors, {
    privacy: "operator-private-explicit",
    required: [
      "OXID_XCODE_DEVELOPER_DIR",
      "OXID_IOS_RUNTIME_ID",
      "OXID_IOS_DEVICE_TYPE_ID",
      "OXID_ANDROID_AVD",
    ],
  });
  assert.deepEqual(contract.local.adb, {
    inventory: "empty-before-qemu",
    physicalPhones: "disconnected",
    physicalPhoneEvidence: "never-simulator-substitute",
  });
  assert.deepEqual(contract.physicalTailnet, {
    tailscale: ["mac-online", "phone-online", "serve-validated-without-identities"],
    adb: { count: 1, device: "approved-non-qemu", simulatorSubstitution: false },
  });
  assert.deepEqual(contract.standalone, {
    ports: [6300, 8088, 9944],
    missingListeners: "actionable",
    inspectOwnership: true,
    dockerDesktop: "start-if-authorized",
    start: "just standalone-up",
    preservePreexistingHealthy: true,
    teardown: "session-owned-or-explicit-owner-authorization",
  });
  assert.deepEqual(contract.safety, {
    retries: "fresh-offer-capability-app-runtime",
    neverReuseConsumedOffers: true,
    acceptance: [
      "explicit-consent",
      "pre-consent-zero-secret-calls",
      "encrypted-persistence",
      "true-restart",
      "listing",
      "fresh-reverification",
    ],
    evidence: { mode: "0600", redacted: true, exactHeadAndTree: true },
    cleanup: "receipt-and-process-owner-safe",
  });
  assert.deepEqual(contract.boundaries, {
    local: { network: "loopback", trust: "pinned-development" },
    tailnet: { network: "tailscale-https", trust: "tailnet-development" },
    prohibitedClaims: ["production-trust", "live-kyc"],
  });

  const runbooks = [
    "docs/factory/portal-macos-laptop.md",
    "docs/factory/portal-mobile-simulators.md",
    "docs/factory/portal-android-tailnet-physical.md",
  ];
  for (const runbook of runbooks) {
    const relativeLink = path.relative(path.dirname(skillPath), path.join(repoRoot, runbook)).split(path.sep).join("/");
    assert.ok(skill.includes(`](${relativeLink})`), `skill must link ${runbook}`);
    await access(path.join(repoRoot, runbook));
  }
});

test("repository verification runs the Portal E2E skill contract exactly once", async () => {
  const runner = await readFile(path.join(repoRoot, "run.sh"), "utf8");
  const registration = "node --test tests/repository/oxid-portal-e2e-skill-contract.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1);
  assert.match(runner.match(/run_repository\(\) \{([\s\S]*?)\n\}/u)?.[1] ?? "", new RegExp(registration.replaceAll(".", "\\.")));
});
