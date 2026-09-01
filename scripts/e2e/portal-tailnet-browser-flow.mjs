#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { pathToFileURL } from "node:url";

import { exactPublicOrigin } from "./tailnet-origin-policy.mjs";

const OPEN_TIMEOUT_MS = 15_000;
const COMMAND_TIMEOUT_MS = 15_000;
const JOURNEY_TIMEOUT_MS = 60_000;
const NAVIGATION_STEPS = Object.freeze([
  { path: "/issue/", pathClass: "index" },
  { path: "/kyc/mock-verification", pathClass: "mock" },
  { path: "/issue/pending.html", pathClass: "pending" },
  { path: "/issue/complete.html", pathClass: "complete" },
]);
const PATH_CLASSES = new Map(NAVIGATION_STEPS.map(({ path, pathClass }) => [path, pathClass]));

function fail(message) {
  throw new Error(message);
}

function navigationPathClass(origin, location) {
  let parsed;
  try {
    parsed = new URL(location);
  } catch {
    fail("invalid browser navigation");
  }
  const pathClass = PATH_CLASSES.get(parsed.pathname);
  if (parsed.origin !== origin || parsed.protocol !== "https:"
      || parsed.username !== "" || parsed.password !== ""
      || parsed.search !== "" || parsed.hash !== "" || !pathClass) {
    fail("browser origin drifted");
  }
  return pathClass;
}

function exactLocationPredicate(origin, path) {
  return `location.origin === ${JSON.stringify(origin)} && location.pathname === ${JSON.stringify(path)} && location.search === "" && location.hash === ""`;
}

/** Formats payload-free, private navigation timing diagnostics. */
export function formatNavigationDiagnostic({ elapsedMs, pathClass }) {
  if (!Number.isSafeInteger(elapsedMs) || elapsedMs < 0
      || !NAVIGATION_STEPS.some((step) => step.pathClass === pathClass)) {
    fail("invalid browser navigation diagnostic");
  }
  return `portal-tailnet-browser-flow: navigation elapsed_ms=${elapsedMs} path_class=${pathClass}`;
}

/** Validates payload-private facts collected from the real browser page. */
export function assertSameOriginJourney({ origin, locations, copyOffer, qrRendered, sessionOffer }) {
  if (!exactPublicOrigin(origin) || !Array.isArray(locations) || !qrRendered
      || typeof copyOffer !== "string" || copyOffer.length === 0
      || copyOffer !== sessionOffer || !copyOffer.startsWith("openid-credential-offer://")) {
    fail("invalid browser journey");
  }
  if (locations.length !== NAVIGATION_STEPS.length) fail("browser journey incomplete");
  for (const [index, location] of locations.entries()) {
    if (navigationPathClass(origin, location) !== NAVIGATION_STEPS[index].pathClass) {
      fail("browser navigation order drifted");
    }
  }
}

function wait(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function connect(endpoint, origin, writeNavigationDiagnostic) {
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
  const navigationPathClasses = [];
  const navigationStartedAt = Date.now();
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
      let pathClass;
      try {
        pathClass = navigationPathClass(origin, message.params.frame.url);
        if (pathClass !== NAVIGATION_STEPS[navigationPathClasses.length]?.pathClass) {
          fail("browser navigation order drifted");
        }
        navigationPathClasses.push(pathClass);
        navigations.push(message.params.frame.url);
        writeNavigationDiagnostic({ elapsedMs: Date.now() - navigationStartedAt, pathClass });
      } catch (error) {
        rejectPending(error);
        socket.close();
        return;
      }
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
  const waitForRecordedNavigation = async (pathClass) => {
    if (!NAVIGATION_STEPS.some((step) => step.pathClass === pathClass)) {
      fail("invalid browser navigation");
    }
    const deadline = Date.now() + JOURNEY_TIMEOUT_MS;
    while (Date.now() < deadline) {
      if (terminalError) throw terminalError;
      if (navigationPathClasses.includes(pathClass)) return;
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
    waitForRecordedNavigation,
  };
}

async function runBrowserJourney(debugEndpoint, origin, setPhase) {
  setPhase("connect");
  const browser = await connect(debugEndpoint, origin, (diagnostic) => {
    process.stderr.write(`${formatNavigationDiagnostic(diagnostic)}\n`);
  });
  try {
    setPhase("page-enable");
    await browser.command("Page.enable");
    await browser.command("Runtime.enable");
    browser.recording = true;
    setPhase("index");
    await browser.command("Page.navigate", { url: `${origin}/issue/` });
    await browser.waitFor(`document.readyState === "complete" && ${exactLocationPredicate(origin, "/issue/")}`);
    setPhase("begin");
    await browser.evaluate("document.getElementById('begin-button')?.click() ?? false");
    await browser.waitFor(`document.readyState === "complete" && ${exactLocationPredicate(origin, "/kyc/mock-verification")}`);
    setPhase("approval");
    await browser.waitFor("Boolean(document.getElementById('approve-btn'))");
    await browser.evaluate("document.getElementById('approve-btn')?.click() ?? false");
    setPhase("pending");
    await browser.waitForRecordedNavigation("pending");
    await browser.waitFor(`document.getElementById('action-button')?.textContent === 'Continue' && ${exactLocationPredicate(origin, "/issue/pending.html")}`);
    await browser.evaluate("document.getElementById('action-button')?.click() ?? false");
    setPhase("complete");
    await browser.waitFor(`document.readyState === "complete" && ${exactLocationPredicate(origin, "/issue/complete.html")}`);
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
