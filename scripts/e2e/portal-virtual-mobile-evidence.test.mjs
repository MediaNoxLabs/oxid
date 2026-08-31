// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  buildEvidence,
  publishEvidence,
  validateEvidence,
} from "./portal-virtual-mobile-evidence.mjs";

const zero = Object.freeze({
  authorizationMetadata: 0,
  credential: 0,
  issuerMetadata: 0,
  issuerResolution: 0,
  issuerResolutionSuccess: 0,
  kyc: 0,
  nonce: 0,
  other: 0,
  token: 0,
});

function delta(overrides = {}) {
  return { ...zero, ...overrides };
}

function measurements(kind = "android_emulator") {
  const ios = kind === "ios_simulator";
  return {
    oxid: { head: "a".repeat(40), tree: "b".repeat(40) },
    portal: {
      integrationCommit: "22ae5369b6f939e6b20648f4b85dd993527748ef",
      integrationTree: "74d8d1a5b87c160ea554006e47d5f3edc3cd3e10",
      provenanceSha256: "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87",
    },
    deployment: {
      manifestSchema: "oxid-portal-deployment-v3",
      authoritySchema: "oxid-app-profile-authority-v2",
    },
    platform: {
      kind,
      osFamily: ios ? "ios" : "android",
      apiLevel: ios ? 26 : 35,
      architecture: "arm64",
    },
    artifactSha256: "c".repeat(64),
    scenarios: [
      { name: "cold-route", passed: true, counterDelta: delta() },
      { name: "prepare-holder", passed: true, counterDelta: delta() },
      {
        name: "route-refuse",
        passed: true,
        counterDelta: delta({ authorizationMetadata: 1, issuerMetadata: 1 }),
      },
      { name: "malformed", passed: true, counterDelta: delta({ issuerMetadata: 1 }) },
      {
        name: "protocol-error",
        passed: true,
        counterDelta: delta({ issuerMetadata: ios ? 1 : 2 }),
      },
      { name: "protocol-timeout", passed: true, counterDelta: delta({ issuerMetadata: 1 }) },
      {
        name: "issue-error",
        passed: true,
        counterDelta: delta({ authorizationMetadata: 1, issuerMetadata: 1, token: 1 }),
      },
      {
        name: "issue",
        passed: true,
        counterDelta: delta({
          authorizationMetadata: 1,
          credential: 1,
          issuerMetadata: 1,
          issuerResolution: 1,
          issuerResolutionSuccess: 1,
          nonce: 1,
          token: 1,
        }),
      },
      {
        name: "restored",
        passed: true,
        counterDelta: delta({ issuerResolution: 1, issuerResolutionSuccess: 1 }),
      },
    ],
    totalCounters: {
      authorizationMetadata: 3,
      credential: 1,
      issuerMetadata: ios ? 6 : 7,
      issuerResolution: 3,
      issuerResolutionSuccess: 3,
      kyc: 14,
      nonce: 1,
      other: 0,
      token: 2,
    },
    offer: {
      triggerOnly: true,
      capabilityMode0600: true,
      capabilityHex64: true,
      stagedAtomically: true,
      burnedBeforeNetwork: true,
      oneShotReadyThenEmpty: true,
      exactRouteCopy: true,
      exactPreview: true,
      fiveQuestions: true,
      rawOfferCleared: true,
      consentUnchecked: true,
      issuanceDisabled: true,
      refusalDeltaExact: true,
      metadataPreviewCallsExpected: true,
      secretCallsBeforeConsent: 0,
      issuerResolutionCallsBeforeConsent: 0,
      offerArmKycOutsideBaseline: true,
    },
    issuance: {
      explicitConsent: true,
      deltaExact: true,
      exactlyOneValidCredential: true,
      claimsHidden: true,
    },
    storage: {
      envelopeHeader: true,
      keyBytes32: true,
      ciphertextDenylistClean: true,
    },
    restart: {
      processAbsent: true,
      differentGeneration: true,
      noDataReset: true,
      custodyReactivated: true,
      oneValidCredential: true,
      noStaleMarker: true,
      reverifyDeltaExact: true,
      freshMarker: true,
    },
    cleanup: {
      virtualTargetOnly: true,
      targetRemoved: true,
      mappingsRestored: true,
      listenersRestored: true,
      stackRestored: true,
      buildSourceRemoved: true,
      privateArtifactsRemoved: true,
      headClean: true,
    },
  };
}

for (const kind of ["android_emulator", "ios_simulator"]) {
  test(`${kind} renders the same closed passing evidence contract`, () => {
    const evidence = buildEvidence(measurements(kind));
    assert.equal(evidence.schema, "oxid-portal-virtual-mobile-evidence-v1");
    assert.equal(evidence.platform.kind, kind);
    assert.equal(evidence.scenarios.length, 9);
    assert.equal(evidence.acceptance.accepted, true);
    assert.equal(evidence.acceptance.secretFreeEvidence, true);
    assert.doesNotThrow(() => validateEvidence(evidence, { requireAccepted: true }));
  });
}

test("unavailable preview counts stay bound to measured platform transport behavior", () => {
  const ios = measurements("ios_simulator");
  ios.scenarios[4].counterDelta.issuerMetadata = 2;
  ios.totalCounters.issuerMetadata = 7;
  assert.equal(buildEvidence(ios).acceptance.accepted, false);

  const android = measurements("android_emulator");
  android.scenarios[4].counterDelta.issuerMetadata = 1;
  android.totalCounters.issuerMetadata = 6;
  assert.equal(buildEvidence(android).acceptance.accepted, false);
});

test("acceptance is derived from every required measured boolean and boundary count", () => {
  const base = measurements();
  const booleanGroups = ["offer", "issuance", "storage", "restart", "cleanup"];
  for (const group of booleanGroups) {
    for (const [name, value] of Object.entries(base[group])) {
      if (typeof value !== "boolean") continue;
      const altered = structuredClone(base);
      altered[group][name] = false;
      assert.equal(buildEvidence(altered).acceptance.accepted, false, `${group}.${name}`);
    }
  }
  for (const name of ["secretCallsBeforeConsent", "issuerResolutionCallsBeforeConsent"]) {
    const altered = structuredClone(base);
    altered.offer[name] = 1;
    assert.equal(buildEvidence(altered).acceptance.accepted, false, `offer.${name}`);
  }
  const alteredScenario = structuredClone(base);
  alteredScenario.scenarios[7].counterDelta.token = 2;
  assert.equal(buildEvidence(alteredScenario).acceptance.accepted, false);
  const alteredTotal = structuredClone(base);
  alteredTotal.totalCounters.kyc = 13;
  assert.equal(buildEvidence(alteredTotal).acceptance.accepted, false);
});

test("unknown fields and secret-bearing string sentinels fail closed", () => {
  const unknown = measurements();
  unknown.extra = true;
  assert.throws(() => buildEvidence(unknown), /unknown/u);

  const sentinel = measurements();
  sentinel.platform.osFamily = "https://secret.invalid";
  assert.throws(() => buildEvidence(sentinel), /secret|platform/u);
});

test("exclusive publication does not clobber and leaves mode 0600", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "oxid-mobile-evidence-"));
  try {
    const output = path.join(directory, "evidence.json");
    const published = publishEvidence(output, measurements());
    assert.equal(published.acceptance.accepted, true);
    assert.equal(fs.statSync(output).mode & 0o777, 0o600);
    const before = fs.readFileSync(output);
    assert.throws(() => publishEvidence(output, measurements()), /exists|occupied/u);
    assert.deepEqual(fs.readFileSync(output), before);
  } finally {
    fs.rmSync(directory, { force: true, recursive: true });
  }
});
