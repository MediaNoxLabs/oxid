// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3];
const modes = new Set([
  "prepare-scan",
  "assert-cancelled",
  "assert-timeout",
  "assert-unavailable",
  "assert-qr-offer",
  "assert-app-link",
]);
if (!endpoint || !modes.has(mode)) {
  throw new Error(
    "usage: node android-identity-ingress.mjs <cdp-websocket-url> " +
      "<prepare-scan|assert-cancelled|assert-timeout|assert-unavailable|assert-qr-offer|assert-app-link>",
  );
}

const socket = new WebSocket(endpoint);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (!message.id || !pending.has(message.id)) return;
  const { resolve, reject } = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) reject(new Error(message.error.message));
  else resolve(message.result);
});
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", () => reject(new Error("CDP connection failed")), {
    once: true,
  });
});

function command(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await command("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) throw new Error("Android WebView evaluation failed");
  return result.result.value;
}

async function waitFor(expression, description, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluate(expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${description}`);
}

function button(label) {
  return `Array.from(document.querySelectorAll("button")).find((element) => element.textContent.trim() === ${JSON.stringify(label)})`;
}

async function click(label) {
  await waitFor(
    `(() => { const element = ${button(label)}; return Boolean(element && !element.disabled && !element.closest("[inert]")); })()`,
    `enabled, interactive ${label} button`,
  );
  const clicked = await evaluate(`(() => {
    const element = ${button(label)};
    if (!element || element.disabled) return false;
    element.click();
    return true;
  })()`);
  if (!clicked) throw new Error(`could not click ${label}`);
}

async function ensureProfile() {
  await waitFor("Boolean(document.body)", "document body");
  if (await evaluate(`Boolean(${button("Create new wallet")})`)) {
    await click("Create new wallet");
    await waitFor(`Boolean(${button("Create and continue")})`, "wallet-name step");
  }
  if (await evaluate(`Boolean(${button("Create and continue")})`)) {
    await click("Create and continue");
    await waitFor(`Boolean(${button("Skip for now")})`, "wallet-protection step", 60_000);
  }
  if (await evaluate(`Boolean(${button("Skip for now")})`)) {
    await click("Skip for now");
  }
  await waitFor(`Boolean(${button("Scan")})`, "Scan action");
}

function noImportedRequestExpression() {
  return `!document.body.innerText.includes("recognized as a credential offer")
    && !document.body.innerText.includes("recognized as a DID login request")
    && !document.body.innerText.includes("recognized as a credential presentation")`;
}

try {
  await ensureProfile();
  if (mode === "prepare-scan") {
    await click("Scan");
    await waitFor(
      `(() => { const element = ${button("Scan")}; return Boolean(element && element.disabled); })()`,
      "active native scanner handoff",
    );
  } else if (mode === "assert-qr-offer") {
    await waitFor(
      'document.body.innerText.includes("QR recognized as a credential offer. Review the request before consent.")',
      "strict QR review boundary",
    );
    const routedOnlyToOffer = await evaluate(`
      !document.body.innerText.includes("recognized as a DID login request")
      && !document.body.innerText.includes("recognized as a credential presentation")
      && Boolean(${button("Dismiss identity request")})
    `);
    if (!routedOnlyToOffer) {
      throw new Error("QR payload entered an unrelated identity review");
    }
    await click("Dismiss identity request");
  } else if (mode === "assert-app-link") {
    await waitFor(
      'document.body.innerText.includes("App link recognized as a credential offer. Review the request before consent.")',
      "strict app-link review boundary",
    );
    const routedOnlyToOffer = await evaluate(`
      !document.body.innerText.includes("recognized as a DID login request")
      && !document.body.innerText.includes("recognized as a credential presentation")
      && Boolean(${button("Dismiss identity request")})
    `);
    if (!routedOnlyToOffer) {
      throw new Error("app link entered an unrelated identity review");
    }
    await click("Dismiss identity request");
  } else {
    const expected = {
      "assert-cancelled": "QR scan cancelled.",
      "assert-timeout": "QR scan timed out; no request was imported.",
      "assert-unavailable":
        "Camera scanning is unavailable here. Paste or load the request in the identity page instead.",
    }[mode];
    await waitFor(
      `document.body.innerText.includes(${JSON.stringify(expected)})`,
      expected,
      mode === "assert-timeout" ? 75_000 : 15_000,
    );
    if (!(await evaluate(noImportedRequestExpression()))) {
      throw new Error(`${mode} imported or classified a request`);
    }
  }
  process.stdout.write(`${JSON.stringify({ mode, passed: true })}\n`);
} finally {
  socket.close();
}
