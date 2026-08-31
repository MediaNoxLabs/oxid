// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3];
const controlOrigin = process.env.OXID_PORTAL_CONTROL_ORIGIN;
const capabilityChunks = [];
let capabilityBytes = 0;
for await (const chunk of process.stdin) {
  capabilityBytes += chunk.length;
  if (capabilityBytes > 64) throw new Error("invalid Android Portal control input");
  capabilityChunks.push(chunk);
}
const controlCapabilityBytes = Buffer.concat(capabilityChunks);
const controlCapability = controlCapabilityBytes.toString("utf8");
for (const chunk of capabilityChunks) chunk.fill(0);
controlCapabilityBytes.fill(0);
const modes = new Set([
  "prepare-holder",
  "route-refuse",
  "malformed",
  "protocol-error",
  "protocol-timeout",
  "issue-error",
  "issue",
  "cold-route",
  "restored",
]);
if (!endpoint || !modes.has(mode) || controlOrigin !== "http://127.0.0.1:18095"
    || !/^[0-9a-f]{64}$/u.test(controlCapability ?? "")) {
  throw new Error("invalid Android Portal test arguments");
}

const CDP_OPEN_TIMEOUT_MS = 10_000;
const CDP_COMMAND_TIMEOUT_MS = 10_000;
const CONTROL_REQUEST_TIMEOUT_MS = 10_000;
const socket = new WebSocket(endpoint);
let nextId = 1;
let terminalError = null;
const pending = new Map();
const measurements = {};
let measuredCounterDelta = {
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

function rejectPending(error) {
  terminalError ??= error;
  for (const { reject, timer } of pending.values()) {
    clearTimeout(timer);
    reject(terminalError);
  }
  pending.clear();
}

socket.addEventListener("message", (event) => {
  let message;
  try {
    message = JSON.parse(event.data);
  } catch {
    rejectPending(new Error("CDP returned an invalid message"));
    socket.close();
    return;
  }
  if (!message.id || !pending.has(message.id)) return;
  const { resolve, reject, timer } = pending.get(message.id);
  clearTimeout(timer);
  pending.delete(message.id);
  if (message.error) reject(new Error(message.error.message));
  else resolve(message.result);
});
socket.addEventListener("close", () => rejectPending(new Error("CDP connection closed")));
socket.addEventListener("error", () => rejectPending(new Error("CDP connection failed")));

await new Promise((resolve, reject) => {
  const timer = setTimeout(() => {
    socket.close();
    reject(new Error("timed out opening CDP connection"));
  }, CDP_OPEN_TIMEOUT_MS);
  socket.addEventListener("open", () => {
    clearTimeout(timer);
    resolve();
  }, { once: true });
  socket.addEventListener("error", () => {
    clearTimeout(timer);
    reject(terminalError ?? new Error("CDP connection failed"));
  }, { once: true });
  socket.addEventListener("close", () => {
    clearTimeout(timer);
    reject(terminalError ?? new Error("CDP connection closed before opening"));
  }, { once: true });
});

function command(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    if (terminalError || socket.readyState !== WebSocket.OPEN) {
      reject(terminalError ?? new Error("CDP connection is not open"));
      return;
    }
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`timed out waiting for CDP command ${method}`));
    }, CDP_COMMAND_TIMEOUT_MS);
    pending.set(id, { resolve, reject, timer });
    try {
      socket.send(JSON.stringify({ id, method, params }));
    } catch (error) {
      clearTimeout(timer);
      pending.delete(id);
      reject(error);
    }
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

function strictReviewBoundaryExpression() {
  return `(() => ({
    credentials: document.body.innerText.includes("Credentials"),
    dismiss: Boolean(${button("Dismiss identity request")}),
    hidden: document.body.innerText.includes("Its one-time grant is hidden while you review it."),
    noEditableOffer: !document.querySelector("#credential-offer"),
    noConsent: !document.querySelector("#credential-issuance-consent")
  }))()`;
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

function controlFetch(path, options = {}) {
  return fetch(`${controlOrigin}${path}`, {
    ...options,
    headers: { ...options.headers, Authorization: `Bearer ${controlCapability}` },
    signal: AbortSignal.timeout(CONTROL_REQUEST_TIMEOUT_MS),
  });
}

async function assertRouted() {
  try {
    await waitFor(
      'document.body.innerText.includes("App link recognized as a credential offer. Review the request before consent.")',
      "credential-offer route",
    );
  } catch (error) {
    const handoff = await controlFetch("/handoff-status", {
      cache: "no-store",
    }).then((response) => response.json());
    throw new Error(`credential-offer route unavailable with handoff state ${handoff.state}: ${error.message}`);
  }
  await waitFor(
    `Object.values(${strictReviewBoundaryExpression()}).every(Boolean)`,
    "strict credential-offer boundary",
  );
  const boundary = await evaluate(strictReviewBoundaryExpression());
  if (!Object.values(boundary).every(Boolean)) {
    throw new Error(`offer crossed the strict review boundary: ${JSON.stringify(boundary)}`);
  }
}

async function preview() {
  await click("Preview credential offer");
}

async function counters() {
  const response = await controlFetch("/counters", { cache: "no-store" });
  if (!response.ok) throw new Error("Portal counters unavailable");
  return response.json();
}

function counterDelta(after, before) {
  return Object.fromEntries(Object.keys(before).map((key) => [key, after[key] - before[key]]));
}

function assertExactCounterDelta(before, after, expected, scenario) {
  const delta = counterDelta(after, before);
  const exact = Object.fromEntries(Object.keys(before).map((key) => [key, expected[key] ?? 0]));
  if (JSON.stringify(delta) !== JSON.stringify(exact)) {
    throw new Error(`${scenario} counters were not exact: ${JSON.stringify(delta)}`);
  }
  return delta;
}

async function setProxyMode(mode) {
  const response = await controlFetch("/proxy-mode", { method: "POST", body: mode });
  if (!response.ok) throw new Error("Portal proxy mode unavailable");
}

try {
  await ensureProfile();
  if (mode === "prepare-holder") {
    await click("Wallet");
    await waitFor(
      `Boolean(${button("Activate development wallet")} || ${button("Use my receive address")})`,
      "wallet activation state",
      30_000,
    );
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
    await waitFor(
      'document.body.innerText.includes("Manage this DID") || Array.from(document.querySelectorAll(".field-error")).some((element) => element.textContent.trim() === "protected DID key operation is unavailable")',
      "managed DID terminal state",
      30_000,
    );
    if (await evaluate('Array.from(document.querySelectorAll(".field-error")).some((element) => element.textContent.trim() === "protected DID key operation is unavailable")')) {
      throw new Error("managed DID creation ran without activated development custody");
    }
    Object.assign(measurements, { managedDidPrepared: true });
  } else if (mode === "route-refuse") {
    const start = await counters();
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
    const consentBoundary = await evaluate(`(() => {
      const consent = document.querySelector("#credential-issuance-consent");
      const issue = ${button("Accept and issue credential")};
      return {
        consentUnchecked: Boolean(consent) && !consent.checked,
        issuanceDisabled: Boolean(issue) && issue.disabled
      };
    })()`);
    if (!Object.values(consentBoundary).every(Boolean)) {
      throw new Error(`preview crossed the consent boundary: ${JSON.stringify(consentBoundary)}`);
    }
    const before = await counters();
    assertExactCounterDelta(start, before, {
      authorizationMetadata: 1,
      issuerMetadata: 1,
    }, "refusal preflight");
    await click("Refuse offer");
    await waitFor(
      'document.body.innerText.includes("Credential offer refused; ephemeral protocol secrets were discarded.")',
      "refusal",
    );
    const after = await counters();
    measuredCounterDelta = assertExactCounterDelta(start, after, {
      authorizationMetadata: 1,
      issuerMetadata: 1,
    }, "refusal");
    await waitFor(
      `!Boolean(${button("Dismiss identity request")})`,
      "refusal clears the consumed router request and raw offer",
    );
    const refusalDelta = counterDelta(after, start);
    Object.assign(measurements, {
      consentInitiallyUnchecked: consentBoundary.consentUnchecked,
      exactOfferRouted: true,
      exactPreview: true,
      fiveQuestions: true,
      issuanceInitiallyDisabled: consentBoundary.issuanceDisabled,
      issuerResolutionCallsBeforeConsent: refusalDelta.issuerResolution,
      rawOfferClearedAfterPreview: true,
      refusalBeforeConsent: true,
      refusalSecretEndpointCalls: refusalDelta.token + refusalDelta.nonce + refusalDelta.credential,
      warmIngress: true,
    });
  } else if (mode === "malformed") {
    const start = await counters();
    await setProxyMode("malformed");
    await assertRouted();
    await preview();
    await waitFor('document.body.innerText.includes("The issuer metadata is not valid")', "strict malformed rejection");
    await waitFor(`!Boolean(${button("Dismiss identity request")})`, "malformed request cleanup");
    const after = await counters();
    measuredCounterDelta = assertExactCounterDelta(
      start,
      after,
      { issuerMetadata: 1 },
      "malformed response",
    );
    await setProxyMode("normal");
    Object.assign(measurements, { malformedRejected: true, warmIngress: true });
  } else if (mode === "protocol-error" || mode === "protocol-timeout") {
    const start = await counters();
    await setProxyMode(mode === "protocol-timeout" ? "timeout" : "unavailable");
    await assertRouted();
    await preview();
    const requestDeadline = Date.now() + 5_000;
    let observed = await counters();
    while (observed.issuerMetadata <= start.issuerMetadata && Date.now() < requestDeadline) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      observed = await counters();
    }
    if (observed.issuerMetadata <= start.issuerMetadata) {
      throw new Error(`${mode} did not reach the issuer metadata boundary`);
    }
    if (mode === "protocol-timeout") {
      await waitFor(
        `(() => { const element = ${button("Checking offer…")}; return Boolean(element && element.disabled); })()`,
        "accessible disabled offer-check busy state",
        5_000,
      );
    }
    await waitFor(
      `(() => {
        return !Boolean(${button("Checking offer…")})
          && Array.from(document.querySelectorAll('[role="status"]')).some((element) => {
            const text = element.textContent.trim();
            return text.length > 0 && text.length <= 512
              && !/(openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|https?:\\/\\/)/iu.test(text);
          });
      })()`,
      "payload-free terminal protocol failure",
      35_000,
    );
    await waitFor(`!Boolean(${button("Dismiss identity request")})`, "failed request cleanup");
    const after = await counters();
    measuredCounterDelta = assertExactCounterDelta(
      start,
      after,
      { issuerMetadata: mode === "protocol-error" ? 2 : 1 },
      mode,
    );
    await setProxyMode("normal");
    Object.assign(
      measurements,
      mode === "protocol-timeout"
        ? { timeoutRejected: true, warmIngress: true }
        : { unavailableRejected: true, warmIngress: true },
    );
  } else if (mode === "issue-error") {
    const start = await counters();
    await assertRouted();
    await preview();
    await waitFor('document.body.innerText.includes("Credential offer preview")', "Portal preview", 30_000);
    await waitFor('Boolean(document.querySelector("#credential-issuance-consent"))', "issuance consent");
    await evaluate('document.querySelector("#credential-issuance-consent").click()');
    await setProxyMode("unavailable");
    await click("Accept and issue credential");
    await waitFor(
      `(() => {
        const leave = ${button("Leave credential review")};
        return Boolean(leave && !leave.disabled)
          && Array.from(document.querySelectorAll('[role="status"]')).some((element) => {
            const text = element.textContent.trim();
            return text.length > 0 && text.length <= 512
              && !/(openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|did:|https?:\\/\\/)/iu.test(text);
          });
      })()`,
      "payload-free cleanup-unavailable locked review",
      35_000,
    );
    await setProxyMode("normal");
    const lockedReview = await evaluate(`(() => {
      const consent = document.querySelector("#credential-issuance-consent");
      const issue = ${button("Accept and issue credential")};
      return {
        consentCleared: Boolean(consent) && !consent.checked,
        preparedReviewRetained: document.body.innerText.includes("Credential offer preview"),
        issueDisabled: Boolean(issue && issue.disabled),
        noRawOfferDismissal: !Boolean(${button("Dismiss identity request")})
      };
    })()`);
    if (!Object.values(lockedReview).every(Boolean)) {
      throw new Error(`failed issuance did not retain its locked review: ${JSON.stringify(lockedReview)}`);
    }
    await click("Wallet");
    await waitFor(
      'Array.from(document.querySelectorAll("h1")).some((element) => element.textContent.trim() === "Credentials") && document.body.innerText.includes("Credential offer preview") && !document.querySelector("#credential-issuance-consent").checked',
      "failed issuance retained prepared review and route lock",
    );
    const counts = await counters();
    measuredCounterDelta = assertExactCounterDelta(start, counts, {
      authorizationMetadata: 1,
      issuerMetadata: 1,
      token: 1,
    }, "failed issuance");
    await click("Leave credential review");
    await waitFor(
      `!Boolean(${button("Leave credential review")}) && !document.body.innerText.includes("Credential offer preview")`,
      "safe credential review cleanup and navigation escape",
    );
    Object.assign(measurements, { issueErrorEscapedSafely: true, warmIngress: true });
  } else if (mode === "issue") {
    const start = await counters();
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
    if (!Object.values(result).every(Boolean)) {
      throw new Error(`Portal issuance UI evidence failed: ${JSON.stringify(result)}`);
    }
    measuredCounterDelta = assertExactCounterDelta(start, counts, {
      authorizationMetadata: 1,
      credential: 1,
      issuerMetadata: 1,
      issuerResolution: 1,
      issuerResolutionSuccess: 1,
      nonce: 1,
      token: 1,
    }, "successful issuance");
    Object.assign(measurements, {
      claimsHidden: result.claimsHidden,
      exactBundleImported: true,
      explicitConsent: true,
      managedAuthenticationProof: true,
      separateJubjubAssertionBinding: true,
      strictFinalExchange: true,
      warmIngress: true,
    });
  } else if (mode === "cold-route") {
    const start = await counters();
    await assertRouted();
    await click("Dismiss identity request");
    const after = await counters();
    measuredCounterDelta = assertExactCounterDelta(start, after, {}, "cold route");
    Object.assign(measurements, { coldIngress: true, oneItemIngress: true });
  } else if (mode === "restored") {
    await click("Wallet");
    await waitFor(`Boolean(${button("Activate development wallet")})`, "truthful development-custody reset");
    await click("Activate development wallet");
    await waitFor(
      `!document.querySelector('button[aria-label="Activate protected Midnight account"]') && Boolean(${button("Use my receive address")})`,
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
    Object.assign(measurements, { noStaleReverificationMarker: true });
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
      throw new Error("Reverify did not produce a fresh issuer-resolution success");
    }
    measuredCounterDelta = assertExactCounterDelta(beforeReverify, afterReverify, {
      issuerResolution: 1,
      issuerResolutionSuccess: 1,
    }, "restart reverification");
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
    Object.assign(measurements, {
      custodyReactivated: true,
      freshReverification: true,
      listedAfterRestart: true,
    });
  }
  process.stdout.write(`${JSON.stringify({ counterDelta: measuredCounterDelta, measurements, mode, passed: true })}\n`);
} finally {
  socket.close();
}
