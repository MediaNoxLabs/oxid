#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { SingleUseOfferHandoff, preparePrivateCapabilityPaths } from "./portal-mobile-offer-handoff.mjs";

const PORTAL_INTEGRATION_COMMIT = "925ec8d04882eabd4ac7b784c70fc2f0c152faae";
const PORTAL_INTEGRATION_TREE = "58b4597524f88a0ae2253439a44dab0dc60cbb6f";
const PORTAL_PR_HEAD = "9c82db23eabe8b6d758b2731f2225910ea627c14";
const PORTAL_PROFILE_SOURCE = "76e8edf394a4cb37ca822037272d543c68f25f71";
const PORTAL_PROVENANCE_SHA256 = "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87";
const PORTAL_PROXY_PORT = 18090;
const CONTROL_PORT = 18091;
// Fixed, non-secret OS trigger shared by the iOS simulator and Android emulator
// standalone-portal builds (issue #124 / ADR-0103). `simctl openurl` and
// `am start -d` deliver only this literal, never the real offer, so the
// pre-authorized grant never enters host/device argv, OS URL/intent state, or
// retained logs/evidence. The app's named worker fetches the offer through the
// bounded loopback-only /offer endpoint after recognizing this exact literal.
const MOBILE_TEST_OFFER_TRIGGER = "openid-credential-offer://standalone-portal-test-fetch";
const HOLDER_RESOLVER_PORT = 18092;
const ISSUER_RESOLVER_PROXY_PORT = 18093;
const LOCAL_ISSUER_RESOLVER_ORIGIN = `http://127.0.0.1:${ISSUER_RESOLVER_PROXY_PORT}`;
const LOCAL_ISSUER_ORIGIN = `http://127.0.0.1:${PORTAL_PROXY_PORT}`;
const MAX_CONTROL_BODY = 2 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;
const CHILD_COMMAND_TIMEOUT_MS = 10 * 60_000;
const HOST_COMMAND_TIMEOUT_MS = 30_000;

const portalTree = process.env.PORTAL_INTEGRATION_CHECKOUT;
const stateDirectory = process.env.OXID_PORTAL_MOBILE_STATE_DIR;
const readyFifo = process.env.OXID_PORTAL_MOBILE_READY_FIFO;
const composeProjectName = process.env.COMPOSE_PROJECT_NAME;
const stackEnvFile = process.env.STACK_ENV_FILE;
const localHeadlessScript = process.env.OXID_LOCAL_HEADLESS_SCRIPT;
const repositoryRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const xcodeDeveloperDirectory = process.env.OXID_XCODE_DEVELOPER_DIR;
const mobilePlatform = process.env.OXID_PORTAL_MOBILE_PLATFORM;
const capabilityFifo = process.env.OXID_PORTAL_MOBILE_CAPABILITY_FIFO;
const portalProfile = process.env.OXID_MOBILE_PORTAL_PROFILE ?? "local";
const suppliedPublicOrigin = process.env.OXID_BUILD_PORTAL_PUBLIC_ORIGIN ?? "";
function exactMagicDnsOrigin(value) {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "https:"
      && parsed.username === "" && parsed.password === ""
      && parsed.port === "9443" && parsed.pathname === "/"
      && parsed.search === "" && parsed.hash === ""
      && parsed.hostname.endsWith(".ts.net")
      && parsed.hostname !== "ts.net"
      && parsed.hostname.split(".").every((label) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(label))
      && parsed.origin === value;
  } catch {
    return false;
  }
}
const iosTailnetProfile = portalProfile === "tailnet-ios-simulator";
const androidPhysicalTailnetProfile = portalProfile === "tailnet-android-physical";
const tailnetProfile = iosTailnetProfile || androidPhysicalTailnetProfile;
const portalProfileValid = portalProfile === "local"
  || (iosTailnetProfile && mobilePlatform === "ios" && exactMagicDnsOrigin(suppliedPublicOrigin))
  || (androidPhysicalTailnetProfile && mobilePlatform === "android"
    && suppliedPublicOrigin === "https://yuriys-macbook-pro.taila4adff.ts.net:9443");
const ISSUER_ORIGIN = tailnetProfile ? suppliedPublicOrigin : LOCAL_ISSUER_ORIGIN;
const ISSUER_RESOLVER_ORIGIN = tailnetProfile
  ? `${suppliedPublicOrigin}/issuer-resolver`
  : LOCAL_ISSUER_RESOLVER_ORIGIN;
const androidCapabilityFifoValid = mobilePlatform !== "android"
  || (capabilityFifo && path.isAbsolute(capabilityFifo)
    && fs.existsSync(capabilityFifo) && fs.lstatSync(capabilityFifo).isFIFO()
    && !fs.lstatSync(capabilityFifo).isSymbolicLink());
if (!portalTree || !path.isAbsolute(portalTree) || !stateDirectory || !path.isAbsolute(stateDirectory)
    || !readyFifo || !path.isAbsolute(readyFifo)
    || !new Set(["ios", "android"]).has(mobilePlatform) || !portalProfileValid || !androidCapabilityFifoValid
    || !/^[a-z0-9][a-z0-9_-]+$/.test(composeProjectName ?? "")
    || !stackEnvFile || !path.isAbsolute(stackEnvFile)
    || !localHeadlessScript || !path.isAbsolute(localHeadlessScript)) {
  process.stderr.write("portal-mobile-support: FAIL phase=configuration\n");
  process.exit(2);
}

fs.mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });
const privateLogPath = path.join(stateDirectory, "support-private.log");
const privateLog = fs.openSync(privateLogPath, "a", 0o600);
const manifestPath = path.join(stateDirectory, "deployment.json");
const readyPath = path.join(stateDirectory, "ready.json");

let phase = "startup";
let holderDocument = null;
let holderGeneration = 0;
let iosDevice = null;
const offerHandoff = new SingleUseOfferHandoff();
let offerArming = false;
let iosCapabilityPath = null;
let iosCapabilityCandidatePath = null;
let proxyMode = "normal";
let complete = false;
let cleanupStarted = false;
let signalExitCode = 0;
const heldSockets = new Set();
const proxiedSockets = new Set();
const counters = {
  authorizationMetadata: 0,
  credential: 0,
  issuerMetadata: 0,
  issuerResolution: 0,
  issuerResolutionSuccess: 0,
  kyc: 0,
  nonce: 0,
  other: 0,
  token: 0,
};

function appendPrivate(message) {
  fs.writeSync(privateLog, `${new Date().toISOString()} ${message}\n`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function runLogged(command, args, cwd = portalTree, timeoutMs = CHILD_COMMAND_TIMEOUT_MS) {
  const result = spawnSync(command, args, {
    cwd,
    env: process.env,
    stdio: ["ignore", privateLog, privateLog],
    killSignal: "SIGKILL",
    timeout: timeoutMs,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed in ${phase}`);
  }
}

function runSilent(command, args, cwd = portalTree) {
  if (!xcodeDeveloperDirectory || !path.isAbsolute(xcodeDeveloperDirectory)) {
    throw new Error(`selected Xcode developer directory unavailable in ${phase}`);
  }
  const hostEnvironment = { ...process.env, DEVELOPER_DIR: xcodeDeveloperDirectory };
  for (const name of ["SDKROOT", "CC", "CXX", "LD", "AR", "NIX_CFLAGS_COMPILE", "NIX_LDFLAGS"]) {
    delete hostEnvironment[name];
  }
  hostEnvironment.PATH = `/usr/bin:/bin:/usr/sbin:/sbin:${process.env.PATH ?? ""}`;
  const result = spawnSync(command, args, {
    cwd,
    env: hostEnvironment,
    stdio: "ignore",
    killSignal: "SIGKILL",
    timeout: HOST_COMMAND_TIMEOUT_MS,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${path.basename(command)} failed in ${phase}`);
  }
}

function runHostCaptured(command, args, cwd = portalTree) {
  if (!xcodeDeveloperDirectory || !path.isAbsolute(xcodeDeveloperDirectory)) {
    throw new Error(`selected Xcode developer directory unavailable in ${phase}`);
  }
  const hostEnvironment = { ...process.env, DEVELOPER_DIR: xcodeDeveloperDirectory };
  for (const name of ["SDKROOT", "CC", "CXX", "LD", "AR", "NIX_CFLAGS_COMPILE", "NIX_LDFLAGS"]) {
    delete hostEnvironment[name];
  }
  hostEnvironment.PATH = `/usr/bin:/bin:/usr/sbin:/sbin:${process.env.PATH ?? ""}`;
  const result = spawnSync(command, args, {
    cwd,
    env: hostEnvironment,
    encoding: "utf8",
    killSignal: "SIGKILL",
    maxBuffer: 64 * 1024,
    timeout: HOST_COMMAND_TIMEOUT_MS,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${path.basename(command)} failed in ${phase}`);
  }
  return result.stdout.trim();
}

function runCaptured(command, args, cwd = portalTree, timeoutMs = CHILD_COMMAND_TIMEOUT_MS) {
  const result = spawnSync(command, args, {
    cwd,
    env: process.env,
    encoding: "utf8",
    killSignal: "SIGKILL",
    maxBuffer: 1024 * 1024,
    timeout: timeoutMs,
  });
  if (result.stderr) fs.writeSync(privateLog, result.stderr);
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed in ${phase}`);
  }
  return result.stdout.trim();
}

function readBounded(request, maximum = MAX_CONTROL_BODY) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let length = 0;
    request.on("data", (chunk) => {
      length += chunk.length;
      if (length > maximum) {
        reject(new Error("request too large"));
        request.destroy();
        return;
      }
      chunks.push(chunk);
    });
    request.on("end", () => resolve(Buffer.concat(chunks)));
    request.on("error", reject);
  });
}

function sendJson(response, status, value) {
  const bytes = Buffer.from(JSON.stringify(value));
  response.writeHead(status, {
    "Cache-Control": "no-store",
    "Content-Length": bytes.length,
    "Content-Type": "application/json",
  });
  response.end(bytes);
}

function transformStoredDid(bytes) {
  const stored = JSON.parse(bytes.toString("utf8"));
  if (stored?.schemaVersion !== 1 || !Array.isArray(stored.records) || stored.records.length === 0) {
    throw new Error("invalid public DID store");
  }
  const source = stored.records.at(-1)?.resolution?.document;
  if (!source || typeof source.id !== "string" || !Array.isArray(source.verificationMethods) || !Array.isArray(source.relationships)) {
    throw new Error("invalid public DID record");
  }
  const methods = source.verificationMethods.map((method) => {
    const key = method.publicKeyJwk;
    if (!method.id || !method.controller || !key?.keyType || !key?.curve || !key?.x) {
      throw new Error("invalid public DID method");
    }
    const publicKeyJwk = { kty: key.keyType, crv: key.curve, x: key.x };
    if (key.y !== null && key.y !== undefined) publicKeyJwk.y = key.y;
    return {
      id: method.id,
      type: "JsonWebKey",
      controller: method.controller,
      publicKeyJwk,
    };
  });
  const document = { id: source.id, verificationMethod: methods };
  for (const relationship of source.relationships) {
    if (typeof relationship.relationship !== "string" || !Array.isArray(relationship.methodIds)) {
      throw new Error("invalid public DID relationship");
    }
    document[relationship.relationship] = relationship.methodIds;
  }
  return document;
}

function pathCounter(requestPath) {
  if (requestPath === "/.well-known/openid-credential-issuer") return "issuerMetadata";
  if (requestPath === "/.well-known/oauth-authorization-server") return "authorizationMetadata";
  if (requestPath === "/api/issuer/token") return "token";
  if (requestPath === "/api/issuer/nonce") return "nonce";
  if (requestPath === "/api/issuer/credentials") return "credential";
  if (requestPath.startsWith("/api/issuer/kyc-sessions")) return "kyc";
  return "other";
}

const proxyServer = http.createServer((request, response) => {
  let parsed;
  try {
    parsed = new URL(request.url, ISSUER_ORIGIN);
  } catch {
    response.destroy();
    return;
  }
  counters[pathCounter(parsed.pathname)] += 1;
  if (proxyMode === "unavailable") {
    request.socket.destroy();
    return;
  }
  if (proxyMode === "timeout") {
    heldSockets.add(request.socket);
    request.socket.once("close", () => heldSockets.delete(request.socket));
    setTimeout(() => request.socket.destroy(), 25_000).unref();
    return;
  }
  const headers = { ...request.headers, host: "127.0.0.1:8090" };
  const upstream = http.request({
    host: "127.0.0.1",
    port: 8090,
    method: request.method,
    path: request.url,
    headers,
  }, (upstreamResponse) => {
    response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  proxiedSockets.add(upstream);
  upstream.once("close", () => proxiedSockets.delete(upstream));
  upstream.on("error", () => response.destroy());
  request.pipe(upstream);
});

const issuerResolverProxy = http.createServer((request, response) => {
  if (request.method !== "POST"
      || !new Set(["/resolve", "/issuer-resolver/resolve"]).has(request.url)) {
    return sendJson(response, 404, { error: "not_found" });
  }
  counters.issuerResolution += 1;
  const upstream = http.request({
    host: "127.0.0.1",
    port: 9092,
    method: "POST",
    path: "/resolve",
    headers: { ...request.headers, host: "127.0.0.1:9092" },
  }, (upstreamResponse) => {
    if ((upstreamResponse.statusCode ?? 500) >= 200 && (upstreamResponse.statusCode ?? 500) < 300) {
      counters.issuerResolutionSuccess += 1;
    }
    response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  proxiedSockets.add(upstream);
  upstream.once("close", () => proxiedSockets.delete(upstream));
  upstream.on("error", () => response.destroy());
  request.pipe(upstream);
});

const holderServer = http.createServer(async (request, response) => {
  if (request.method === "GET" && request.url === "/health") {
    return sendJson(response, 200, { ok: true });
  }
  if (request.method !== "POST" || request.url !== "/resolve") {
    return sendJson(response, 404, { error: "not_found" });
  }
  try {
    const requested = JSON.parse((await readBounded(request, 64 * 1024)).toString("utf8"));
    if (holderDocument && requested?.did === holderDocument.id) {
      return sendJson(response, 200, {
        didDocument: holderDocument,
        didDocumentMetadata: { deactivated: false },
        didResolutionMetadata: { contentType: "application/did+ld+json" },
      });
    }
    return sendJson(response, 404, {
      didDocument: null,
      didDocumentMetadata: { deactivated: false },
      didResolutionMetadata: { error: "notFound" },
    });
  } catch {
    return sendJson(response, 400, { error: "invalid_request" });
  }
});

let completeResolve;
const completion = new Promise((resolve) => { completeResolve = resolve; });
const controlServer = http.createServer(async (request, response) => {
  try {
    if (offerHandoff.handle(request, response)) return;
    if (request.method === "GET" && request.url === "/health") {
      return sendJson(response, 200, { ok: true });
    }
    if (request.method === "GET" && request.url === "/counters") {
      return sendJson(response, 200, counters);
    }
    if (request.method === "GET" && request.url === "/holder-generation") {
      return sendJson(response, 200, { generation: holderGeneration });
    }
    if (request.method === "POST" && request.url === "/holder") {
      holderDocument = transformStoredDid(await readBounded(request));
      holderGeneration += 1;
      return sendJson(response, 200, { generation: holderGeneration });
    }
    if (request.method === "POST" && request.url === "/ios-device") {
      const candidate = (await readBounded(request, 64)).toString("utf8");
      if (!/^[0-9A-F]{8}(?:-[0-9A-F]{4}){3}-[0-9A-F]{12}$/.test(candidate)) {
        return sendJson(response, 400, { error: "invalid_device" });
      }
      iosDevice = candidate;
      return sendJson(response, 200, { ok: true });
    }
    if (request.method === "POST" && request.url === "/arm-android-offer") {
      if (mobilePlatform !== "android") {
        return sendJson(response, 404, { error: "not_found" });
      }
      await armPortalOffer(provisionAndroidCapability);
      return sendJson(response, 200, { armed: true });
    }
    if (request.method === "POST" && request.url === "/deliver-ios") {
      const delivery = (await readBounded(request, 32)).toString("utf8");
      if (!iosDevice || !new Set(["real", "real-cold"]).has(delivery)) {
        return sendJson(response, 400, { error: "invalid_delivery" });
      }
      if (delivery === "real-cold") {
        try {
          runSilent("/usr/bin/xcrun", ["simctl", "terminate", iosDevice, "io.medianox.oxid"]);
        } catch {}
        // `simctl terminate` can return before SpringBoard completes the
        // process transition. Opening immediately can deliver the trigger to
        // the dying process and lose the cold-start handoff.
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      await armPortalOffer(provisionIosCapability);
      // Never pass `offer` here: it is the real single-use pre-authorized
      // grant and must not appear in this host process's argv. The fixed,
      // non-secret trigger is the only value the OS/host ever sees; the app
      // fetches the real offer itself over a loopback GET to /offer below.
      runSilent("/usr/bin/xcrun", ["simctl", "openurl", iosDevice, MOBILE_TEST_OFFER_TRIGGER]);
      return sendJson(response, 200, { kind: delivery === "real-cold" ? "cold" : "warm" });
    }
    if (request.method === "POST" && request.url === "/proxy-mode") {
      const mode = (await readBounded(request, 32)).toString("utf8");
      if (!new Set(["normal", "timeout", "unavailable"]).has(mode)) {
        return sendJson(response, 400, { error: "invalid_mode" });
      }
      proxyMode = mode;
      if (mode === "normal") {
        for (const socket of heldSockets) socket.destroy();
      }
      return sendJson(response, 200, { mode });
    }
    if (request.method === "POST" && request.url === "/complete") {
      complete = true;
      sendJson(response, 200, { ok: true });
      completeResolve();
      return;
    }
    return sendJson(response, 404, { error: "not_found" });
  } catch {
    return sendJson(response, 400, { error: "invalid_request" });
  }
});

function listen(server, port) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => resolve());
  });
}

async function requestJson(url, options = {}) {
  const response = await fetch(url, {
    ...options,
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status} in ${phase}`);
  return response.json();
}

function provisionIosCapability(capability) {
  if (mobilePlatform !== "ios" || !iosDevice) {
    throw new Error("iOS capability delivery is unavailable");
  }
  const container = runHostCaptured("/usr/bin/xcrun", [
    "simctl", "get_app_container", iosDevice, "io.medianox.oxid", "data",
  ]);
  if (!path.isAbsolute(container)) throw new Error("iOS app data container is invalid");
  const directory = path.join(container, "Library", "Application Support", "io.medianox.oxid");
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  const { target, candidate } = preparePrivateCapabilityPaths(
    directory,
    "portal-offer.capability",
    `.portal-offer.capability.tmp-${process.pid}`,
  );
  iosCapabilityPath = target;
  iosCapabilityCandidatePath = candidate;
  fs.writeFileSync(candidate, capability, { mode: 0o600, flag: "wx" });
  fs.renameSync(candidate, target);
  iosCapabilityCandidatePath = null;
}

function provisionAndroidCapability(capability) {
  if (mobilePlatform !== "android" || !androidCapabilityFifoValid) {
    throw new Error("Android capability FIFO is unavailable");
  }
  fs.writeFileSync(capabilityFifo, capability);
}

async function armPortalOffer(provisionCapability) {
  if (offerArming || offerHandoff.state === "ready" || offerHandoff.state === "consuming") {
    throw new Error("an offer handoff is already armed");
  }
  offerArming = true;
  const previousPhase = phase;
  let offerBytes = null;
  try {
    phase = "mock-kyc-handoff";
    const kyc = await requestJson(`${ISSUER_ORIGIN}/api/issuer/kyc-sessions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });
    const sessionId = kyc?.sessionId;
    const offerValue = kyc?.credentialOfferUri;
    if (typeof sessionId !== "string" || typeof offerValue !== "string"
        || !offerValue.startsWith("openid-credential-offer://")) {
      throw new Error("mock KYC offer unavailable");
    }
    offerBytes = Buffer.from(offerValue);
    kyc.credentialOfferUri = null;
    const status = await requestJson(
      `${ISSUER_ORIGIN}/api/issuer/kyc-sessions/${encodeURIComponent(sessionId)}/status`,
    );
    if (String(status?.status).toLowerCase() !== "approved") {
      throw new Error("mock KYC not approved");
    }
    offerHandoff.arm(offerBytes, provisionCapability);
    offerBytes = null;
  } finally {
    offerBytes?.fill(0);
    offerArming = false;
    phase = previousPhase;
  }
}

function issuerMetadataReady() {
  return new Promise((resolve, reject) => {
    const request = http.request({
      host: "127.0.0.1",
      port: PORTAL_PROXY_PORT,
      method: "GET",
      path: "/.well-known/openid-credential-issuer",
    }, (response) => {
      response.resume();
      resolve((response.statusCode ?? 500) >= 200 && (response.statusCode ?? 500) < 300);
    });
    request.setTimeout(2_000, () => request.destroy(new Error("issuer readiness timeout")));
    request.once("error", reject);
    request.end();
  });
}

async function waitForIssuer() {
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    try {
      if (await issuerMetadataReady()) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("issuer did not become ready");
}

function canonicalManifest(issuerDid, issuerMethod, sourceJwk) {
  const jwk = {
    crv: sourceJwk.crv,
    kty: sourceJwk.kty,
    x: sourceJwk.x,
    y: sourceJwk.y,
  };
  if (!jwk.crv || !jwk.kty || !jwk.x || !jwk.y) throw new Error("issuer JWK unavailable");
  const jwkBytes = Buffer.from(JSON.stringify(jwk));
  return {
    integrationCommit: PORTAL_INTEGRATION_COMMIT,
    integrationTree: PORTAL_INTEGRATION_TREE,
    issuerDid,
    issuerJubjubJwk: jwk,
    issuerJubjubJwkSha256: sha256(jwkBytes),
    issuerMethod,
    issuerOrigin: ISSUER_ORIGIN,
    issuerResolverOrigin: ISSUER_RESOLVER_ORIGIN,
    portalPrHead: PORTAL_PR_HEAD,
    profileSourceCommit: PORTAL_PROFILE_SOURCE,
    provenanceSha256: PORTAL_PROVENANCE_SHA256,
    schema: "oxid-portal-deployment-v2",
  };
}

async function cleanup() {
  if (cleanupStarted) return;
  cleanupStarted = true;
  let cleanupFailure = null;
  proxyMode = "unavailable";
  offerHandoff.dispose();
  for (const candidate of [iosCapabilityCandidatePath, iosCapabilityPath]) {
    if (!candidate) continue;
    try {
      const metadata = fs.lstatSync(candidate);
      if (!metadata.isFile() || metadata.isSymbolicLink()) {
        throw new Error("iOS capability cleanup target is not a regular file");
      }
      fs.rmSync(candidate);
    } catch (error) {
      if (error?.code !== "ENOENT") cleanupFailure = error;
    }
  }
  iosCapabilityCandidatePath = null;
  iosCapabilityPath = null;
  for (const socket of heldSockets) socket.destroy();
  for (const request of proxiedSockets) request.destroy();
  for (const server of [controlServer, holderServer, issuerResolverProxy, proxyServer]) server.close();
  fs.closeSync(privateLog);
  if (cleanupFailure) throw cleanupFailure;
}
for (const [signal, status] of [["SIGINT", 130], ["SIGTERM", 143]]) {
  process.on(signal, () => {
    signalExitCode = status;
    completeResolve();
  });
}

try {
  phase = "listen";
  await Promise.all([
    listen(proxyServer, PORTAL_PROXY_PORT),
    listen(controlServer, CONTROL_PORT),
    listen(holderServer, HOLDER_RESOLVER_PORT),
    listen(issuerResolverProxy, ISSUER_RESOLVER_PROXY_PORT),
  ]);
  phase = "shared-status";
  runLogged(localHeadlessScript, ["status", stackEnvFile], repositoryRoot);
  await waitForIssuer();

  phase = "issuer-public-facts";
  const issuerContainers = runCaptured("docker", [
    "container", "ls", "--quiet",
    "--filter", `label=com.docker.compose.project=${composeProjectName}`,
    "--filter", "label=com.docker.compose.service=issuer",
  ]).split(/\s+/u).filter(Boolean);
  if (issuerContainers.length !== 1) throw new Error("exact issuer container unavailable");
  const issuerMethod = runCaptured("docker", [
    "exec", issuerContainers[0], "sh", "-c",
    'IFS= read -r value < /bootstrap/issuer-key-id; printf %s "$value"',
  ]);
  const issuerDid = issuerMethod.split("#", 1)[0];
  if (!issuerDid || !issuerMethod.startsWith(`${issuerDid}#`)) throw new Error("issuer method invalid");
  const resolution = await requestJson(`${LOCAL_ISSUER_RESOLVER_ORIGIN}/resolve`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ did: issuerDid }),
  });
  const method = resolution?.didDocument?.verificationMethod?.find((entry) => entry.id === issuerMethod);
  const manifest = canonicalManifest(issuerDid, issuerMethod, method?.publicKeyJwk ?? {});
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  const manifestSha256 = sha256(manifestBytes);
  fs.writeFileSync(manifestPath, manifestBytes, { mode: 0o600 });


  const ready = {
    controlOrigin: `http://127.0.0.1:${CONTROL_PORT}`,
    manifestPath,
    manifestSha256,
    issuerOrigin: ISSUER_ORIGIN,
    issuerResolverOrigin: ISSUER_RESOLVER_ORIGIN,
    offerUrl: `${ISSUER_ORIGIN}/offer`,
    portalProxyPort: PORTAL_PROXY_PORT,
    portalResolverPort: ISSUER_RESOLVER_PROXY_PORT,
    schema: "oxid-portal-mobile-ready-v1",
  };
  fs.writeFileSync(readyPath, JSON.stringify(ready), { mode: 0o600 });
  fs.writeFileSync(readyFifo, "READY\n");
  process.stdout.write("portal-mobile-support: READY\n");

  await completion;
  if (!complete) {
    appendPrivate("support terminated before platform completion");
    process.exitCode = signalExitCode || 1;
  }
} catch (error) {
  appendPrivate(error.stack ?? String(error));
  try { fs.writeFileSync(readyFifo, `FAIL:${phase}\n`); } catch {}
  process.stderr.write(`portal-mobile-support: FAIL phase=${phase}\n`);
  process.exitCode = 1;
} finally {
  try {
    await cleanup();
  } catch {
    process.stderr.write(`portal-mobile-support: FAIL phase=${phase}\n`);
    process.exitCode = 1;
  }
}
