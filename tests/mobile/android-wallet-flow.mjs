// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3] ?? "flow";

if (!endpoint || !["flow", "restored", "app-link", "native-authorize", "native-custody", "native-restored"].includes(mode)) {
  throw new Error("usage: node android-wallet-flow.mjs <cdp-websocket-url> <flow|restored|app-link|native-authorize|native-custody|native-restored>");
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

async function waitForButton(label, timeoutMs = 15_000) {
  await waitFor(`Boolean(${buttonExpression(label)})`, `button ${label}`, timeoutMs);
}

async function clickButton(label) {
  await waitFor(
    `(() => { const element = ${buttonExpression(label)}; return Boolean(element && !element.disabled); })()`,
    `enabled button ${label}`,
  );
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
    `(() => { const element = document.querySelector(${JSON.stringify(selector)}); return Boolean(element && !element.disabled); })()`,
    `enabled button labelled ${label}`,
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
    await waitForButton("Use my receive address", 90_000);
    await waitForButton("Scan QR");

    await clickButton("Sync DUST");
    await waitFor(
      "document.body.innerText.includes('12 DUST')",
      "exact simulated DUST balance",
    );
    await waitForButton("Resync DUST");

    await clickButton("Sync shielded assets");
    await waitFor(
      "document.body.innerText.includes('1 shielded notes') && document.body.innerText.includes('5000000 atomic units')",
      "exact simulated shielded note and token balance",
    );
    await waitForButton("Resync shielded assets");

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

    await clickButtonByLabel("Copy Unshielded receive address");
    await waitFor(
      "document.body.innerText.includes('Public receive address copied to the native clipboard.')",
      "native public-address clipboard confirmation",
    );
    const publicAddressCopied = await evaluate(
      "document.body.innerText.includes('Public receive address copied to the native clipboard.')",
    );

    await clickButton("Use my receive address");
    await setInput("Amount in NIGHT", "1.5");
    await clickButton("Review transfer");
    await clickButtonByLabel("Authorize reviewed NIGHT transfer");
    await clickButtonByLabel("Prove and submit NIGHT transfer");
    await clickButtonByLabel("Cancel NIGHT transfer submission");
    await waitForButton("Retry safe submission");
    await clickButton("Retry safe submission");
    await clickButtonByLabel("Prove and submit NIGHT transfer");
    await waitFor(
      "document.body.innerText.includes('Transfer submitted')",
      "simulated transfer inclusion",
    );

    const walletResult = await evaluate(`(() => ({
      submitted: document.body.innerText.includes("Transfer submitted"),
      simulated: document.body.innerText.includes("Mode: simulated"),
      dustSynced: document.body.innerText.includes("12 DUST"),
      shieldedSynced: document.body.innerText.includes("1 shielded notes")
        && document.body.innerText.includes("5000000 atomic units"),
    }))()`);
    await clickButton("DIDs");
    await waitForButton("Create standalone DID");
    await clickButton("Create standalone DID");
    await waitFor(
      "document.body.innerText.includes('standalone-1') && document.body.innerText.includes('Manage this DID')",
      "created managed standalone DID",
    );
    await evaluate(`(() => {
      const manager = document.querySelector('.did-manager');
      if (!manager) return false;
      manager.open = true;
      const input = manager.querySelector('input[type="text"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      setter.call(input, "https://example.test/android-wallet");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      const confirmation = manager.querySelector('input[type="checkbox"]');
      confirmation.click();
      return confirmation.checked;
    })()`);
    await waitFor(
      `(() => {
        const apply = Array.from(document.querySelectorAll('.did-manager button'))
          .find((button) => button.textContent.trim() === 'Apply DID update');
        return Boolean(apply && !apply.disabled);
      })()`,
      "enabled managed DID update",
    );
    await clickButton("Apply DID update");
    await waitFor(
      "document.body.innerText.includes('standalone-2')",
      "managed DID update",
    );
    await clickButton("Use standalone login request");
    await clickButton("Preview login request");
    await waitFor(
      "document.body.innerText.includes('DID authentication preview') && document.body.innerText.includes('Authenticate with the selected DID.')",
      "SIOPv2 DID authentication preview",
    );
    await evaluate(`(() => {
      const consent = document.querySelector('#self-issued-authentication-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Authenticate with DID");
    await waitFor(
      "document.body.innerText.includes('DID authentication succeeded and the standalone verifier independently validated the proof.')",
      "verified SIOPv2 DID authentication",
    );
    const didAuthenticated = await evaluate(
      "document.body.innerText.includes('DID authentication succeeded and the standalone verifier independently validated the proof.')",
    );
    await clickButton("Credentials");
    await waitForButton("Use standalone demo offer");
    await clickButton("Use standalone demo offer");
    await clickButton("Preview credential offer");
    await waitFor(
      "document.body.innerText.includes('Credential offer preview') && document.body.innerText.includes('Digital Passport')",
      "OID4VCI credential offer preview",
    );
    await evaluate(`(() => {
      const consent = document.querySelector('#credential-issuance-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Accept and issue credential");
    await waitFor(
      "document.body.innerText.includes('Credential issued, verified, and stored in the protected inventory.') && document.body.innerText.includes('valid')",
      "issued and verified OID4VCI credential",
    );
    await clickButton("Use standalone verifier request");
    await clickButton("Preview presentation request");
    await waitFor(
      "document.body.innerText.includes('Presentation preview') && document.body.innerText.includes('Requested claims') && document.body.innerText.includes('No presentation or vp_token has been generated.')",
      "claim-free OpenID4VP presentation preview",
    );
    await evaluate(`(() => {
      const consent = document.querySelector('#credential-presentation-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Consent and present");
    await waitFor(
      "document.body.innerText.includes('The holder authorized this exact presentation, but Compact proving is unavailable. No presentation or vp_token was generated.')",
      "fail-closed Compact presentation proof gate",
    );
    const presentationProofGated = await evaluate(
      "document.body.innerText.includes('No presentation or vp_token was generated.')",
    );
    const claimsHiddenByDefault = await evaluate(
      "Boolean(document.querySelector('.passport-claims')) && !document.body.innerText.includes('Alice') && !document.body.innerText.includes('Example')",
    );
    await clickButtonByLabel("Reveal First name locally");
    await waitFor(
      "document.body.innerText.includes('Alice')",
      "explicit local first-name reveal",
    );
    await clickButtonByLabel("Hide First name");
    await waitFor(
      "!document.body.innerText.includes('Alice')",
      "hidden local first-name value",
    );
    await clickButtonByLabel("Reveal Last name locally");
    await waitFor(
      "document.body.innerText.includes('Example')",
      "explicit local last-name reveal",
    );
    await clickButtonByLabel("Hide Last name");
    await waitFor(
      "!document.body.innerText.includes('Example')",
      "hidden local last-name value",
    );
    const thresholdAvailable = await evaluate(
      "Boolean(document.querySelector('input[aria-label=\"Age threshold\"]'))",
    );
    await clickButton("Preview disclosure plan");
    await waitFor(
      "document.body.innerText.includes('local preview ready · local preview only · no presentation generated')",
      "claim-free local disclosure preview",
    );
    const disclosurePreviewed = await evaluate(
      "document.body.innerText.includes('local preview ready · local preview only · no presentation generated')",
    );
    await clickButton("Reverify");
    await waitForButton("Reverify");
    const credentialVerified = await evaluate(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('valid')",
    );

    await clickButton("Vault");
    await waitFor(
      "document.body.innerText.includes('Owner-private durable conformance ledger') && document.body.innerText.includes('survives app restart')",
      "durable standalone Passport Vault state label",
    );
    const vaultStatePersistent = await evaluate(
      "document.body.innerText.includes('Owner-private durable conformance ledger') && document.body.innerText.includes('no on-chain transaction submitted')",
    );
    await waitFor(
      "document.body.innerText.includes('Deterministic simulation') && document.body.innerText.includes('no node broadcast occurs')",
      "truthfully labelled deterministic vault-call mode",
    );
    await clickButton("Read contract state");
    await waitFor(
      "document.body.innerText.includes('Contract state') && document.body.innerText.includes('simulated')",
      "simulated Passport Vault contract state",
    );
    await clickButton("Review contract call");
    await clickButton("Authorize exact call");
    await clickButton("Prove and submit");
    await waitFor(
      "document.body.innerText.includes('Passport Vault call completed') && document.body.innerText.includes('Mode: simulated')",
      "simulated native Passport Vault call lifecycle",
    );
    const nativeVaultCallFlow = await evaluate(
      "document.body.innerText.includes('Passport Vault call completed') && document.body.innerText.includes('Mode: simulated') && document.body.innerText.includes('Transaction') && document.body.innerText.includes('Block')",
    );
    await waitForButton("Create confirmed lock");
    await setInput("Vault required issuing state", "US");
    await setInput("Vault required document number", "AB1234567");
    await clickButton("Create confirmed lock");
    await waitFor(
      "document.body.innerText.includes('100 base units remaining') && document.body.innerText.includes('state US') && document.body.innerText.includes('document AB1234567')",
      "created policy-bound Passport Vault lock",
    );
    await clickButton("Deposit");
    await waitFor(
      "document.body.innerText.includes('110 base units remaining')",
      "Passport Vault deposit",
    );
    await clickButton("Claim with credential");
    await waitFor(
      "document.body.innerText.includes('100 base units remaining') && document.body.innerText.includes('Claims 1')",
      "credential-gated Passport Vault claim",
    );
    await clickButton("Withdraw");
    await waitFor(
      "document.body.innerText.includes('90 base units remaining')",
      "Passport Vault creator withdrawal",
    );
    const vaultFlow = await evaluate(
      "document.body.innerText.includes('Passport Vault') && document.body.innerText.includes('90 base units remaining') && document.body.innerText.includes('Claims 1')",
    );

    await clickButton("DIDs");
    await waitFor(
      "document.body.innerText.includes('standalone-2')",
      "managed DID before deactivation",
    );
    await evaluate(`(() => {
      const manager = document.querySelector('.did-manager');
      if (!manager) return false;
      manager.open = true;
      const select = manager.querySelector('select');
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set;
      setter.call(select, "deactivate");
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    })()`);
    await waitFor(
      "Boolean(document.querySelector('.did-manager input[type=checkbox]'))",
      "DID deactivation confirmation",
    );
    await evaluate(`(() => {
      const checkbox = document.querySelector('.did-manager input[type=checkbox]');
      checkbox.click();
      return checkbox.checked;
    })()`);
    await clickButton("Deactivate DID");
    await waitFor(
      "document.body.innerText.includes('Deactivated')",
      "deactivated managed DID",
    );
    const didManaged = await evaluate(
      "document.body.innerText.includes('Deactivated') && document.body.innerText.includes('Manage this DID')",
    );
    await waitForButton("Resolve and save");
    await clickButton("Resolve and save");
    await waitFor(
      "document.body.innerText.includes('standalone-fixture-v2')",
      "resolved standalone DID",
    );
    const didResolved = await evaluate(
      "document.body.innerText.includes('standalone-fixture-v2')",
    );
    await clickButton("Credentials");
    await waitFor(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('valid') && document.body.innerText.includes('Proof')",
      "verified issued credential",
    );
    const result = { ...walletResult, claimsHiddenByDefault, credentialVerified, didAuthenticated, didManaged, didResolved, disclosurePreviewed, nativeVaultCallFlow, presentationProofGated, publicAddressCopied, qrRendered, shieldedAddressRendered, thresholdAvailable, vaultFlow, vaultStatePersistent };
    if (!result.submitted || !result.simulated || !result.dustSynced || !result.shieldedSynced || !result.claimsHiddenByDefault || !result.credentialVerified || !result.didAuthenticated || !result.didManaged || !result.didResolved || !result.disclosurePreviewed || !result.nativeVaultCallFlow || !result.presentationProofGated || !result.publicAddressCopied || !result.qrRendered || !result.shieldedAddressRendered || !result.thresholdAvailable || !result.vaultFlow || !result.vaultStatePersistent) {
      throw new Error(`Android standalone wallet flow did not expose the expected public result: ${JSON.stringify(result)}`);
    }
    await clickButton("Assets");
    await clickButtonByLabel("Share Unshielded receive address");
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } else if (mode === "restored") {
    await waitForButton("Assets");
    await clickButton("Assets");
    await waitForButton("Activate development wallet");
    const walletRestored = await evaluate(`(() => ({
      profileRestored: !document.body.innerText.includes("Create your wallet profile"),
      developmentRootReset: document.body.innerText.includes("Activate protected test account"),
      submissionRestored: document.body.innerText.includes("Transfer included"),
    }))()`);
    await clickButton("DIDs");
    await waitFor(
      "document.body.innerText.includes('standalone-fixture-v2')",
      "restored DID inventory",
    );
    const didRestored = await evaluate(
      "document.body.innerText.includes('standalone-fixture-v2')",
    );
    const managedDidRestored = await evaluate(
      "document.body.innerText.includes('standalone-3') && document.body.innerText.includes('Deactivated')",
    );
    await clickButton("Credentials");
    await waitFor(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('valid') && Boolean(document.querySelector('.passport-claims')) && !document.body.innerText.includes('Alice') && !document.body.innerText.includes('Example')",
      "restored credential inventory",
    );
    await waitForButton("Reverify");
    await clickButton("Preview disclosure plan");
    await waitFor(
      "document.body.innerText.includes('local preview ready · local preview only · no presentation generated')",
      "restored protected disclosure preview",
    );
    const credentialRestored = await evaluate(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('valid') && !document.body.innerText.includes('Alice') && !document.body.innerText.includes('Example')",
    );
    await clickButton("Vault");
    await waitFor(
      "document.body.innerText.includes('90 base units remaining') && document.body.innerText.includes('Claims 1') && document.body.innerText.includes('Owner-private durable conformance ledger')",
      "restored standalone Passport Vault accounting",
    );
    const vaultRestored = await evaluate(
      "document.body.innerText.includes('90 base units remaining') && document.body.innerText.includes('Claims 1') && document.body.innerText.includes('survives app restart')",
    );
    const restored = { ...walletRestored, credentialRestored, didRestored, managedDidRestored, vaultRestored };
    if (!restored.profileRestored || !restored.developmentRootReset || !restored.submissionRestored || !restored.credentialRestored || !restored.didRestored || !restored.managedDidRestored || !restored.vaultRestored) {
      throw new Error("Android restart did not restore the expected public and owner-private state");
    }
    process.stdout.write(`${JSON.stringify(restored)}\n`);
  } else if (mode === "native-authorize") {
    const createProfile = await evaluate(`Boolean(${buttonExpression("Create and continue")})`);
    if (createProfile) await clickButton("Create and continue");
    await clickButton("Settings");
    await waitFor(
      "document.body.textContent.includes('Wallet protection') && !document.body.textContent.includes('Checking custody capability')",
      "settled native protection settings card",
    );
    const securityAction = await evaluate(`(() => {
      if (document.querySelector('button[aria-label="Initialize wallet"]')) return 'Initialize wallet';
      if (document.querySelector('button[aria-label="Unlock wallet"]')) return 'Unlock wallet';
      if (document.querySelector('button[aria-label="Lock wallet"]')) return 'already unlocked';
      return '';
    })()`);
    if (!securityAction) {
      throw new Error("native custody did not expose an initialization or unlock action");
    }
    if (securityAction !== "already unlocked") {
      await clickButtonByLabel(securityAction);
    }
    process.stdout.write(`${JSON.stringify({ mode, securityAction })}\n`);
  } else if (mode === "native-custody" || mode === "native-restored") {
    await clickButton("Assets");
    await waitFor(
      "document.body.textContent.includes('Wallet overview')",
      "Assets page before custody refresh",
    );
    await clickButton("Settings");
    await waitFor(
      "document.body.textContent.includes('Local controls') && document.body.textContent.includes('Wallet protection') && !document.body.textContent.includes('Checking custody capability')",
      "refreshed native protection settings card",
    );
    const refreshedStatus = await evaluate(`(() => {
      const card = Array.from(document.querySelectorAll('.settings-card'))
        .find((element) => element.textContent.includes('Wallet protection'));
      return card?.textContent.trim() ?? '';
    })()`);
    if (!refreshedStatus.includes("Unlocked · Operating system") &&
        !refreshedStatus.includes("Unlocked · Hardware backed")) {
      throw new Error(`native custody authorization did not persist: ${refreshedStatus}`);
    }
    await clickButton("Assets");
    await waitFor(
      "document.body.textContent.includes('Wallet overview')",
      "Assets page before native account activation",
    );
    await clickButtonByLabel("Activate protected Midnight account");
    await waitFor(
      `Boolean(${buttonExpression("Use my receive address")}) || Boolean(document.querySelector('[role="alert"]'))`,
      "native account derivation or a safe failure",
      90_000,
    );
    const accountFailure = await evaluate(
      "document.querySelector('[role=\"alert\"]')?.textContent.trim() ?? ''",
    );
    if (accountFailure) {
      throw new Error(`native custody account activation failed safely: ${accountFailure}`);
    }
    const receiveAddress = await evaluate(`(() => {
      const row = Array.from(document.querySelectorAll('.address-row'))
        .find((element) => element.innerText.includes('Unshielded'));
      return row?.querySelector('code')?.innerText ?? '';
    })()`);
    if (!receiveAddress.startsWith("mn_addr_")) {
      throw new Error("native custody did not derive the expected public Midnight address");
    }
    await clickButton("Settings");
    await waitFor(
      "document.body.textContent.includes('Wallet protection') && !document.body.textContent.includes('Checking custody capability')",
      "settled native protection settings card",
    );
    const nativeStatus = await evaluate(`(() => {
      const card = Array.from(document.querySelectorAll('.settings-card'))
        .find((element) => element.textContent.includes('Wallet protection'));
      return card?.textContent ?? '';
    })()`);
    const protection = nativeStatus.includes('Hardware backed')
      ? 'hardware_backed'
      : nativeStatus.includes('Operating system')
        ? 'operating_system'
        : '';
    if (!protection || (!nativeStatus.includes('Unlocked') && !nativeStatus.includes('Locked'))) {
      throw new Error(`native custody reported an unexpected status: ${nativeStatus}`);
    }
    if (mode === "native-custody") {
      if (nativeStatus.includes('Unlocked')) {
        await clickButtonByLabel("Lock wallet");
        await waitFor(
          "document.body.innerText.includes('Locked · Operating system') || document.body.innerText.includes('Locked · Hardware backed')",
          "explicitly locked native protection status",
        );
      }
    }
    process.stdout.write(`${JSON.stringify({ mode, protection, receiveAddress })}\n`);
  } else {
    await waitFor(
      "document.body.innerText.includes('App link recognized as a credential offer. Review the request before consent.')",
      "strictly routed credential-offer app link",
    );
    await waitForButton("Dismiss identity request");
    const routed = await evaluate(`(() => ({
      credentialsPage: document.body.innerText.includes("Credentials"),
      consentPending: document.body.innerText.includes("App link recognized as a credential offer. Review the request before consent."),
      dismissAvailable: Boolean(${buttonExpression("Dismiss identity request")}),
    }))()`);
    if (!routed.credentialsPage || !routed.consentPending || !routed.dismissAvailable) {
      throw new Error(`Android app link did not enter the preview/consent boundary: ${JSON.stringify(routed)}`);
    }
    await clickButton("Dismiss identity request");
    process.stdout.write(`${JSON.stringify(routed)}\n`);
  }
} finally {
  socket.close();
}
