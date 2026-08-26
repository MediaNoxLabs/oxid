// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const filter = path.join(root, "portal-android-evidence.jq");

const scenarios = [
  { mode: "cold-route", passed: true, measurements: { coldIngress: true, oneItemIngress: true } },
  {
    mode: "issue",
    passed: true,
    measurements: {
      exactBundleImported: true,
      explicitConsent: true,
      managedAuthenticationProof: true,
      separateJubjubAssertionBinding: true,
      strictFinalExchange: true,
      warmIngress: true,
    },
  },
  { mode: "issue-error", passed: true, measurements: { issueErrorEscapedSafely: true, warmIngress: true } },
  { mode: "malformed", passed: true, measurements: { malformedRejected: true, warmIngress: true } },
  { mode: "prepare-holder", passed: true, measurements: { managedDidPrepared: true } },
  { mode: "protocol-error", passed: true, measurements: { unavailableRejected: true, warmIngress: true } },
  { mode: "protocol-timeout", passed: true, measurements: { timeoutRejected: true, warmIngress: true } },
  {
    mode: "restored",
    passed: true,
    measurements: { custodyReactivated: true, freshReverification: true, listedAfterRestart: true },
  },
  {
    mode: "route-refuse",
    passed: true,
    measurements: { refusalBeforeConsent: true, refusalSecretEndpointCalls: 0, warmIngress: true },
  },
];

const counters = {
  authorizationMetadata: 3,
  credential: 1,
  issuerMetadata: 7,
  issuerResolution: 3,
  issuerResolutionSuccess: 3,
  kyc: 14,
  nonce: 1,
  other: 0,
  token: 2,
};

function render({ duration = 299, measuredCounters = counters, measuredScenarios = scenarios } = {}) {
  const text = execFileSync("jq", [
    "-cn",
    "--arg", "head", "a".repeat(40),
    "--arg", "commit", "b".repeat(40),
    "--arg", "tree", "c".repeat(40),
    "--arg", "resolver", `sha256:${"d".repeat(64)}`,
    "--arg", "didManager", `sha256:${"e".repeat(64)}`,
    "--arg", "issuer", `sha256:${"f".repeat(64)}`,
    "--arg", "os", "16",
    "--arg", "api", "36",
    "--argjson", "duration", String(duration),
    "--argjson", "counters", JSON.stringify(measuredCounters),
    "--argjson", "scenarios", JSON.stringify(measuredScenarios),
    "--argjson", "encryptedPersistence", "true",
    "--argjson", "processRestart", "true",
    "--argjson", "noAdbReverse", "true",
    "--argjson", "tailnetIdentityDiscovered", "true",
    "--argjson", "temporaryListenerDiscovered", "true",
    "--argjson", "preservedStandaloneRoutes", "true",
    "--argjson", "exactServeReceiptCleanup", "true",
    "--argjson", "portalConsumerCleanup", "true",
    "-f", filter,
  ], { encoding: "utf8" });
  return JSON.parse(text);
}

const booleanAcceptance = [
  "mockKycApproved",
  "warmIngress",
  "coldIngress",
  "refusalBeforeConsent",
  "malformedRejected",
  "unavailableRejected",
  "timeoutRejected",
  "issueErrorEscapedSafely",
  "exactProtocolCounters",
  "strictFinalExchange",
  "explicitConsent",
  "managedAuthenticationProof",
  "separateJubjubAssertionBinding",
  "exactBundleImported",
  "encryptedPersistence",
  "processRestart",
  "custodyReactivated",
  "listedAfterRestart",
  "freshReverification",
  "oneItemIngress",
  "noAdbReverse",
  "tailnetIdentityDiscovered",
  "temporaryListenerDiscovered",
  "preservedStandaloneRoutes",
  "exactServeReceiptCleanup",
  "portalConsumerCleanup",
  "completedWithin300Seconds",
];

test("physical evidence schema contains exact measured passing results", () => {
  const evidence = render();
  assert.equal(evidence.schema, "oxid-portal-android-evidence-v1");
  assert.equal(evidence.oxid.head, "a".repeat(40));
  assert.deepEqual(evidence.measurements.protocolCounters, counters);
  assert.deepEqual(evidence.measurements.scenarioResults, scenarios);
  assert.equal(evidence.measurements.completedSeconds, 299);
  assert.equal(evidence.measurements.portalConsumerCleanup, true);
  assert.equal(evidence.acceptance.refusalSecretEndpointCalls, 0);
  assert.deepEqual(Object.keys(evidence.acceptance).sort(), [
    ...booleanAcceptance,
    "refusalSecretEndpointCalls",
  ].sort());
  for (const name of booleanAcceptance) assert.equal(evidence.acceptance[name], true, name);
});

test("physical evidence acceptance changes when measured scenarios or counters change", () => {
  const alteredScenarios = structuredClone(scenarios);
  alteredScenarios.find(({ mode }) => mode === "issue").measurements.strictFinalExchange = false;
  const alteredCounters = { ...counters, token: 3 };
  const evidence = render({ duration: 301, measuredCounters: alteredCounters, measuredScenarios: alteredScenarios });
  assert.equal(evidence.acceptance.strictFinalExchange, false);
  assert.equal(evidence.acceptance.exactProtocolCounters, false);
  assert.equal(evidence.acceptance.completedWithin300Seconds, false);
  assert.equal(evidence.measurements.protocolCounters.token, 3);
});
