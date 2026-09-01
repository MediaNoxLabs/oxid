#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { spawnSync } from "node:child_process";

import { exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const PORTAL_COMMIT = "22ae5369b6f939e6b20648f4b85dd993527748ef";
const PORTAL_TREE = "74d8d1a5b87c160ea554006e47d5f3edc3cd3e10";
const PORTAL_PROVENANCE_SHA256 = "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87";
const ISSUER_PROXY_PORT = 18090;
const PHYSICAL_CONTROL_PORT = 18095;
const HOLDER_RESOLVER_PORT = 18092;
const VIRTUAL_OFFER_PORT = 18091;
const PHYSICAL_OFFER_PORT = 18094;
const ISSUER_RESOLVER_PROXY_PORT = 18093;
const MAX_CONTROL_BODY = 2 * 1024 * 1024;
const MAX_OFFER_BYTES = 32 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;
const LIFECYCLE_TIMEOUT_MS = 15 * 60_000;

const source = process.env.PORTAL_INTEGRATION_CHECKOUT;
const stateDirectory = process.env.OXID_PORTAL_MOBILE_STATE_DIR;
const readyFifo = process.env.OXID_PORTAL_MOBILE_READY_FIFO;
const capabilityFifo = process.env.OXID_PORTAL_MOBILE_CAPABILITY_FIFO;
const lifecycle = process.env.PORTAL_CONSUMER_LIFECYCLE;
const supportProfile = process.env.OXID_PORTAL_MOBILE_SUPPORT_PROFILE ?? "physical-android";
const controlReceiptEnabled = process.env.OXID_PORTAL_MOBILE_CONTROL_RECEIPT !== "none";
const virtualProfile = supportProfile === "virtual-mobile";
const publicOrigin = virtualProfile
  ? `http://127.0.0.1:${ISSUER_PROXY_PORT}`
  : process.env.OXID_BUILD_PORTAL_PUBLIC_ORIGIN;
const offerPort = virtualProfile ? VIRTUAL_OFFER_PORT : PHYSICAL_OFFER_PORT;
const offerPath = virtualProfile ? "/offer" : "/";
const issuerResolverOrigin = virtualProfile
  ? `http://127.0.0.1:${ISSUER_RESOLVER_PROXY_PORT}`
  : `${publicOrigin}/issuer-resolver`;

function exactPrivatePath(value, kind) {
  if (!value || !path.isAbsolute(value) || !fs.existsSync(value)) return false;
  const metadata = fs.lstatSync(value);
  return !metadata.isSymbolicLink()
    && (kind === "directory" ? metadata.isDirectory() : metadata.isFIFO());
}

if (!source || !path.isAbsolute(source)
    || !lifecycle || !path.isAbsolute(lifecycle)
    || !exactPrivatePath(stateDirectory, "directory")
    || !exactPrivatePath(readyFifo, "fifo")
    || !exactPrivatePath(capabilityFifo, "fifo")
    || !new Set(["physical-android", "virtual-mobile"]).has(supportProfile)
    || (!virtualProfile && !exactPublicOrigin(publicOrigin))) {
  process.stderr.write("portal-android-support: FAIL phase=configuration\n");
  process.exit(2);
}

const privateLogPath = path.join(stateDirectory, "support-private.log");
const privateLog = fs.openSync(privateLogPath, "a", 0o600);
const manifestPath = path.join(stateDirectory, "deployment.json");
const consumerState = path.join(stateDirectory, "portal-consumer");
let phase = "startup";
let holderResolution = null;
let offer = null;
let capability = null;
const controlCapability = Buffer.from(randomBytes(32).toString("hex"), "utf8");
const holderCapability = virtualProfile
  ? null : Buffer.from(randomBytes(32).toString("hex"), "utf8");
let handoffState = "empty";
let proxyMode = "normal";
let completionResolve;
let complete = false;
let cleanupStarted = false;
const proxied = new Set();
const delayedResponses = new Map();
const counters = {
  authorizationMetadata: 0,
  credential: 0,
  issuerMetadata: 0,
  issuerResolution: 0,
  issuerResolutionSuccess: 0,
  holderPublications: 0,
  kyc: 0,
  nonce: 0,
  other: 0,
  token: 0,
};
const completion = new Promise((resolve) => { completionResolve = resolve; });

function appendPrivate(message) {
  fs.writeSync(privateLog, `${new Date().toISOString()} ${message}\n`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function runLifecycle(operation) {
  const result = spawnSync(lifecycle, [operation], {
    env: {
      ...process.env,
      PORTAL_INTEGRATION_CHECKOUT: source,
      OXID_PORTAL_CONSUMER_STATE_DIR: consumerState,
      PORTAL_ISSUER_URL: publicOrigin,
      PORTAL_HOLDER_RESOLVER_URL: `http://host.docker.internal:${HOLDER_RESOLVER_PORT}`,
    },
    stdio: ["ignore", privateLog, privateLog],
    timeout: LIFECYCLE_TIMEOUT_MS,
    killSignal: "SIGKILL",
  });
  if (result.error || result.status !== 0) throw new Error(`lifecycle ${operation} failed`);
}

function runCaptured(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    timeout: 30_000,
    killSignal: "SIGKILL",
  });
  if (result.error || result.status !== 0) throw new Error(`${command} failed`);
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
      } else {
        chunks.push(chunk);
      }
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

function pathCounter(requestPath) {
  if (requestPath === "/.well-known/openid-credential-issuer") return "issuerMetadata";
  if (requestPath === "/.well-known/oauth-authorization-server") return "authorizationMetadata";
  if (requestPath === "/api/issuer/token") return "token";
  if (requestPath === "/api/issuer/nonce") return "nonce";
  if (requestPath === "/api/issuer/credentials") return "credential";
  if (requestPath.startsWith("/api/issuer/kyc-sessions")) return "kyc";
  return "other";
}

function proxyRequest(request, response, port, upstreamPath = request.url) {
  const upstream = http.request({
    host: "127.0.0.1",
    port,
    method: request.method,
    path: upstreamPath,
    headers: { ...request.headers, host: `127.0.0.1:${port}` },
  }, (upstreamResponse) => {
    response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  proxied.add(upstream);
  upstream.once("close", () => proxied.delete(upstream));
  upstream.once("error", () => response.destroy());
  request.pipe(upstream);
}

const issuerProxy = http.createServer((request, response) => {
  let parsed;
  try {
    parsed = new URL(request.url, publicOrigin);
  } catch {
    response.destroy();
    return;
  }
  const counter = pathCounter(parsed.pathname);
  counters[counter] += 1;
  if (counter !== "kyc" && proxyMode === "unavailable") {
    // A transport reset permits client retries, making this single-boundary
    // failure non-deterministic. A terminal 503 preserves the unavailable
    // condition while proving exactly one rejected protocol request.
    sendJson(response, 503, { error: "unavailable" });
    return;
  }
  if (counter === "issuerMetadata" && proxyMode === "malformed") {
    const body = Buffer.from('{"credential_issuer":');
    response.writeHead(200, {
      "Cache-Control": "no-store",
      "Content-Length": body.length,
      "Content-Type": "application/json",
    });
    response.end(body);
    return;
  }
  if (counter !== "kyc" && proxyMode === "timeout") {
    const timer = setTimeout(() => {
      delayedResponses.delete(response);
      if (!response.destroyed) sendJson(response, 504, { error: "timeout" });
    }, REQUEST_TIMEOUT_MS);
    delayedResponses.set(response, timer);
    response.once("close", () => {
      const active = delayedResponses.get(response);
      if (active) clearTimeout(active);
      delayedResponses.delete(response);
    });
    return;
  }
  proxyRequest(request, response, 8090);
});

const issuerResolverProxy = http.createServer((request, response) => {
  if (request.method !== "POST" || request.url !== "/resolve") {
    sendJson(response, 404, { error: "not_found" });
    return;
  }
  counters.issuerResolution += 1;
  const upstream = http.request({
    host: "127.0.0.1",
    port: 9092,
    method: "POST",
    path: "/resolve",
    headers: { ...request.headers, host: "127.0.0.1:9092" },
  }, (upstreamResponse) => {
    if ((upstreamResponse.statusCode ?? 500) >= 200
        && (upstreamResponse.statusCode ?? 500) < 300) {
      counters.issuerResolutionSuccess += 1;
    }
    response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  proxied.add(upstream);
  upstream.once("close", () => proxied.delete(upstream));
  upstream.once("error", () => response.destroy());
  request.pipe(upstream);
});

function validatePublishedDid(bytes) {
  const resolution = JSON.parse(bytes.toString("utf8"));
  const document = resolution?.didDocument;
  if (!document || typeof document.id !== "string"
      || !document.id.startsWith("did:midnight:")
      || document.id.length > 512
      || !Array.isArray(document.verificationMethod)
      || document.verificationMethod.length === 0
      || document.verificationMethod.length > 32) {
    throw new Error("invalid public DID resolution");
  }
  const verificationMethod = document.verificationMethod.map((method) => {
    const key = method?.publicKeyJwk;
    if (typeof method?.id !== "string" || typeof method?.controller !== "string"
        || method.controller !== document.id || method.type !== "JsonWebKey"
        || typeof key?.kty !== "string" || typeof key?.crv !== "string"
        || typeof key?.x !== "string" || method.id.length > 1024
        || !method.id.startsWith(`${document.id}#`) || key.kty.length > 32
        || key.crv.length > 32 || key.x.length > 256
        || (key.y !== undefined && (typeof key.y !== "string" || key.y.length > 256))
        || Object.hasOwn(key, "d")) {
      throw new Error("invalid public DID method");
    }
    const publicKeyJwk = { kty: key.kty, crv: key.crv, x: key.x };
    if (key.y !== undefined) publicKeyJwk.y = key.y;
    return { id: method.id, type: "JsonWebKey", controller: document.id, publicKeyJwk };
  });
  const methodIds = new Set(verificationMethod.map((method) => method.id));
  const relationships = ["authentication", "assertionMethod"];
  if (!relationships.every((name) => Array.isArray(document[name])
      && document[name].length > 0 && document[name].length <= 32
      && document[name].every((method) => typeof method === "string"
        && methodIds.has(method)))) {
    throw new Error("invalid public DID relationships");
  }
  return {
    didDocument: {
      id: document.id,
      verificationMethod,
      authentication: [...document.authentication],
      assertionMethod: [...document.assertionMethod],
    },
    didDocumentMetadata: { deactivated: false },
    didResolutionMetadata: { contentType: "application/did+ld+json" },
  };
}

// Virtual-device and desktop harnesses retain their private control-plane
// bridge. The physical Tailnet flow must use the in-app explicit bootstrap.
function transformStoredDid(bytes) {
  const stored = JSON.parse(bytes.toString("utf8"));
  if (stored?.schemaVersion !== 1 || !Array.isArray(stored.records) || stored.records.length === 0) {
    throw new Error("invalid public DID store");
  }
  const source = stored.records.at(-1)?.resolution?.document;
  if (!source || typeof source.id !== "string" || !Array.isArray(source.verificationMethods)
      || !Array.isArray(source.relationships)) {
    throw new Error("invalid public DID record");
  }
  const verificationMethod = source.verificationMethods.map((method) => {
    const key = method.publicKeyJwk;
    if (!method.id || !method.controller || !key?.keyType || !key?.curve || !key?.x) {
      throw new Error("invalid public DID method");
    }
    const publicKeyJwk = { kty: key.keyType, crv: key.curve, x: key.x };
    if (key.y !== null && key.y !== undefined) publicKeyJwk.y = key.y;
    return { id: method.id, type: "JsonWebKey", controller: method.controller, publicKeyJwk };
  });
  const document = { id: source.id, verificationMethod };
  for (const relationship of source.relationships) {
    if (typeof relationship.relationship !== "string" || !Array.isArray(relationship.methodIds)) {
      throw new Error("invalid public DID relationship");
    }
    document[relationship.relationship] = relationship.methodIds;
  }
  return {
    didDocument: document,
    didDocumentMetadata: { deactivated: false },
    didResolutionMetadata: { contentType: "application/did+ld+json" },
  };
}

const holderResolver = http.createServer(async (request, response) => {
  if (request.method === "GET" && request.url === "/health") {
    sendJson(response, 200, { ok: true });
    return;
  }
  if (request.method !== "POST" || request.url !== "/resolve") {
    sendJson(response, 404, { error: "not_found" });
    return;
  }
  try {
    const requested = JSON.parse((await readBounded(request, 64 * 1024)).toString("utf8"));
    if (holderResolution && requested?.did === holderResolution.didDocument.id) {
      sendJson(response, 200, holderResolution);
    } else {
      sendJson(response, 404, {
        didDocument: null,
        didDocumentMetadata: { deactivated: false },
        didResolutionMetadata: { error: "notFound" },
      });
    }
  } catch {
    sendJson(response, 400, { error: "invalid_request" });
  }
});

async function requestJson(url, options = {}) {
  const response = await fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
  if (!response.ok) throw new Error("request failed");
  return response.json();
}

function clearHandoff() {
  offer?.fill(0);
  capability?.fill(0);
  offer = null;
  capability = null;
  handoffState = "empty";
}

function handleOffer(request, response) {
  if (request.method !== "GET" || request.url !== offerPath) return false;
  if (handoffState !== "ready" || !offer || !capability) {
    sendJson(response, 410, { error: "unavailable" });
    return true;
  }
  const prefix = "Bearer ";
  const header = request.headers.authorization;
  const supplied = typeof header === "string" && header.startsWith(prefix)
    ? Buffer.from(header.slice(prefix.length), "utf8") : Buffer.alloc(0);
  const authenticated = supplied.length === capability.length && timingSafeEqual(supplied, capability);
  supplied.fill(0);
  if (!authenticated) {
    sendJson(response, 401, { error: "unauthorized" });
    return true;
  }
  handoffState = "consuming";
  const body = offer;
  response.writeHead(200, {
    "Cache-Control": "no-store",
    "Content-Length": body.length,
    "Content-Type": "text/plain; charset=utf-8",
  });
  response.end(body, () => clearHandoff());
  return true;
}

async function armOffer() {
  if (handoffState !== "empty") throw new Error("handoff busy");
  const kyc = await requestJson(`http://127.0.0.1:${ISSUER_PROXY_PORT}/api/issuer/kyc-sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: "{}",
  });
  const sessionId = kyc?.sessionId;
  const value = kyc?.credentialOfferUri;
  if (typeof sessionId !== "string" || typeof value !== "string"
      || !value.startsWith("openid-credential-offer://")) {
    throw new Error("offer unavailable");
  }
  const status = await requestJson(
    `http://127.0.0.1:${ISSUER_PROXY_PORT}/api/issuer/kyc-sessions/${encodeURIComponent(sessionId)}/status`,
  );
  if (String(status?.status).toLowerCase() !== "approved") throw new Error("KYC unavailable");
  const candidateOffer = Buffer.from(value, "utf8");
  kyc.credentialOfferUri = null;
  if (candidateOffer.length === 0 || candidateOffer.length > MAX_OFFER_BYTES) {
    candidateOffer.fill(0);
    throw new Error("offer invalid");
  }
  const candidateCapability = Buffer.from(randomBytes(32).toString("hex"), "utf8");
  offer = candidateOffer;
  capability = candidateCapability;
  handoffState = "ready";
  try {
    fs.writeFileSync(capabilityFifo, capability);
  } catch (error) {
    clearHandoff();
    throw error;
  }
}

function controlAuthorized(request) {
  const prefix = "Bearer ";
  const header = request.headers.authorization;
  const supplied = typeof header === "string" && header.startsWith(prefix)
    ? Buffer.from(header.slice(prefix.length), "utf8") : Buffer.alloc(0);
  const authenticated = supplied.length === controlCapability.length
    && timingSafeEqual(supplied, controlCapability);
  supplied.fill(0);
  return authenticated;
}

function holderAuthorized(request) {
  if (!holderCapability) return false;
  const prefix = "Bearer ";
  const header = request.headers.authorization;
  const supplied = typeof header === "string" && header.startsWith(prefix)
    ? Buffer.from(header.slice(prefix.length), "utf8") : Buffer.alloc(0);
  const authenticated = supplied.length === holderCapability.length
    && timingSafeEqual(supplied, holderCapability);
  supplied.fill(0);
  return authenticated;
}

const offerServer = http.createServer(async (request, response) => {
  try {
    if (handleOffer(request, response)) return;
    if (request.method === "POST" && request.url === "/") {
      if (!holderAuthorized(request)) {
        sendJson(response, 401, { error: "unauthorized" });
        return;
      }
      if (request.headers["content-type"] !== "application/json") {
        sendJson(response, 415, { error: "unsupported_media_type" });
        return;
      }
      holderResolution = validatePublishedDid(await readBounded(request, 512 * 1024));
      counters.holderPublications += 1;
      sendJson(response, 200, { accepted: true });
      return;
    }
    sendJson(response, 404, { error: "not_found" });
  } catch {
    sendJson(response, 400, { error: "invalid_request" });
  }
});

const controlServer = http.createServer(async (request, response) => {
  try {
    if (request.method === "GET" && request.url === "/health") {
      sendJson(response, 200, { ok: true });
      return;
    }
    if (!controlAuthorized(request)) {
      sendJson(response, 401, { error: "unauthorized" });
      return;
    }
    if (request.method === "GET" && request.url === "/counters") {
      sendJson(response, 200, counters);
    } else if (request.method === "GET" && request.url === "/handoff-status") {
      sendJson(response, 200, { state: handoffState });
    } else if (request.method === "POST" && request.url === "/holder" && virtualProfile) {
      holderResolution = transformStoredDid(await readBounded(request));
      sendJson(response, 200, { accepted: true });
    } else if (request.method === "POST" && request.url === "/arm-android-offer") {
      await armOffer();
      sendJson(response, 200, { armed: true });
    } else if (request.method === "POST" && request.url === "/proxy-mode") {
      const candidate = (await readBounded(request, 32)).toString("utf8");
      if (!new Set(["normal", "malformed", "unavailable", "timeout"]).has(candidate)) {
        sendJson(response, 400, { error: "invalid_mode" });
        return;
      }
      proxyMode = candidate;
      sendJson(response, 200, { mode: proxyMode });
    } else if (request.method === "POST" && request.url === "/complete") {
      complete = true;
      sendJson(response, 200, { ok: true });
      completionResolve();
    } else {
      sendJson(response, 404, { error: "not_found" });
    }
  } catch {
    sendJson(response, 400, { error: "invalid_request" });
  }
});

function listen(server, port) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
}

async function issuerPublicFacts() {
  const containers = runCaptured("docker", [
    "ps", "--quiet",
    "--filter", "label=com.docker.compose.project=oxid-portal-consumer",
    "--filter", "label=com.docker.compose.service=issuer",
  ]).split(/\s+/u).filter(Boolean);
  if (containers.length !== 1) throw new Error("issuer unavailable");
  const issuerMethod = runCaptured("docker", [
    "exec", containers[0], "sh", "-c",
    'IFS= read -r value < /bootstrap/issuer-key-id; printf %s "$value"',
  ]);
  const issuerDid = issuerMethod.split("#", 1)[0];
  if (!issuerDid || !issuerMethod.startsWith(`${issuerDid}#`)) throw new Error("issuer invalid");
  const resolution = await requestJson(`http://127.0.0.1:${ISSUER_RESOLVER_PROXY_PORT}/resolve`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ did: issuerDid }),
  });
  const sourceJwk = resolution?.didDocument?.verificationMethod
    ?.find((entry) => entry.id === issuerMethod)?.publicKeyJwk;
  const jwk = { crv: sourceJwk?.crv, kty: sourceJwk?.kty, x: sourceJwk?.x, y: sourceJwk?.y };
  if (!jwk.crv || !jwk.kty || !jwk.x || !jwk.y) throw new Error("issuer key unavailable");
  return { issuerDid, issuerMethod, jwk };
}

async function cleanup() {
  if (cleanupStarted) return;
  cleanupStarted = true;
  clearHandoff();
  controlCapability.fill(0);
  holderCapability?.fill(0);
  for (const request of proxied) request.destroy();
  for (const [response, timer] of delayedResponses) {
    clearTimeout(timer);
    response.destroy();
  }
  delayedResponses.clear();
  await Promise.all([issuerProxy, issuerResolverProxy, holderResolver, offerServer, controlServer].map(
    (server) => new Promise((resolve) => server.close(resolve)),
  ));
  try {
    runLifecycle("down");
  } finally {
    fs.rmSync(manifestPath, { force: true });
    fs.closeSync(privateLog);
    fs.rmSync(privateLogPath, { force: true });
    fs.rmSync(consumerState, { recursive: true, force: true });
  }
}

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    if (!controlReceiptEnabled) complete = true;
    completionResolve();
  });
}

try {
  phase = "listeners";
  await Promise.all([
    listen(issuerProxy, ISSUER_PROXY_PORT),
    listen(controlServer, PHYSICAL_CONTROL_PORT),
    listen(holderResolver, HOLDER_RESOLVER_PORT),
    listen(offerServer, offerPort),
    listen(issuerResolverProxy, ISSUER_RESOLVER_PROXY_PORT),
  ]);
  phase = "portal-stack";
  runLifecycle("up");
  phase = "issuer-public-facts";
  const { issuerDid, issuerMethod, jwk } = await issuerPublicFacts();
  const manifest = {
    integrationCommit: PORTAL_COMMIT,
    integrationTree: PORTAL_TREE,
    issuerDid,
    issuerJubjubJwk: jwk,
    issuerJubjubJwkSha256: sha256(Buffer.from(JSON.stringify(jwk))),
    issuerMethod,
    issuerOrigin: publicOrigin,
    issuerResolverOrigin,
    provenanceSha256: PORTAL_PROVENANCE_SHA256,
    schema: "oxid-portal-deployment-v3",
  };
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  fs.writeFileSync(manifestPath, manifestBytes, { mode: 0o600 });
  const ready = {
    controlCapability: controlReceiptEnabled
      ? "0".repeat(controlCapability.length)
      : "0".repeat(64),
    controlOrigin: `http://127.0.0.1:${PHYSICAL_CONTROL_PORT}`,
    issuerProxyPort: ISSUER_PROXY_PORT,
    offerPort,
    resolverProxyPort: ISSUER_RESOLVER_PROXY_PORT,
    manifestPath,
    manifestSha256: sha256(manifestBytes),
    schema: virtualProfile ? "oxid-portal-virtual-ready-v1" : "oxid-portal-android-ready-v2",
  };
  if (holderCapability) ready.holderCapability = "h".repeat(holderCapability.length);
  const readyBytes = Buffer.from(JSON.stringify(ready));
  if (controlReceiptEnabled) {
    const placeholder = Buffer.from(`"${"0".repeat(controlCapability.length)}"`);
    const placeholderOffset = readyBytes.indexOf(placeholder);
    if (placeholderOffset < 0) throw new Error("control capability staging failed");
    controlCapability.copy(readyBytes, placeholderOffset + 1);
    placeholder.fill(0);
  }
  if (holderCapability) {
    const holderPlaceholder = Buffer.from(`"${"h".repeat(holderCapability.length)}"`);
    const holderPlaceholderOffset = readyBytes.indexOf(holderPlaceholder);
    if (holderPlaceholderOffset < 0) throw new Error("holder capability staging failed");
    holderCapability.copy(readyBytes, holderPlaceholderOffset + 1);
    holderPlaceholder.fill(0);
  }
  fs.writeFileSync(path.join(stateDirectory, "ready.json"), readyBytes, { mode: 0o600 });
  readyBytes.fill(0);
  fs.writeFileSync(readyFifo, "READY\n");
  process.stdout.write("portal-android-support: READY\n");
  await completion;
  if (!complete) process.exitCode = 1;
} catch (error) {
  appendPrivate(`${phase}: ${error?.message ?? "failed"}`);
  try { fs.writeFileSync(readyFifo, `FAIL:${phase}\n`); } catch {}
  process.stderr.write(`portal-android-support: FAIL phase=${phase}\n`);
  process.exitCode = 1;
} finally {
  try {
    await cleanup();
  } catch {
    process.stderr.write("portal-android-support: FAIL phase=cleanup\n");
    process.exitCode = 1;
  }
}
