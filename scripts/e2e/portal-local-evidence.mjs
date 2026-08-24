#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const SOURCE_LOCK_PATH = path.join(
  REPOSITORY_ROOT,
  "fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/source-lock.json",
);
const SOURCE_LOCK = JSON.parse(fs.readFileSync(SOURCE_LOCK_PATH, "utf8"));
const MAX_EVIDENCE_BYTES = 16_384;
const SHA_PATTERN = /^[0-9a-f]{40}$/u;
const DIGEST_PATTERN = /^[0-9a-f]{64}$/u;
const PORTAL_HELPER_COMMIT = "8915760a4523d282fa07d45a48b7f58e4287bb54";
const PORTAL_HELPER_TREE = "1317e109cf0792c0e1d7c8f9e2b8857251f6e92d";
const SECRET_SENTINEL = /openid-credential-offer|credential_offer|pre-authorized|access[_-]?token|c_nonce|authorization\s*[:=]\s*bearer|eyJ|did:|https?:\/\/|AB1234567|\bJohn\b|\bDoe\b|private.?parts|signed.?bytes|detached.?proof|portal-offer-capability|emulator-[0-9]+|[0-9A-F]{8}-[0-9A-F-]{27}/iu;

const HEADLESS_ACCEPTANCE_KEYS = [
  "confirmationRequired",
  "encryptedPersistence",
  "exactBundleImported",
  "managedAuthenticationProof",
  "mockKycApproved",
  "newProcessRestore",
  "refusalWithoutSecretCalls",
  "replayRejected",
  "reverified",
  "separateJubjubAssertionBinding",
  "sharedMidnightIdentityUnchanged",
];
const MOBILE_COMMON_ACCEPTANCE_KEYS = [
  "developmentCustodyReactivated",
  "encryptedPersistence",
  "exactBundleImported",
  "explicitConsent",
  "managedAuthenticationProof",
  "mockKycApproved",
  "oneItemStrictRouter",
  "processRestart",
  "reverified",
  "secretFreeEvidence",
  "separateJubjubAssertionBinding",
  "strictFinalExchange",
  "timeoutDenied",
  "unavailableDenied",
  "warmColdCustomScheme",
];
const IOS_ACCEPTANCE_KEYS = [...MOBILE_COMMON_ACCEPTANCE_KEYS, "cameraUnavailable"];
const ANDROID_ACCEPTANCE_KEYS = [
  ...MOBILE_COMMON_ACCEPTANCE_KEYS,
  "clockSynchronized",
  "malformedDenied",
  "noEmulatorAlias",
  "qemuVerified",
];

function fail(phase) {
  throw new Error(`portal-local-evidence: FAIL phase=${phase}`);
}

function exactKeys(value, expected, phase) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) fail(phase);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    fail(phase);
  }
}

function readEvidence(filePath, phase) {
  let stat;
  try {
    stat = fs.lstatSync(filePath);
  } catch {
    fail(`${phase}-file`);
  }
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > MAX_EVIDENCE_BYTES) {
    fail(`${phase}-file`);
  }
  const raw = fs.readFileSync(filePath, "utf8");
  if (SECRET_SENTINEL.test(raw)) fail(`${phase}-sentinel`);
  try {
    return JSON.parse(raw);
  } catch {
    fail(`${phase}-json`);
  }
}

function validateSourceLock() {
  exactKeys(
    SOURCE_LOCK,
    [
      "integrationCommit",
      "integrationTree",
      "portalPrHead",
      "profileSourceCommit",
      "provenancePath",
      "provenanceSha256",
      "schema",
    ],
    "source-lock-keys",
  );
  if (
    SOURCE_LOCK.schema !== "oxid-portal-source-lock-v2" ||
    !SHA_PATTERN.test(SOURCE_LOCK.integrationCommit) ||
    !SHA_PATTERN.test(SOURCE_LOCK.integrationTree) ||
    !SHA_PATTERN.test(SOURCE_LOCK.portalPrHead) ||
    !SHA_PATTERN.test(SOURCE_LOCK.profileSourceCommit) ||
    !DIGEST_PATTERN.test(SOURCE_LOCK.provenanceSha256) ||
    SOURCE_LOCK.provenancePath !== "openid4vci-final/provenance.json"
  ) {
    fail("source-lock");
  }
}

function validateCommon(document, expectedHead, phase) {
  if (!SHA_PATTERN.test(expectedHead)) fail("expected-head");
  exactKeys(document.oxid, ["head"], `${phase}-oxid`);
  if (document.oxid.head !== expectedHead) fail(`${phase}-head`);
  exactKeys(
    document.portal,
    ["helperCommit", "helperTree", "integrationCommit", "integrationTree", "prHead", "profileSourceCommit", "provenanceSha256"],
    `${phase}-portal`,
  );
  const expectedPortal = {
    helperCommit: PORTAL_HELPER_COMMIT,
    helperTree: PORTAL_HELPER_TREE,
    integrationCommit: SOURCE_LOCK.integrationCommit,
    integrationTree: SOURCE_LOCK.integrationTree,
    prHead: SOURCE_LOCK.portalPrHead,
    profileSourceCommit: SOURCE_LOCK.profileSourceCommit,
    provenanceSha256: SOURCE_LOCK.provenanceSha256,
  };
  for (const [key, expected] of Object.entries(expectedPortal)) {
    if (document.portal[key] !== expected) fail(`${phase}-portal-${key}`);
  }
}

function validateAcceptance(acceptance, expectedKeys, phase) {
  exactKeys(acceptance, expectedKeys, `${phase}-acceptance`);
  if (!Object.values(acceptance).every((value) => value === true)) {
    fail(`${phase}-acceptance`);
  }
}

function validateHeadless(filePath, expectedHead) {
  const document = readEvidence(filePath, "headless");
  exactKeys(document, ["acceptance", "oxid", "portal", "schema"], "headless-keys");
  if (document.schema !== "oxid-portal-headless-evidence-v1") fail("headless-schema");
  validateCommon(document, expectedHead, "headless");
  validateAcceptance(document.acceptance, HEADLESS_ACCEPTANCE_KEYS, "headless");
  return document;
}

function validatePlatformText(value, pattern, phase) {
  if (typeof value !== "string" || !pattern.test(value) || value === "unknown") fail(phase);
}

function validateMobile(filePath, expectedHead, platform, standardSmokeRequired = true) {
  const phase = platform;
  const document = readEvidence(filePath, phase);
  exactKeys(document, ["acceptance", "oxid", "platform", "portal", "schema"], `${phase}-keys`);
  if (document.schema !== "oxid-portal-mobile-evidence-v1") fail(`${phase}-schema`);
  validateCommon(document, expectedHead, phase);
  const baseAcceptance = platform === "ios" ? IOS_ACCEPTANCE_KEYS : ANDROID_ACCEPTANCE_KEYS;
  const expectedAcceptance = standardSmokeRequired
    ? [...baseAcceptance, "standardSmoke"]
    : baseAcceptance;
  validateAcceptance(document.acceptance, expectedAcceptance, phase);

  if (platform === "ios") {
    exactKeys(
      document.platform,
      ["applicationId", "kind", "model", "os", "profile"],
      "ios-platform",
    );
    if (
      document.platform.kind !== "ios_simulator" ||
      document.platform.applicationId !== "io.medianox.oxid" ||
      document.platform.profile !== "standalone-local-development-portal"
    ) {
      fail("ios-platform");
    }
    validatePlatformText(document.platform.model, /^iPhone [A-Za-z0-9 +().-]{1,64}$/u, "ios-model");
    validatePlatformText(
      document.platform.os,
      /^com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9-]+$/u,
      "ios-os",
    );
  } else if (platform === "android") {
    exactKeys(
      document.platform,
      [
        "adbReversePorts",
        "apiLevel",
        "applicationId",
        "clockSkewSeconds",
        "kind",
        "model",
        "os",
        "profile",
      ],
      "android-platform",
    );
    if (
      document.platform.kind !== "android_qemu_emulator" ||
      document.platform.applicationId !== "io.medianox.oxid" ||
      document.platform.profile !== "standalone-local-development-portal" ||
      !Number.isInteger(document.platform.clockSkewSeconds) ||
      document.platform.clockSkewSeconds < -2 ||
      document.platform.clockSkewSeconds > 2 ||
      !Array.isArray(document.platform.adbReversePorts) ||
      JSON.stringify(document.platform.adbReversePorts) !== JSON.stringify([6300, 8088, 9944, 18090, 18091, 18093])
    ) {
      fail("android-platform");
    }
    validatePlatformText(document.platform.model, /^[A-Za-z0-9._() +:-]{1,96}$/u, "android-model");
    validatePlatformText(document.platform.os, /^[0-9][A-Za-z0-9._ -]{0,31}$/u, "android-os");
    validatePlatformText(document.platform.apiLevel, /^[0-9]{1,3}$/u, "android-api");
  } else {
    fail("platform");
  }
  return document;
}

function parseNamedArguments(args) {
  const values = new Map();
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    const value = args[index + 1];
    if (!name?.startsWith("--") || value === undefined || values.has(name)) fail("arguments");
    values.set(name, value);
  }
  return values;
}

function validateArgumentNames(values, expected) {
  const actual = [...values.keys()].sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((name, index) => name !== wanted[index])) {
    fail("arguments");
  }
}

function required(values, name) {
  const value = values.get(name);
  if (!value) fail("arguments");
  return value;
}

function attestStandardSmoke(filePath, expectedHead, platform) {
  const document = validateMobile(filePath, expectedHead, platform, false);
  document.acceptance.standardSmoke = true;
  const candidate = path.join(path.dirname(filePath), `.${path.basename(filePath)}.tmp.${process.pid}`);
  try {
    const descriptor = fs.openSync(candidate, "wx", 0o600);
    try {
      fs.writeFileSync(descriptor, `${JSON.stringify(document)}\n`, "utf8");
      fs.fsyncSync(descriptor);
    } finally {
      fs.closeSync(descriptor);
    }
    fs.renameSync(candidate, filePath);
  } catch (error) {
    try {
      fs.rmSync(candidate, { force: true });
    } catch {
      // Preserve the original error and never expose evidence contents.
    }
    throw error;
  }
  validateMobile(filePath, expectedHead, platform, true);
}

function main() {
  validateSourceLock();
  const [command, ...rest] = process.argv.slice(2);
  const values = parseNamedArguments(rest);
  if (command === "validate") {
    validateArgumentNames(values, ["--head", "--headless", "--ios", "--android"]);
    const head = required(values, "--head");
    validateHeadless(required(values, "--headless"), head);
    validateMobile(required(values, "--ios"), head, "ios", true);
    validateMobile(required(values, "--android"), head, "android", true);
    process.stdout.write(`portal-local-evidence: PASS head=${head}\n`);
    return;
  }
  if (command === "attest-standard-smoke") {
    validateArgumentNames(values, ["--head", "--platform", "--evidence"]);
    const head = required(values, "--head");
    const platform = required(values, "--platform");
    if (platform !== "ios" && platform !== "android") fail("platform");
    attestStandardSmoke(required(values, "--evidence"), head, platform);
    process.stdout.write(`portal-local-evidence: ATTESTED platform=${platform} head=${head}\n`);
    return;
  }
  fail("arguments");
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : "portal-local-evidence: FAIL phase=unknown";
  process.stderr.write(`${message.startsWith("portal-local-evidence:") ? message : "portal-local-evidence: FAIL phase=io"}\n`);
  process.exitCode = 1;
}
