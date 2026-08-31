#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { pathToFileURL } from "node:url";

import { exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const OPEN_TIMEOUT_MS = 15_000;
const COMMAND_TIMEOUT_MS = 15_000;
const JOURNEY_TIMEOUT_MS = 60_000;

function fail(message) {
  throw new Error(message);
}

/** Validates payload-private facts collected from the real browser page. */
export function assertSameOriginJourney({ origin, locations, copyOffer, qrRendered, sessionOffer }) {
  if (!exactPublicOrigin(origin) || !Array.isArray(locations) || !qrRendered
      || typeof copyOffer !== "string" || copyOffer.length === 0
      || copyOffer !== sessionOffer || !copyOffer.startsWith("openid-credential-offer://")) {
    fail("invalid browser journey");
  }
  const expectedPaths = new Set([
    "/issue/",
    "/kyc/mock-verification",
    "/issue/pending.html",
    "/issue/complete.html",
  ]);
  const observedPaths = new Set();
  for (const location of locations) {
    let parsed;
    try {
      parsed = new URL(location);
    } catch {
      fail("invalid browser navigation");
    }
    if (parsed.origin !== origin || parsed.protocol !== "https:"
        || parsed.username !== "" || parsed.password !== ""
        || parsed.search !== "" || parsed.hash !== ""
        || !expectedPaths.has(parsed.pathname)) {
      fail("browser origin drifted");
    }
    observedPaths.add(parsed.pathname);
  }
  if (observedPaths.size !== expectedPaths.size
      || [...expectedPaths].some((expected) => !observedPaths.has(expected))) {
    fail("browser journey incomplete");
  }
}

function wait(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function connect(endpoint) {
  const response = await fetch(`${endpoint}/json/list`, {
    signal: AbortSignal.timeout(OPEN_TIMEOUT_MS),
  });
  if (!response.ok) fail("browser debugging endpoint unavailable");
  const descriptions = await response.json();
  const description = Array.isArray(descriptions)
    ? descriptions.find((candidate) => candidate?.type === "page"
      && typeof candidate.webSocketDebuggerUrl === "string")
    : undefined;
  if (!description) fail("browser debugging endpoint unavailable");

  const socket = new WebSocket(description.webSocketDebuggerUrl);
  let nextId = 1;
  const pending = new Map();
  const navigations = [];
  let recording = false;
  let terminalError = null;
  const rejectPending = (error) => {
    terminalError ??= error;
    for (const { reject, timer } of pending.values()) {
      clearTimeout(timer);
      reject(terminalError);
    }
    pending.clear();
  };
  socket.addEventListener("message", (event) => {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      rejectPending(new Error("browser sent invalid protocol data"));
      socket.close();
      return;
    }
    if (message.method === "Page.frameNavigated" && recording && !message.params.frame.parentId) {
      navigations.push(message.params.frame.url);
    }
    if (!message.id || !pending.has(message.id)) return;
    const { resolve, reject, timer } = pending.get(message.id);
    clearTimeout(timer);
    pending.delete(message.id);
    if (message.error) reject(new Error("browser command failed"));
    else resolve(message.result);
  });
  socket.addEventListener("close", () => rejectPending(new Error("browser connection closed")));
  socket.addEventListener("error", () => rejectPending(new Error("browser connection failed")));
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error("browser connection timed out"));
    }, OPEN_TIMEOUT_MS);
    socket.addEventListener("open", () => {
      clearTimeout(timer);
      resolve();
    }, { once: true });
    socket.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error("browser connection failed"));
    }, { once: true });
  });

  const command = (method, params = {}) => {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      if (terminalError || socket.readyState !== WebSocket.OPEN) {
        reject(terminalError ?? new Error("browser connection unavailable"));
        return;
      }
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error("browser command timed out"));
      }, COMMAND_TIMEOUT_MS);
      pending.set(id, { resolve, reject, timer });
      socket.send(JSON.stringify({ id, method, params }));
    });
  };
  const evaluate = async (expression) => {
    const result = await command("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) fail("browser page evaluation failed");
    return result.result.value;
  };
  const waitFor = async (expression) => {
    const deadline = Date.now() + JOURNEY_TIMEOUT_MS;
    while (Date.now() < deadline) {
      if (await evaluate(expression)) return;
      await wait(100);
    }
    fail("browser page timed out");
  };
  return {
    close() { socket.close(); },
    command,
    evaluate,
    navigations,
    set recording(value) { recording = value; },
    waitFor,
  };
}

async function runBrowserJourney(debugEndpoint, origin, setPhase) {
  setPhase("connect");
  const browser = await connect(debugEndpoint);
  try {
    setPhase("page-enable");
    await browser.command("Page.enable");
    await browser.command("Runtime.enable");
    browser.recording = true;
    setPhase("index");
    await browser.command("Page.navigate", { url: `${origin}/issue/` });
    await browser.waitFor("document.readyState === 'complete' && location.pathname === '/issue/'");
    setPhase("begin");
    await browser.evaluate("document.getElementById('begin-button')?.click() ?? false");
    await browser.waitFor("location.pathname === '/kyc/mock-verification'");
    setPhase("approval");
    await browser.waitFor("Boolean(document.getElementById('approve-btn'))");
    await browser.evaluate("document.getElementById('approve-btn')?.click() ?? false");
    setPhase("pending");
    await browser.waitFor("location.pathname === '/issue/pending.html'");
    setPhase("complete");
    await browser.waitFor("location.pathname === '/issue/complete.html'");
    setPhase("offer-check");
    const completion = await browser.evaluate(`(() => {
      const copy = document.getElementById('offer-uri-text')?.textContent;
      const copyButton = document.getElementById('copy-button');
      copyButton?.click();
      return {
        copyOffer: copy,
        qrRendered: Boolean(document.querySelector('#qr-container svg')),
        sessionOffer: sessionStorage.getItem('passport_issuer_credential_offer_uri'),
      };
    })()`);
    assertSameOriginJourney({
      origin,
      locations: browser.navigations,
      ...completion,
    });
  } finally {
    browser.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let phase = "connect";
  try {
    const [operation, debugEndpoint, origin] = process.argv.slice(2);
    if (operation !== "--run" || !debugEndpoint || !origin || process.argv.length !== 5
        || !/^http:\/\/127\.0\.0\.1:[0-9]+$/u.test(debugEndpoint) || !exactPublicOrigin(origin)) {
      fail("invalid arguments");
    }
    await runBrowserJourney(debugEndpoint, origin, (next) => { phase = next; });
    process.stdout.write("portal-tailnet-browser-flow: PASS\n");
  } catch {
    process.stderr.write(`portal-tailnet-browser-flow: FAIL phase=${phase}\n`);
    process.exitCode = 1;
  }
}
