// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3] ?? "flow";

if (!endpoint || !["flow", "restored"].includes(mode)) {
  throw new Error("usage: node android-wallet-flow.mjs <cdp-websocket-url> <flow|restored>");
}

const socket = new WebSocket(endpoint);
let nextId = 1;
const pending = new Map();

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) {
      reject(new Error(message.error.message));
    } else {
      resolve(message.result);
    }
  }
});

await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", () => reject(new Error("CDP WebSocket connection failed")), {
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
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text ?? "Android WebView evaluation failed");
  }
  return result.result.value;
}

async function waitFor(expression, description, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluate(expression)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${description}`);
}

function buttonExpression(label) {
  return `Array.from(document.querySelectorAll("button")).find((element) => element.textContent.trim() === ${JSON.stringify(label)})`;
}

async function waitForButton(label) {
  await waitFor(`Boolean(${buttonExpression(label)})`, `button ${label}`);
}

async function clickButton(label) {
  await waitForButton(label);
  const clicked = await evaluate(`(() => {
    const element = ${buttonExpression(label)};
    if (!element || element.disabled) return false;
    element.click();
    return true;
  })()`);
  if (!clicked) {
    throw new Error(`button ${label} was disabled`);
  }
}

async function clickButtonByLabel(label) {
  const selector = `button[aria-label=${JSON.stringify(label)}]`;
  await waitFor(
    `Boolean(document.querySelector(${JSON.stringify(selector)}))`,
    `button labelled ${label}`,
  );
  const clicked = await evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element || element.disabled) return false;
    element.click();
    return true;
  })()`);
  if (!clicked) {
    throw new Error(`button labelled ${label} was disabled`);
  }
}

async function setInput(label, value) {
  const selector = `input[aria-label=${JSON.stringify(label)}]`;
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(selector)}))`, `input ${label}`);
  await evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(element, ${JSON.stringify(value)});
    element.dispatchEvent(new Event("input", { bubbles: true }));
    return element.value;
  })()`);
}

try {
  await command("Runtime.enable");
  await waitFor("document.readyState === 'complete'", "Dioxus document");

  if (mode === "flow") {
    await clickButton("Create and continue");
    await clickButtonByLabel("Activate protected Midnight account");
    await waitForButton("Use my receive address");

    await clickButton("Sync DUST");
    await waitFor(
      "document.body.innerText.includes('12 DUST')",
      "exact simulated DUST balance",
    );
    await waitForButton("Resync DUST");

    await clickButton("Show QR");
    await waitFor(
      "Boolean(document.querySelector('.address-qr__frame svg'))",
      "rendered receive QR",
    );
    const qrRendered = await evaluate(
      "Boolean(document.querySelector('.address-qr__frame svg'))",
    );
    await clickButton("Hide QR");
    const shieldedAddressRendered = await evaluate(`(() => {
      const rows = Array.from(document.querySelectorAll('.address-row'));
      return rows.some((row) =>
        row.innerText.includes('Shielded') && row.querySelector('code')?.innerText.startsWith('mn_shield-addr_')
      );
    })()`);

    await clickButton("Use my receive address");
    await setInput("Amount in NIGHT", "1.5");
    await clickButton("Review transfer");
    await clickButtonByLabel("Authorize reviewed NIGHT transfer");
    await clickButtonByLabel("Prove and submit NIGHT transfer");
    await waitFor(
      "document.body.innerText.includes('Transfer submitted')",
      "simulated transfer inclusion",
    );

    const publicResult = await evaluate(`(() => ({
      submitted: document.body.innerText.includes("Transfer submitted"),
      simulated: document.body.innerText.includes("Mode: simulated"),
      dustSynced: document.body.innerText.includes("12 DUST"),
    }))()`);
    const result = { ...publicResult, qrRendered, shieldedAddressRendered };
    if (!result.submitted || !result.simulated || !result.dustSynced || !result.qrRendered || !result.shieldedAddressRendered) {
      throw new Error("Android standalone wallet flow did not expose the expected public result");
    }
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } else {
    await waitForButton("Assets");
    await clickButton("Assets");
    await waitForButton("Activate development wallet");
    const restored = await evaluate(`(() => ({
      profileRestored: !document.body.innerText.includes("Create your wallet profile"),
      developmentRootReset: document.body.innerText.includes("Activate protected test account"),
    }))()`);
    if (!restored.profileRestored || !restored.developmentRootReset) {
      throw new Error("Android restart did not restore only public profile metadata");
    }
    process.stdout.write(`${JSON.stringify(restored)}\n`);
  }
} finally {
  socket.close();
}
