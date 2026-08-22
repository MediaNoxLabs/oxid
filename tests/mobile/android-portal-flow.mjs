// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3];
const controlOrigin = process.env.OXID_PORTAL_CONTROL_ORIGIN;
const modes = new Set([
  "prepare-holder",
  "route-refuse",
  "malformed",
  "protocol-error",
  "issue",
  "cold-route",
  "restored",
]);
if (!endpoint || !modes.has(mode) || controlOrigin !== "http://127.0.0.1:18091") {
  throw new Error("invalid Android Portal test arguments");
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
  socket.addEventListener("error", () => reject(new Error("CDP connection failed")), { once: true });
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

async function waitFor(expression, description, timeoutMs = 20_000) {
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

async function click(label, timeoutMs = 20_000) {
  await waitFor(
    `(() => { const element = ${button(label)}; return Boolean(element && !element.disabled && !element.closest("[inert]")); })()`,
    `enabled ${label}`,
    timeoutMs,
  );
  if (!(await evaluate(`(() => { const element = ${button(label)}; element.click(); return true; })()`))) {
    throw new Error(`could not click ${label}`);
  }
}

async function ensureProfile() {
  await waitFor("Boolean(document.body)", "document body", 60_000);
  await waitFor(
    `Boolean(${button("Create new wallet")} || ${button("Create and continue")} || ${button("Skip for now")} || ${button("Home")})`,
    "onboarding or wallet",
    60_000,
  );
  if (await evaluate(`Boolean(${button("Create new wallet")})`)) {
    await click("Create new wallet");
    await waitFor(`Boolean(${button("Create and continue")})`, "wallet-name step");
  }
  if (await evaluate(`Boolean(${button("Create and continue")})`)) {
    await click("Create and continue");
    await waitFor(`Boolean(${button("Skip for now")})`, "wallet-protection step", 60_000);
  }
  if (await evaluate(`Boolean(${button("Skip for now")})`)) await click("Skip for now", 60_000);
  await waitFor(`Boolean(${button("Home")})`, "composed wallet", 60_000);
}

async function assertRouted() {
  await waitFor(
    'document.body.innerText.includes("App link recognized as a credential offer. Review the request before consent.")',
    "credential-offer route",
  );
  const boundary = await evaluate(`(() => ({
    credentials: document.body.innerText.includes("Credentials"),
    dismiss: Boolean(${button("Dismiss identity request")}),
    hidden: document.body.innerText.includes("Its one-time grant is hidden while you review it."),
    noEditableOffer: !document.querySelector("#credential-offer"),
    noConsent: !document.querySelector("#credential-issuance-consent")
  }))()`);
  if (!Object.values(boundary).every(Boolean)) {
    throw new Error(`offer crossed the strict review boundary: ${JSON.stringify(boundary)}`);
  }
}

async function preview() {
  await click("Preview credential offer");
}

async function counters() {
  const response = await fetch(`${controlOrigin}/counters`, { cache: "no-store" });
  if (!response.ok) throw new Error("Portal counters unavailable");
  return response.json();
}

try {
  await ensureProfile();
  if (mode === "prepare-holder") {
    await click("Wallet");
    if (await evaluate(`Boolean(${button("Activate development wallet")})`)) {
      await click("Activate development wallet");
      await waitFor(
        `!document.querySelector('button[aria-label="Activate protected Midnight account"]') && Boolean(${button("Use my receive address")})`,
        "activated local account",
        60_000,
      );
    }
    await click("Documents");
    await click("Manage identities");
    await click("Create standalone DID");
    await waitFor('document.body.innerText.includes("Manage this DID")', "managed DID", 30_000);
  } else if (mode === "route-refuse") {
    await assertRouted();
    await preview();
    await waitFor('document.body.innerText.includes("Credential offer preview")', "Portal preview", 30_000);
    await waitFor(
      `!document.body.innerText.includes("Its one-time grant is hidden while you review it.") && !Boolean(${button("Dismiss identity request")})`,
      "successful preview to clear the imported raw offer and hide router dismissal",
    );
    const questions = await evaluate(`[
      "Who is issuing it?", "What will you receive?", "Which identity receives it?",
      "Why add it?", "Unverified endpoint"
    ].every((value) => document.body.innerText.includes(value))`);
    if (!questions) throw new Error("Portal consent questions are incomplete");
    const before = await counters();
    if (before.token !== 0 || before.nonce !== 0 || before.credential !== 0) {
      throw new Error("Portal denial preflight contacted a secret endpoint");
    }
    await click("Refuse offer");
    await waitFor(
      'document.body.innerText.includes("Credential offer refused; ephemeral protocol secrets were discarded.")',
      "refusal",
    );
    const after = await counters();
    if (after.token !== 0 || after.nonce !== 0 || after.credential !== 0) {
      throw new Error("Portal refusal contacted a secret endpoint");
    }
    await waitFor(
      `!Boolean(${button("Dismiss identity request")})`,
      "refusal clears the consumed router request and raw offer",
    );
  } else if (mode === "malformed") {
    await assertRouted();
    await preview();
    await waitFor('document.body.innerText.includes("The issuer metadata is not valid")', "strict malformed rejection");
    await click("Dismiss identity request");
  } else if (mode === "protocol-error") {
    await assertRouted();
    await preview();
    await waitFor(
      'document.body.innerText.includes("This protocol is unavailable in the current build")',
      "payload-free protocol failure",
      35_000,
    );
    await click("Dismiss identity request");
  } else if (mode === "issue") {
    await assertRouted();
    await preview();
    await waitFor('document.body.innerText.includes("Credential offer preview")', "Portal preview", 30_000);
    await waitFor('Boolean(document.querySelector("#credential-issuance-consent"))', "issuance consent");
    await evaluate('document.querySelector("#credential-issuance-consent").click()');
    await click("Accept and issue credential");
    try {
      await waitFor(
        'document.body.innerText.includes("Credential issued, verified, and stored in the protected inventory.")',
        "Portal issuance",
        90_000,
      );
    } catch (error) {
      const diagnosticCounts = await counters();
      throw new Error(`Portal issuance failed with payload-free counters ${JSON.stringify(diagnosticCounts)}: ${error.message}`);
    }
    const result = await evaluate(`({
      valid: Array.from(document.querySelectorAll(".credential-record")).length === 1
        && document.body.innerText.includes("Valid"),
      policy: document.body.innerText.includes("Credential policy · issuer passed · time passed · trust passed · revocation not checked"),
      claimsHidden: !document.body.innerText.includes("John") && !document.body.innerText.includes("Doe")
    })`);
    const counts = await counters();
    if (!Object.values(result).every(Boolean) || counts.token !== 1 || counts.nonce !== 1 || counts.credential !== 1) {
      throw new Error(`Portal issuance evidence failed: ${JSON.stringify({ result, counts })}`);
    }
  } else if (mode === "cold-route") {
    await assertRouted();
    await click("Dismiss identity request");
  } else if (mode === "restored") {
    await click("Wallet");
    await waitFor(`Boolean(${button("Activate development wallet")})`, "truthful development-custody reset");
    await click("Activate development wallet");
    await waitFor(
      `!Boolean(${button("Activate development wallet")}) && Boolean(${button("Use my receive address")})`,
      "reactivated local account",
      45_000,
    );
    await click("Documents");
    await waitFor(
      'document.querySelectorAll(".credential-record").length === 1 && document.body.innerText.includes("Valid")',
      "encrypted credential restore",
      30_000,
    );
    // The restored credential already shows this exact policy summary
    // before the tap, so that text alone cannot prove reverification ran.
    // Require both the unchanged resolver delta and a fresh payload-free UI
    // marker that can be emitted only after the updated record is applied.
    if (await evaluate('Boolean(document.querySelector(".credential-reverification-success"))')) {
      throw new Error("fresh reverification marker was stale before reverify");
    }
    const beforeReverify = await counters();
    await click("Reverify");
    const reverifyDeadline = Date.now() + 30_000;
    let afterReverify = await counters();
    while (
      afterReverify.issuerResolutionSuccess <= beforeReverify.issuerResolutionSuccess
      && Date.now() < reverifyDeadline
    ) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      afterReverify = await counters();
    }
    if (!(afterReverify.issuerResolutionSuccess > beforeReverify.issuerResolutionSuccess)) {
      throw new Error(
        `Reverify did not produce a fresh issuer-resolution success: before=${beforeReverify.issuerResolutionSuccess} after=${afterReverify.issuerResolutionSuccess}`,
      );
    }
    await waitFor(
      `(() => { const element = ${button("Reverify")}; return Boolean(element && !element.disabled); })()`,
      "reverify busy completion",
      30_000,
    );
    await waitFor(
      'document.querySelector(".credential-reverification-success")?.textContent.trim() === "Credential reverification applied"',
      "fresh applied reverification marker",
      30_000,
    );
    const reverified = await evaluate(`(() => {
      const records = Array.from(document.querySelectorAll(".credential-record"));
      return {
        noOperationError: !document.querySelector(".credential-operation-error"),
        oneValidRecord: records.length === 1 && Array.from(records[0].querySelectorAll(".status-pill.success"))
          .some((element) => element.textContent.trim() === "Valid"),
        policy: records.length === 1 && records[0].innerText.includes(
          "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ),
        freshMarker: document.querySelector(".credential-reverification-success")?.textContent.trim()
          === "Credential reverification applied"
      };
    })()`);
    if (!Object.values(reverified).every(Boolean)) {
      throw new Error(`restored credential reverification UI evidence failed: ${JSON.stringify(reverified)}`);
    }
  }
  process.stdout.write(`${JSON.stringify({ mode, passed: true })}\n`);
} finally {
  socket.close();
}
