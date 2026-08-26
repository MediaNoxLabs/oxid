#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { randomBytes, timingSafeEqual } from "node:crypto";
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const VIRTUAL_OFFER_PORT = 18091;
const MAX_OFFER_BYTES = 32 * 1024;
const CAPABILITY_BYTES = 64;
const HANDOFF_TIMEOUT_MS = 5 * 60_000;
const OFFER_PREFIX = Buffer.from("openid-credential-offer://", "utf8");

function readPrivateFile(filePath, maximum) {
  if (!filePath || !path.isAbsolute(filePath)) throw new Error("invalid private file path");
  const metadata = fs.lstatSync(filePath);
  if (!metadata.isFile() || metadata.isSymbolicLink() || (metadata.mode & 0o077) !== 0
      || metadata.size === 0 || metadata.size > maximum) {
    throw new Error("invalid private file");
  }
  const bytes = fs.readFileSync(filePath);
  fs.rmSync(filePath);
  return bytes;
}

function sendJson(response, status, value) {
  const body = Buffer.from(JSON.stringify(value));
  response.writeHead(status, {
    "Cache-Control": "no-store",
    "Content-Length": body.length,
    "Content-Type": "application/json",
  });
  response.end(body);
}

function requestAuthorized(request, capability) {
  const prefix = "Bearer ";
  const header = request.headers.authorization;
  const supplied = typeof header === "string" && header.startsWith(prefix)
    ? Buffer.from(header.slice(prefix.length), "utf8") : Buffer.alloc(0);
  const authorized = supplied.length === capability.length
    && timingSafeEqual(supplied, capability);
  supplied.fill(0);
  return authorized;
}

async function runHarness() {
  let capability;
  let offer;
  let server;
  let timeout;
  try {
    capability = readPrivateFile(
      process.env.OXID_PORTAL_VIRTUAL_CAPABILITY_FILE,
      CAPABILITY_BYTES,
    );
    offer = readPrivateFile(process.env.OXID_PORTAL_VIRTUAL_OFFER_FILE, MAX_OFFER_BYTES);
    if (capability.length !== CAPABILITY_BYTES
        || !capability.every((byte) => byte >= 0x30 && (byte <= 0x39 || (byte >= 0x61 && byte <= 0x66)))
        || offer.length <= OFFER_PREFIX.length
        || !offer.subarray(0, OFFER_PREFIX.length).equals(OFFER_PREFIX)) {
      throw new Error("invalid handoff material");
    }

    let handoffState = "ready";
    server = http.createServer((request, response) => {
      if (request.method !== "GET" || request.url !== "/offer") {
        sendJson(response, 404, { error: "not_found" });
        return;
      }
      if (handoffState !== "ready") {
        sendJson(response, 410, { error: "unavailable" });
        return;
      }
      if (!requestAuthorized(request, capability)) {
        sendJson(response, 401, { error: "unauthorized" });
        return;
      }
      handoffState = "consuming";
      response.writeHead(200, {
        "Cache-Control": "no-store",
        "Content-Length": offer.length,
        "Content-Type": "text/plain; charset=utf-8",
      });
      response.end(offer, () => {
        offer.fill(0);
        handoffState = "consumed";
        setTimeout(() => server.close(), 500);
      });
    });
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(VIRTUAL_OFFER_PORT, "127.0.0.1", resolve);
    });
    timeout = setTimeout(() => {
      process.exitCode = 1;
      server.close();
    }, HANDOFF_TIMEOUT_MS);
    process.stdout.write(`portal-virtual-mobile-offer: READY port=${VIRTUAL_OFFER_PORT}\n`);
    await new Promise((resolve) => server.once("close", resolve));
    if (handoffState !== "consumed") throw new Error("offer was not consumed");
    process.stdout.write("portal-virtual-mobile-offer: PASS consumed=true\n");
  } finally {
    if (timeout) clearTimeout(timeout);
    capability?.fill(0);
    offer?.fill(0);
    if (server?.listening) server.close();
  }
}

async function runContract() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "oxid-portal-virtual-offer-"));
  fs.chmodSync(root, 0o700);
  const capabilityPath = path.join(root, "capability");
  const offerPath = path.join(root, "offer");
  const capability = randomBytes(32).toString("hex");
  const offer = "openid-credential-offer://?credential_offer=%7B%7D";
  fs.writeFileSync(capabilityPath, capability, { mode: 0o600 });
  fs.writeFileSync(offerPath, offer, { mode: 0o600 });
  const child = spawn(process.execPath, [fileURLToPath(import.meta.url)], {
    env: {
      ...process.env,
      OXID_PORTAL_VIRTUAL_CAPABILITY_FILE: capabilityPath,
      OXID_PORTAL_VIRTUAL_OFFER_FILE: offerPath,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout += chunk; });
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  try {
    const deadline = Date.now() + 10_000;
    while (!stdout.includes("READY") && child.exitCode === null && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (!stdout.includes("READY")) throw new Error("virtual offer harness did not become ready");
    if (fs.existsSync(capabilityPath) || fs.existsSync(offerPath)) {
      throw new Error("virtual offer handoff files were not unlinked before listen");
    }
    const origin = `http://127.0.0.1:${VIRTUAL_OFFER_PORT}`;
    const wrongPath = await fetch(`${origin}/counters`);
    if (wrongPath.status !== 404) throw new Error("control route was exposed on virtual offer port");
    const unauthorized = await fetch(`${origin}/offer`);
    if (unauthorized.status !== 401) throw new Error("unauthorized offer was not rejected");
    const accepted = await fetch(`${origin}/offer`, {
      headers: { Authorization: `Bearer ${capability}` },
    });
    if (accepted.status !== 200 || await accepted.text() !== offer) {
      throw new Error("authorized offer response was not exact");
    }
    const replay = await fetch(`${origin}/offer`, {
      headers: { Authorization: `Bearer ${capability}` },
    });
    if (replay.status !== 410) throw new Error("offer replay was not rejected");
    const status = await new Promise((resolve) => child.once("exit", resolve));
    if (status !== 0 || stderr !== "" || !stdout.includes("PASS consumed=true")) {
      throw new Error("virtual offer harness did not exit cleanly");
    }
    process.stdout.write("portal-virtual-mobile-offer-contract: PASS\n");
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
    fs.rmSync(root, { recursive: true, force: true });
  }
}

try {
  if (process.argv[2] === "--contract-test" && process.argv.length === 3) {
    await runContract();
  } else if (process.argv.length === 2) {
    await runHarness();
  } else {
    throw new Error("invalid arguments");
  }
} catch {
  process.stderr.write("portal-virtual-mobile-offer: FAIL\n");
  process.exitCode = 1;
}
