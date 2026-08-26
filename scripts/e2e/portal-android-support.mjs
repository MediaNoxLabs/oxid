#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { spawnSync } from "node:child_process";

const PORTAL_COMMIT = "22ae5369b6f939e6b20648f4b85dd993527748ef";
const PORTAL_TREE = "74d8d1a5b87c160ea554006e47d5f3edc3cd3e10";
const PORTAL_PROVENANCE_SHA256 = "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87";
const ISSUER_PROXY_PORT = 18090;
const CONTROL_PORT = 18091;
const HOLDER_RESOLVER_PORT = 18092;
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
const publicOrigin = process.env.OXID_BUILD_PORTAL_PUBLIC_ORIGIN;

function exactPublicOrigin(value) {
  try {
    const parsed = new URL(value);
    const port = Number(parsed.port);
    return parsed.protocol === "https:"
      && parsed.username === "" && parsed.password === ""
      && Number.isInteger(port) && port >= 1024
      && !new Set([443, 8443, 10000]).has(port)
      && parsed.pathname === "/" && parsed.search === "" && parsed.hash === ""
      && parsed.hostname.endsWith(".ts.net") && parsed.hostname !== "ts.net"
      && parsed.hostname.split(".").every((label) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(label))
      && parsed.origin === value;
  } catch {
    return false;
  }
}

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
    || !exactPublicOrigin(publicOrigin)) {
  process.stderr.write("portal-android-support: FAIL phase=configuration\n");
  process.exit(2);
}

const privateLogPath = path.join(stateDirectory, "support-private.log");
const privateLog = fs.openSync(privateLogPath, "a", 0o600);
const manifestPath = path.join(stateDirectory, "deployment.json");
const consumerState = path.join(stateDirectory, "portal-consumer");
let phase = "startup";
let holderDocument = null;
let offer = null;
let capability = null;
let handoffState = "empty";
let completionResolve;
let complete = false;
let cleanupStarted = false;
const proxied = new Set();
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
  counters[pathCounter(parsed.pathname)] += 1;
  proxyRequest(request, response, 8090);
});

const issuerResolverProxy = http.createServer((request, response) => {
  if (request.method !== "POST"
      || !new Set(["/resolve", "/issuer-resolver/resolve"]).has(request.url)) {
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

function transformStoredDid(bytes) {
  const stored = JSON.parse(bytes.toString("utf8"));
  if (stored?.schemaVersion !== 1 || !Array.isArray(stored.records) || stored.records.length === 0) {
    throw new Error("invalid public DID store");
  }
  const sourceDocument = stored.records.at(-1)?.resolution?.document;
  if (!sourceDocument || typeof sourceDocument.id !== "string"
      || !Array.isArray(sourceDocument.verificationMethods)
      || !Array.isArray(sourceDocument.relationships)) {
    throw new Error("invalid public DID record");
  }
  const verificationMethod = sourceDocument.verificationMethods.map((method) => {
    const key = method.publicKeyJwk;
    if (!method.id || !method.controller || !key?.keyType || !key?.curve || !key?.x) {
      throw new Error("invalid public DID method");
    }
    const publicKeyJwk = { kty: key.keyType, crv: key.curve, x: key.x };
    if (key.y !== null && key.y !== undefined) publicKeyJwk.y = key.y;
    return { id: method.id, type: "JsonWebKey", controller: method.controller, publicKeyJwk };
  });
  const document = { id: sourceDocument.id, verificationMethod };
  for (const relationship of sourceDocument.relationships) {
    if (typeof relationship.relationship !== "string" || !Array.isArray(relationship.methodIds)) {
      throw new Error("invalid public DID relationship");
    }
    document[relationship.relationship] = relationship.methodIds;
  }
  return document;
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
    if (holderDocument && requested?.did === holderDocument.id) {
      sendJson(response, 200, {
        didDocument: holderDocument,
        didDocumentMetadata: { deactivated: false },
        didResolutionMetadata: { contentType: "application/did+ld+json" },
      });
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
  if (request.method !== "GET" || request.url !== "/offer") return false;
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

const controlServer = http.createServer(async (request, response) => {
  try {
    if (handleOffer(request, response)) return;
    if (request.method === "GET" && request.url === "/health") {
      sendJson(response, 200, { ok: true });
    } else if (request.method === "GET" && request.url === "/counters") {
      sendJson(response, 200, counters);
    } else if (request.method === "GET" && request.url === "/handoff-status") {
      sendJson(response, 200, { state: handoffState });
    } else if (request.method === "POST" && request.url === "/holder") {
      holderDocument = transformStoredDid(await readBounded(request));
      sendJson(response, 200, { accepted: true });
    } else if (request.method === "POST" && request.url === "/arm-android-offer") {
      await armOffer();
      sendJson(response, 200, { armed: true });
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
  for (const request of proxied) request.destroy();
  await Promise.all([issuerProxy, issuerResolverProxy, holderResolver, controlServer].map(
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
  process.on(signal, () => completionResolve());
}

try {
  phase = "listeners";
  await Promise.all([
    listen(issuerProxy, ISSUER_PROXY_PORT),
    listen(controlServer, CONTROL_PORT),
    listen(holderResolver, HOLDER_RESOLVER_PORT),
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
    issuerResolverOrigin: `${publicOrigin}/issuer-resolver`,
    provenanceSha256: PORTAL_PROVENANCE_SHA256,
    schema: "oxid-portal-deployment-v3",
  };
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  fs.writeFileSync(manifestPath, manifestBytes, { mode: 0o600 });
  const ready = {
    controlOrigin: `http://127.0.0.1:${CONTROL_PORT}`,
    issuerProxyPort: ISSUER_PROXY_PORT,
    resolverProxyPort: ISSUER_RESOLVER_PROXY_PORT,
    manifestPath,
    manifestSha256: sha256(manifestBytes),
    schema: "oxid-portal-android-ready-v1",
  };
  fs.writeFileSync(path.join(stateDirectory, "ready.json"), JSON.stringify(ready), { mode: 0o600 });
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
