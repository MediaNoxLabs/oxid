#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCHEMA = "oxid-portal-virtual-mobile-evidence-v1";
const PORTAL = Object.freeze({
  integrationCommit: "22ae5369b6f939e6b20648f4b85dd993527748ef",
  integrationTree: "74d8d1a5b87c160ea554006e47d5f3edc3cd3e10",
  provenanceSha256: "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87",
});
const DEPLOYMENT = Object.freeze({
  manifestSchema: "oxid-portal-deployment-v3",
  authoritySchema: "oxid-app-profile-authority-v2",
});
const COUNTER_KEYS = Object.freeze([
  "authorizationMetadata",
  "credential",
  "issuerMetadata",
  "issuerResolution",
  "issuerResolutionSuccess",
  "holderPublications",
  "kyc",
  "nonce",
  "other",
  "token",
]);
const ZERO_COUNTERS = Object.freeze(Object.fromEntries(COUNTER_KEYS.map((key) => [key, 0])));
function expectedScenarios() {
  return [
    ["cold-route", {}],
    ["prepare-holder", {}],
    ["route-refuse", { authorizationMetadata: 1, issuerMetadata: 1 }],
    ["malformed", { issuerMetadata: 1 }],
    ["protocol-error", { issuerMetadata: 1 }],
    ["protocol-timeout", { issuerMetadata: 1 }],
    ["issue-error", { authorizationMetadata: 1, issuerMetadata: 1, token: 1 }],
    ["issue", {
      authorizationMetadata: 1,
      credential: 1,
      issuerMetadata: 1,
      issuerResolution: 1,
      issuerResolutionSuccess: 1,
      nonce: 1,
      token: 1,
    }],
    ["restored", { issuerResolution: 1, issuerResolutionSuccess: 1 }],
  ];
}

function expectedTotals() {
  return {
    authorizationMetadata: 3,
    credential: 1,
    issuerMetadata: 6,
    issuerResolution: 3,
    issuerResolutionSuccess: 3,
    holderPublications: 0,
    kyc: 14,
    nonce: 1,
    other: 0,
    token: 2,
  };
}
const OFFER_KEYS = Object.freeze([
  "triggerOnly",
  "capabilityMode0600",
  "capabilityHex64",
  "stagedAtomically",
  "burnedBeforeNetwork",
  "oneShotReadyThenEmpty",
  "exactRouteCopy",
  "exactPreview",
  "fiveQuestions",
  "rawOfferCleared",
  "consentUnchecked",
  "issuanceDisabled",
  "refusalDeltaExact",
  "metadataPreviewCallsExpected",
  "secretCallsBeforeConsent",
  "issuerResolutionCallsBeforeConsent",
  "offerArmKycOutsideBaseline",
]);
const ISSUANCE_KEYS = Object.freeze([
  "explicitConsent",
  "deltaExact",
  "exactlyOneValidCredential",
  "claimsHidden",
]);
const STORAGE_KEYS = Object.freeze(["envelopeHeader", "keyBytes32", "ciphertextDenylistClean"]);
const RESTART_KEYS = Object.freeze([
  "processAbsent",
  "differentGeneration",
  "noDataReset",
  "custodyReactivated",
  "oneValidCredential",
  "noStaleMarker",
  "reverifyDeltaExact",
  "freshMarker",
]);
const CLEANUP_KEYS = Object.freeze([
  "virtualTargetOnly",
  "targetRemoved",
  "mappingsRestored",
  "listenersRestored",
  "stackRestored",
  "buildSourceRemoved",
  "privateArtifactsRemoved",
  "headClean",
]);
const ACCEPTANCE_KEYS = Object.freeze([
  "exactOfferAndPreview",
  "preConsentBoundary",
  "issuance",
  "encryptedPersistence",
  "restartAndReverification",
  "exactScenarioDeltas",
  "exactTotalCounters",
  "cleanup",
  "virtualTargetOnly",
  "secretFreeEvidence",
  "accepted",
]);
const SECRET_VALUE = /(?:openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|https?:\/\/|(?:^|[\\/])(?:Users|tmp|private|var)(?:[\\/])|[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}|(?:^|\b)(?:grant|seed|serial|udid|avd|pid|capability)\s*[:=])/iu;

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.getPrototypeOf(value) === Object.prototype;
}

function assertClosedObject(value, keys, label) {
  if (!isPlainObject(value)) throw new Error(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    const unknown = actual.filter((key) => !expected.includes(key));
    throw new Error(unknown.length > 0 ? `${label} has unknown fields` : `${label} is incomplete`);
  }
}

function assertBooleanObject(value, keys, label) {
  assertClosedObject(value, keys, label);
  for (const key of keys) {
    if (typeof value[key] !== "boolean") throw new Error(`${label}.${key} must be boolean`);
  }
}

function assertHex(value, length, label) {
  if (typeof value !== "string" || !new RegExp(`^[0-9a-f]{${length}}$`, "u").test(value)) {
    throw new Error(`${label} is invalid`);
  }
}

function assertCounters(value, label) {
  assertClosedObject(value, COUNTER_KEYS, label);
  for (const key of COUNTER_KEYS) {
    if (!Number.isSafeInteger(value[key]) || value[key] < 0) throw new Error(`${label}.${key} is invalid`);
  }
}

function expectedDelta(overrides) {
  return { ...ZERO_COUNTERS, ...overrides };
}

function exactCounters(actual, expected) {
  return COUNTER_KEYS.every((key) => actual[key] === expected[key]);
}

function exactScenarioDeltas(scenarios) {
  return expectedScenarios().every(([name, overrides], index) => {
    const scenario = scenarios[index];
    return scenario.name === name && scenario.passed === true
      && exactCounters(scenario.counterDelta, expectedDelta(overrides));
  });
}

function allTrue(object, keys) {
  return keys.every((key) => object[key] === true);
}

function assertSecretFreeStrings(value) {
  if (typeof value === "string") {
    if (SECRET_VALUE.test(value)) throw new Error("evidence contains a secret or identifier sentinel");
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) assertSecretFreeStrings(item);
    return;
  }
  if (isPlainObject(value)) {
    for (const item of Object.values(value)) assertSecretFreeStrings(item);
  }
}

function validateMeasurements(input) {
  assertClosedObject(input, [
    "oxid", "portal", "deployment", "platform", "artifactSha256", "scenarios",
    "totalCounters", "offer", "issuance", "storage", "restart", "cleanup",
  ], "measurements");
  assertClosedObject(input.oxid, ["head", "tree"], "oxid");
  assertHex(input.oxid.head, 40, "oxid.head");
  assertHex(input.oxid.tree, 40, "oxid.tree");

  assertClosedObject(input.portal, Object.keys(PORTAL), "portal");
  for (const [key, expected] of Object.entries(PORTAL)) {
    if (input.portal[key] !== expected) throw new Error(`portal.${key} is invalid`);
  }
  assertClosedObject(input.deployment, Object.keys(DEPLOYMENT), "deployment");
  for (const [key, expected] of Object.entries(DEPLOYMENT)) {
    if (input.deployment[key] !== expected) throw new Error(`deployment.${key} is invalid`);
  }

  assertClosedObject(input.platform, ["kind", "osFamily", "apiLevel", "architecture"], "platform");
  if (!new Set(["android_emulator", "ios_simulator"]).has(input.platform.kind)) {
    throw new Error("platform.kind is invalid");
  }
  const expectedFamily = input.platform.kind === "android_emulator" ? "android" : "ios";
  if (input.platform.osFamily !== expectedFamily) throw new Error("platform.osFamily is invalid");
  if (!Number.isSafeInteger(input.platform.apiLevel) || input.platform.apiLevel < 1
      || input.platform.apiLevel > 99) throw new Error("platform.apiLevel is invalid");
  if (!new Set(["arm64", "x86_64"]).has(input.platform.architecture)) {
    throw new Error("platform.architecture is invalid");
  }
  assertHex(input.artifactSha256, 64, "artifactSha256");

  if (!Array.isArray(input.scenarios)
      || input.scenarios.length !== expectedScenarios().length) {
    throw new Error("scenarios are incomplete");
  }
  input.scenarios.forEach((scenario, index) => {
    assertClosedObject(scenario, ["name", "passed", "counterDelta"], `scenarios[${index}]`);
    if (typeof scenario.name !== "string" || typeof scenario.passed !== "boolean") {
      throw new Error(`scenarios[${index}] is invalid`);
    }
    assertCounters(scenario.counterDelta, `scenarios[${index}].counterDelta`);
  });
  assertCounters(input.totalCounters, "totalCounters");

  assertClosedObject(input.offer, OFFER_KEYS, "offer");
  for (const key of OFFER_KEYS) {
    if (new Set(["secretCallsBeforeConsent", "issuerResolutionCallsBeforeConsent"]).has(key)) {
      if (!Number.isSafeInteger(input.offer[key]) || input.offer[key] < 0) {
        throw new Error(`offer.${key} is invalid`);
      }
    } else if (typeof input.offer[key] !== "boolean") {
      throw new Error(`offer.${key} must be boolean`);
    }
  }
  assertBooleanObject(input.issuance, ISSUANCE_KEYS, "issuance");
  assertBooleanObject(input.storage, STORAGE_KEYS, "storage");
  assertBooleanObject(input.restart, RESTART_KEYS, "restart");
  assertBooleanObject(input.cleanup, CLEANUP_KEYS, "cleanup");
  assertSecretFreeStrings(input);
}

function deriveAcceptance(input) {
  const offerBooleans = OFFER_KEYS.filter((key) => typeof input.offer[key] === "boolean");
  const exactOfferAndPreview = allTrue(input.offer, offerBooleans);
  const preConsentBoundary = input.offer.metadataPreviewCallsExpected === true
    && input.offer.refusalDeltaExact === true
    && input.offer.secretCallsBeforeConsent === 0
    && input.offer.issuerResolutionCallsBeforeConsent === 0;
  const issuance = allTrue(input.issuance, ISSUANCE_KEYS);
  const encryptedPersistence = allTrue(input.storage, STORAGE_KEYS);
  const restartAndReverification = allTrue(input.restart, RESTART_KEYS);
  const exactScenarios = exactScenarioDeltas(input.scenarios);
  const exactTotals = exactCounters(input.totalCounters, expectedTotals());
  const cleanup = allTrue(input.cleanup, CLEANUP_KEYS);
  const virtualTargetOnly = input.cleanup.virtualTargetOnly === true;
  const secretFreeEvidence = true;
  const accepted = [
    exactOfferAndPreview,
    preConsentBoundary,
    issuance,
    encryptedPersistence,
    restartAndReverification,
    exactScenarios,
    exactTotals,
    cleanup,
    virtualTargetOnly,
    secretFreeEvidence,
  ].every(Boolean);
  return {
    exactOfferAndPreview,
    preConsentBoundary,
    issuance,
    encryptedPersistence,
    restartAndReverification,
    exactScenarioDeltas: exactScenarios,
    exactTotalCounters: exactTotals,
    cleanup,
    virtualTargetOnly,
    secretFreeEvidence,
    accepted,
  };
}

export function buildEvidence(input) {
  validateMeasurements(input);
  const evidence = {
    schema: SCHEMA,
    oxid: structuredClone(input.oxid),
    portal: structuredClone(input.portal),
    deployment: structuredClone(input.deployment),
    platform: structuredClone(input.platform),
    artifact: { sha256: input.artifactSha256 },
    scenarios: structuredClone(input.scenarios),
    measurements: { totalCounters: structuredClone(input.totalCounters) },
    observations: {
      offer: structuredClone(input.offer),
      issuance: structuredClone(input.issuance),
      storage: structuredClone(input.storage),
      restart: structuredClone(input.restart),
      cleanup: structuredClone(input.cleanup),
    },
    acceptance: deriveAcceptance(input),
  };
  assertSecretFreeStrings(evidence);
  return evidence;
}

export function validateEvidence(evidence, { requireAccepted = false } = {}) {
  assertClosedObject(evidence, [
    "schema", "oxid", "portal", "deployment", "platform", "artifact", "scenarios",
    "measurements", "observations", "acceptance",
  ], "evidence");
  if (evidence.schema !== SCHEMA) throw new Error("evidence schema is invalid");
  assertClosedObject(evidence.artifact, ["sha256"], "artifact");
  assertClosedObject(evidence.measurements, ["totalCounters"], "measurements");
  assertClosedObject(evidence.observations, ["offer", "issuance", "storage", "restart", "cleanup"], "observations");
  assertClosedObject(evidence.acceptance, ACCEPTANCE_KEYS, "acceptance");
  for (const key of ACCEPTANCE_KEYS) {
    if (typeof evidence.acceptance[key] !== "boolean") throw new Error(`acceptance.${key} must be boolean`);
  }
  const input = {
    oxid: evidence.oxid,
    portal: evidence.portal,
    deployment: evidence.deployment,
    platform: evidence.platform,
    artifactSha256: evidence.artifact.sha256,
    scenarios: evidence.scenarios,
    totalCounters: evidence.measurements.totalCounters,
    offer: evidence.observations.offer,
    issuance: evidence.observations.issuance,
    storage: evidence.observations.storage,
    restart: evidence.observations.restart,
    cleanup: evidence.observations.cleanup,
  };
  validateMeasurements(input);
  const expectedAcceptance = deriveAcceptance(input);
  if (JSON.stringify(evidence.acceptance) !== JSON.stringify(expectedAcceptance)) {
    throw new Error("acceptance is not derived from measurements");
  }
  if (requireAccepted && evidence.acceptance.accepted !== true) throw new Error("evidence is not accepted");
  assertSecretFreeStrings(evidence);
  return evidence;
}

export function publishEvidence(output, input) {
  if (!path.isAbsolute(output)) throw new Error("output path must be absolute");
  const parent = path.dirname(output);
  const parentStat = fs.lstatSync(parent);
  if (!parentStat.isDirectory() || parentStat.isSymbolicLink()) throw new Error("output parent is invalid");
  try {
    fs.lstatSync(output);
    throw new Error("evidence output already exists or is occupied");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  const evidence = validateEvidence(buildEvidence(input), { requireAccepted: true });
  const temporary = path.join(parent, `.evidence.${process.pid}.${Date.now()}.${Math.random().toString(16).slice(2)}`);
  let descriptor;
  try {
    descriptor = fs.openSync(temporary, fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY, 0o600);
    fs.writeFileSync(descriptor, `${JSON.stringify(evidence)}\n`, { encoding: "utf8" });
    fs.fsyncSync(descriptor);
    fs.closeSync(descriptor);
    descriptor = undefined;
    fs.chmodSync(temporary, 0o600);
    fs.linkSync(temporary, output);
    fs.chmodSync(output, 0o600);
  } finally {
    if (descriptor !== undefined) fs.closeSync(descriptor);
    try { fs.unlinkSync(temporary); } catch (error) { if (error?.code !== "ENOENT") throw error; }
  }
  validateEvidence(JSON.parse(fs.readFileSync(output, "utf8")), { requireAccepted: true });
  if ((fs.statSync(output).mode & 0o777) !== 0o600) throw new Error("evidence mode is invalid");
  return evidence;
}

function main() {
  const args = process.argv.slice(2);
  if (args.length !== 4 || args[0] !== "--input" || args[2] !== "--output") {
    throw new Error("invalid arguments");
  }
  const inputPath = args[1];
  const outputPath = args[3];
  if (!path.isAbsolute(inputPath) || !path.isAbsolute(outputPath)) throw new Error("paths must be absolute");
  const inputStat = fs.lstatSync(inputPath);
  if (!inputStat.isFile() || inputStat.isSymbolicLink() || inputStat.size > 1024 * 1024) {
    throw new Error("input is invalid");
  }
  publishEvidence(outputPath, JSON.parse(fs.readFileSync(inputPath, "utf8")));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch {
    process.stderr.write("portal-virtual-mobile-evidence: FAIL\n");
    process.exitCode = 1;
  }
}
