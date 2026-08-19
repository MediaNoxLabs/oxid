// SPDX-License-Identifier: Apache-2.0

const endpoint = process.argv[2];
const mode = process.argv[3] ?? "flow";
const backupRecoverySecret = "oxidandroidbackup2026";

if (!endpoint || !["flow", "live-account", "prepare-live-account-touch", "live-account-after-touch", "live-account-restarted", "restored", "app-link", "privacy-reveal", "privacy-rearmed", "backup-export", "backup-recover", "developer", "demo", "native-authorize", "native-custody", "native-restored"].includes(mode)) {
  throw new Error("usage: node android-wallet-flow.mjs <cdp-websocket-url> <flow|live-account|prepare-live-account-touch|live-account-after-touch|live-account-restarted|restored|app-link|privacy-reveal|privacy-rearmed|backup-export|backup-recover|developer|demo|native-authorize|native-custody|native-restored>");
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

async function openDocuments() {
  await clickButton("Documents");
}

async function openIdentities() {
  await openDocuments();
  await clickButton("Manage identities");
}

async function openSettings() {
  await clickButtonByLabel("Open profile menu");
  await clickButtonByLabel("Open settings");
}

async function openPassportVault() {
  await clickButton("Home");
  await waitFor(
    "document.querySelector('.home-card--identity')?.innerText.includes('Digital Passport') && document.querySelector('.home-security-strip')?.innerText.includes('Standalone custody')",
    "populated Home document and truthful standalone security summary",
  );
  await clickButtonByLabel("Open Passport Vault");
}

async function openWallet() {
  await clickButton("Wallet");
}

async function createFreshProfile() {
  await waitFor(
    `Boolean(${buttonExpression("Create new wallet")}) || Boolean(${buttonExpression("Wallet")})`,
    "first-run or restored profile readiness",
    30_000,
  );
  const createAvailable = await evaluate(`Boolean(${buttonExpression("Create new wallet")})`);
  if (!createAvailable) return;
  await clickButton("Create new wallet");
  await clickButton("Create and continue");
  await clickButton("Skip for now");
}

async function assertHomeComposition() {
  await clickButton("Home");
  await waitFor(
    `document.body.innerText.includes("Everything in one place")
      && Boolean(document.querySelector('.home-quick-actions'))
      && Boolean(document.querySelector('button[aria-label="Open Wallet NIGHT account"]'))
      && Boolean(document.querySelector('button[aria-label="Open Wallet shielded account"]'))
      && Boolean(document.querySelector('button[aria-label="Open newest document"]'))
      && Boolean(document.querySelector('button[aria-label="Open Passport Vault"]'))
      && Boolean(document.querySelector('button[aria-label="Open wallet security settings"]'))
      && Boolean(document.querySelector('button[aria-label="See all activity"]'))`,
    "five-part Home composition",
  );
  const truthful = await evaluate(`(() => {
    const labels = Array.from(document.querySelectorAll('.home-quick-action'))
      .map((element) => element.textContent.trim());
    return ["Receive", "Send", "Present", "Scan"].every((label) => labels.includes(label))
      && !document.body.innerText.includes("Backed up");
  })()`);
  if (!truthful) {
    throw new Error("Home quick actions or security capability truth did not match the composed design");
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

async function setInputById(identifier, value) {
  const selector = `#${identifier}`;
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(selector)}))`, `input ${identifier}`);
  await evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(element, ${JSON.stringify(value)});
    element.dispatchEvent(new Event("input", { bubbles: true }));
    return element.value;
  })()`);
}

async function clickConfirmation(label) {
  const expression = `Array.from(document.querySelectorAll("label")).find((element) => element.textContent.includes(${JSON.stringify(label)}))?.querySelector('input[type="checkbox"]')`;
  await waitFor(`Boolean(${expression})`, `confirmation ${label}`);
  const checked = await evaluate(`(() => {
    const element = ${expression};
    if (!element) return false;
    if (!element.checked) element.click();
    return element.checked;
  })()`);
  if (!checked) {
    throw new Error(`confirmation ${label} could not be selected`);
  }
}

async function clickCheckboxById(identifier) {
  const selector = `#${identifier}`;
  await waitFor(
    `Boolean(document.querySelector(${JSON.stringify(selector)}))`,
    `checkbox ${identifier}`,
  );
  const checked = await evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) return false;
    if (!element.checked) element.click();
    return element.checked;
  })()`);
  if (!checked) {
    throw new Error(`checkbox ${identifier} could not be selected`);
  }
}

try {
  await command("Runtime.enable");
  await waitFor("document.readyState === 'complete'", "Dioxus document");

  if (mode === "developer") {
    await waitFor(
      `document.querySelector('[data-ui-profile="OXID_UI_PROFILE_DEVELOPMENT"]')
        ?.innerText.includes("DEVELOPER PROFILE")`,
      "persistent developer-profile banner before onboarding",
    );
    await createFreshProfile();
    await clickButtonByLabel("Open profile menu");
    await clickButtonByLabel("Open developer capabilities");
    await waitFor(
      `document.body.innerText.includes("Capability manifest")
        && document.body.innerText.includes("oxid_capabilities_application")`,
      "shared capability manifest",
    );
    const result = await evaluate(`(() => {
      const signing = Array.from(document.querySelectorAll('.developer-capability-row'))
        .find((element) => element.innerText.includes('wallet.key.sign'));
      return {
        banner: Boolean(document.querySelector(
          '[data-ui-profile="OXID_UI_PROFILE_DEVELOPMENT"]'
        )),
        confirmationDeclared: Boolean(
          signing && signing.innerText.includes('confirmationRequired')
        ),
        secretInputExcluded: !document.body.innerText.includes('credential_offer'),
        sourceShared: document.body.innerText.includes('oxid_capabilities_application'),
      };
    })()`);
    if (!result.banner || !result.confirmationDeclared || !result.secretInputExcluded || !result.sourceShared) {
      throw new Error(`Android developer profile was not safe and truthful: ${JSON.stringify(result)}`);
    }
    process.stdout.write(`${JSON.stringify({ mode, ...result })}\n`);
  } else if (mode === "demo") {
    await waitFor(
      `document.querySelector('[data-ui-profile="OXID_UI_PROFILE_DEMO"]')
        ?.innerText.includes("STANDALONE DEMO")`,
      "persistent demo-profile banner before onboarding",
    );
    await clickButtonByLabel("Open standalone demo setup");
    await waitFor(
      `Boolean(document.querySelector('#demo-bootstrap-drawer'))`,
      "demo bootstrap drawer",
    );
    const modal = await evaluate(`(() => {
      const drawer = document.querySelector('#demo-bootstrap-drawer');
      const background = Array.from(document.body.children)
        .find((element) => element.getAttribute('aria-hidden') === 'true');
      return {
        dialog: drawer?.getAttribute('role') === 'dialog',
        modal: drawer?.getAttribute('aria-modal') === 'true',
        close: Boolean(drawer?.querySelector('button[aria-label="Close standalone demo setup"]')),
        backgroundHidden: Boolean(background),
      };
    })()`);
    if (!modal.dialog || !modal.modal || !modal.close || !modal.backgroundHidden) {
      throw new Error(`Android demo drawer accessibility contract failed: ${JSON.stringify(modal)}`);
    }
    await clickButton("Run full demo setup");
    await waitFor(
      `document.body.innerText.includes("Accept a credential offer")
        && Boolean(${buttonExpression("Preview credential offer")})
        && Boolean(${buttonExpression("Dismiss identity request")})`,
      "unchanged credential offer review after safe setup",
      60_000,
    );
    const review = await evaluate(`(() => ({
      consentAbsent: !document.querySelector('#credential-issuance-consent'),
      acceptanceAbsent: !${buttonExpression("Accept and issue credential")},
      banner: Boolean(document.querySelector('[data-ui-profile="OXID_UI_PROFILE_DEMO"]')),
    }))()`);
    if (!review.consentAbsent || !review.acceptanceAbsent || !review.banner) {
      throw new Error(`Android demo setup bypassed review: ${JSON.stringify(review)}`);
    }
    await clickButtonByLabel("Open standalone demo setup");
    await waitFor(
      `document.querySelector('#demo-bootstrap-drawer')?.innerText.includes("Safe setup complete")
        && document.querySelector('#demo-bootstrap-drawer')?.innerText.includes("5 NIGHT funding snapshot")`,
      "demo full-chain and per-action completion",
    );
    process.stdout.write(`${JSON.stringify({ mode, ...modal, ...review, safeSetup: true })}\n`);
  } else if (mode === "privacy-reveal") {
    await createFreshProfile();
    await waitFor(
      `document.querySelector('.app-shell')?.getAttribute('data-secret-mode') === 'masked'`,
      "default masked secret mode",
    );
    await waitFor(
      `(() => {
        const value = document.querySelector('.privacy-value');
        if (!value) return false;
        return getComputedStyle(value).color === 'rgba(0, 0, 0, 0)'
          && getComputedStyle(value, '::after').content.includes('••••');
      })()`,
      "visually masked private value",
    );
    await clickButtonByLabel("Show private values for 30 seconds");
    await waitFor(
      `document.querySelector('.app-shell')?.getAttribute('data-secret-mode') === 'revealed'
        && Boolean(document.querySelector('button[aria-label="Hide private values"]'))`,
      "explicit timed secret-mode reveal",
    );
    process.stdout.write(`${JSON.stringify({ mode, revealed: true })}\n`);
  } else if (mode === "privacy-rearmed") {
    await waitFor(
      `document.querySelector('.app-shell')?.getAttribute('data-secret-mode') === 'masked'
        && Boolean(document.querySelector('button[aria-label="Show private values for 30 seconds"]'))`,
      "background-rearmed secret mode",
    );
    process.stdout.write(`${JSON.stringify({ mode, rearmed: true })}\n`);
  } else if (mode === "backup-export") {
    await createFreshProfile();
    await openWallet();
    await clickButtonByLabel("Activate protected Midnight account");
    await waitForButton("Use my receive address", 90_000);

    await openIdentities();
    await clickButton("Create standalone DID");
    await waitFor(
      "document.body.innerText.includes('standalone-1') && document.body.innerText.includes('Manage this DID')",
      "managed DID for complete backup",
      30_000,
    );

    await openDocuments();
    await clickButton("Use standalone demo offer");
    await clickButton("Preview credential offer");
    await waitFor(
      "document.body.innerText.includes('Credential offer preview') && document.body.innerText.includes('Digital Passport')",
      "Digital Passport offer preview for complete backup",
    );
    await clickCheckboxById("credential-issuance-consent");
    await clickButton("Accept and issue credential");
    await waitFor(
      "document.body.innerText.includes('Credential issued, verified, and stored in the protected inventory.')",
      "issued Digital Passport for complete backup",
      30_000,
    );

    await openSettings();
    await waitFor(
      "document.body.innerText.includes('One encrypted wallet document')",
      "complete wallet backup settings",
    );
    await setInputById("wallet-backup-secret", backupRecoverySecret);
    await setInputById("wallet-backup-secret-confirmation", backupRecoverySecret);
    await clickConfirmation(
      "I confirm this complete wallet export and will store its recovery secret separately.",
    );
    await clickButton("Choose file and export");
    await waitFor(
      "document.body.innerText.includes('Backup complete') || Boolean(document.querySelector('[role=\"alert\"]'))",
      "complete wallet document export",
      180_000,
    );
    const exportError = await evaluate(
      "document.querySelector('[role=\"alert\"]')?.textContent.trim() ?? ''",
    );
    if (exportError) {
      throw new Error(`Android complete wallet export failed: ${exportError}`);
    }
    await waitFor(
      "document.body.innerText.includes('Backed up')",
      "persisted complete backup receipt",
    );
    process.stdout.write(`${JSON.stringify({ mode, exported: true })}\n`);
  } else if (mode === "backup-recover") {
    await clickButton("Restore from backup");
    await setInputById("onboarding-recovery-secret", backupRecoverySecret);
    await clickConfirmation("I confirm complete recovery into this empty Oxid installation.");
    await clickButtonByLabel("Choose complete wallet backup and recover");
    await waitFor(
      "document.body.innerText.includes('My wallet · Standalone') || Boolean(document.querySelector('[role=\"alert\"]'))",
      "fresh-install complete wallet recovery",
      180_000,
    );
    const recoveryError = await evaluate(
      "document.querySelector('[role=\"alert\"]')?.textContent.trim() ?? ''",
    );
    if (recoveryError) {
      throw new Error(`Android complete wallet recovery failed: ${recoveryError}`);
    }
    await openWallet();
    await waitFor(
      'Boolean(document.querySelector(\'button[aria-label="Copy Unshielded receive address"]\')) && Boolean(document.querySelector(\'button[aria-label="Copy Shielded receive address"]\'))',
      "restored Midnight receive addresses",
      90_000,
    );
    await openIdentities();
    await waitFor(
      "document.body.innerText.includes('standalone-1') && document.body.innerText.includes('Manage this DID')",
      "restored managed DID",
      30_000,
    );
    await openDocuments();
    await waitFor(
      `document.body.innerText.includes('Digital Passport') && Boolean(${buttonExpression("Reverify")})`,
      "restored Digital Passport",
      30_000,
    );
    process.stdout.write(`${JSON.stringify({
      mode,
      profileRestored: true,
      accountRestored: true,
      didRestored: true,
      credentialRestored: true,
    })}\n`);
  } else if (mode === "prepare-live-account-touch") {
    await createFreshProfile();
    await openWallet();
    await waitFor(
      `Boolean(document.querySelector('button[aria-label="Activate protected Midnight account"]'))`,
      "protected account activation control",
    );
    const geometry = await evaluate(`(() => {
      const activation = document.querySelector('button[aria-label="Activate protected Midnight account"]');
      activation.scrollIntoView({ block: 'center', inline: 'nearest' });
      const scan = document.querySelector('button[aria-label="Scan identity QR code"]');
      const nav = document.querySelector('.bottom-nav');
      const rectangle = (element) => {
        const bounds = element.getBoundingClientRect();
        return { left: bounds.left, top: bounds.top, right: bounds.right, bottom: bounds.bottom };
      };
      const activationBounds = rectangle(activation);
      const navBounds = rectangle(nav);
      const centerX = (activationBounds.left + activationBounds.right) / 2;
      const centerY = (activationBounds.top + activationBounds.bottom) / 2;
      const hit = document.elementFromPoint(centerX, centerY);
      return {
        activation: activationBounds,
        nav: navBounds,
        scan: rectangle(scan),
        clearOfNav: activationBounds.bottom <= navBounds.top,
        activationOwnsCenter: hit === activation || activation.contains(hit),
      };
    })()`);
    if (!geometry.clearOfNav || !geometry.activationOwnsCenter) {
      throw new Error(`Android activation touch geometry overlaps navigation: ${JSON.stringify(geometry)}`);
    }
    process.stdout.write(`${JSON.stringify({ mode, ...geometry })}\n`);
  } else if (mode === "live-account-after-touch") {
    await waitForButton("Use my receive address", 90_000);
    const result = await evaluate(`(() => ({
      live: document.querySelector('.status-pill')?.textContent.trim() === 'Live',
      synchronized: document.querySelector('.trust-line')?.innerText.includes('Synced · Live source'),
      scannerNoticeAbsent: !document.body.innerText.includes('QR scanning was cancelled.'),
    }))()`);
    if (!result.live || !result.synchronized || !result.scannerNoticeAbsent) {
      throw new Error(`Android physical activation tap did not reach the wallet: ${JSON.stringify(result)}`);
    }
    process.stdout.write(`${JSON.stringify({ mode, ...result })}\n`);
  } else if (mode === "live-account-restarted") {
    await openWallet();
    await waitFor(
      `Boolean(document.querySelector('button[aria-label="Activate protected Midnight account"]'))`,
      "honest process-local account reactivation",
    );
    const result = await evaluate(`(() => ({
      notConnected: document.querySelector('.status-pill')?.textContent.trim() === 'Not connected',
      accountAddressWithheld: !document.querySelector('button[aria-label="Copy Unshielded receive address"]'),
      failedClosed: !document.body.innerText.includes('Account state could not be loaded safely.'),
    }))()`);
    if (!result.notConnected || !result.accountAddressWithheld || !result.failedClosed) {
      throw new Error(`Android restarted development custody was not truthful: ${JSON.stringify(result)}`);
    }
    process.stdout.write(`${JSON.stringify({ mode, ...result })}\n`);
  } else if (mode === "live-account") {
    await createFreshProfile();
    await openWallet();
    await clickButtonByLabel("Activate protected Midnight account");
    await waitForButton("Use my receive address", 90_000);
    const result = await evaluate(`(() => ({
      live: document.querySelector('.status-pill')?.textContent.trim() === 'Live',
      synchronized: document.querySelector('.trust-line')?.innerText.includes('Synced · Live source'),
      unshielded: Boolean(document.querySelector('button[aria-label="Copy Unshielded receive address"]')),
      shielded: Boolean(document.querySelector('button[aria-label="Copy Shielded receive address"]')),
      failedClosed: !document.body.innerText.includes('Account state could not be loaded safely.'),
    }))()`);
    if (!result.live || !result.synchronized || !result.unshielded || !result.shielded || !result.failedClosed) {
      throw new Error(`Android live account did not synchronize safely: ${JSON.stringify(result)}`);
    }
    process.stdout.write(`${JSON.stringify({ mode, ...result })}\n`);
  } else if (mode === "flow") {
    await createFreshProfile();
    await assertHomeComposition();
    await clickButton("Present");
    await waitForButton("Manage identities");
    await clickButton("Home");
    await waitFor(
      "document.body.innerText.includes('Everything in one place')",
      "Home route after presentation shortcut",
    );
    await clickButton("Receive");
    await waitFor(
      "document.body.innerText.includes('Receive NIGHT') && Boolean(document.querySelector('[role=dialog]'))",
      "one-tap Home Receive sheet",
    );
    await waitForButton("Open Wallet to activate");
    await clickButton("Open Wallet to activate");
    await clickButtonByLabel("Activate protected Midnight account");
    await waitForButton("Use my receive address", 90_000);
    await waitForButton("Scan");

    await clickButton("Sync now");
    await waitFor(
      "document.body.innerText.includes('12 DUST') && document.body.innerText.includes('1 shielded notes') && document.body.innerText.includes('5 NIGHT')",
      "exact simulated account, DUST, and shielded synchronization",
    );
    await waitForButton("Sync now");

    await clickButton("Home");
    await waitFor(
      `document.querySelector('.app-header__title strong')?.textContent === 'Home'
        && document.body.innerText.includes('Everything in one place')
        && Boolean(${buttonExpression("Receive")})`,
      "populated Home route before Receive",
    );
    await clickButton("Receive");
    await waitFor(
      `Boolean(document.querySelector('button[aria-label="Use Public receive address"]'))
        && Boolean(document.querySelector('button[aria-label="Use Private receive address"]'))
        && Boolean(document.querySelector('.receive-sheet .address-qr__frame svg'))`,
      "public and private receive selectors with rendered QR",
    );
    const qrRendered = await evaluate(
      "Boolean(document.querySelector('.receive-sheet .address-qr__frame svg'))",
    );
    await clickButtonByLabel("Use Private receive address");
    await waitFor(
      'Boolean(document.querySelector(\'.receive-sheet [role="img"][aria-label="QR code for Shielded receive address"]\'))',
      "shielded receive QR",
    );
    const shieldedAddressRendered = await evaluate(
      'Boolean(document.querySelector(\'.receive-sheet [role="img"][aria-label="QR code for Shielded receive address"]\'))',
    );
    await clickButtonByLabel("Use Public receive address");

    await clickButtonByLabel("Copy Unshielded receive address");
    await waitFor(
      "document.body.innerText.includes('Public receive address copied to the native clipboard.')",
      "native public-address clipboard confirmation",
    );
    const publicAddressCopied = await evaluate(
      "document.body.innerText.includes('Public receive address copied to the native clipboard.')",
    );

    await clickButtonByLabel("Close Receive");
    await openWallet();
    await clickButton("Use my receive address");
    await clickButtonByLabel("Continue to transfer amount");
    await setInput("Amount in NIGHT", "1.5");
    await clickButton("Review exact transfer");
    await clickButtonByLabel("Continue to NIGHT transfer confirmation");
    await clickButtonByLabel("Authorize reviewed NIGHT transfer");
    await clickButtonByLabel("Prove and submit NIGHT transfer");
    await clickButtonByLabel("Cancel NIGHT transfer submission");
    await waitForButton("Retry safely — nothing was broadcast");
    await clickButton("Retry safely — nothing was broadcast");
    await clickButtonByLabel("Prove and submit NIGHT transfer");
    await waitFor(
      "document.body.innerText.includes('Transfer confirmed')",
      "simulated transfer inclusion",
    );

    const walletResult = await evaluate(`(() => ({
      submitted: document.body.innerText.includes("Transfer confirmed"),
      simulated: document.body.innerText.includes("Mode: Simulated — runs locally, nothing on Midnight"),
      dustSynced: document.body.innerText.includes("12 DUST"),
      shieldedSynced: document.body.innerText.includes("1 shielded notes")
        && document.body.innerText.includes("5 NIGHT"),
    }))()`);
    await openIdentities();
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
      "document.body.innerText.includes('DID authentication preview') && document.body.innerText.includes('Who is asking?') && document.body.innerText.includes('What will you prove?') && document.body.innerText.includes('Which identity?') && document.body.innerText.includes('Why is it requested?') && document.body.innerText.includes('Unverified endpoint') && document.body.innerText.includes('No credential or document claims will be disclosed.')",
      "four-question SIOPv2 DID authentication preview",
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
    await openDocuments();
    await waitForButton("Use standalone demo offer");
    await clickButton("Use standalone demo offer");
    await clickButton("Preview credential offer");
    await waitFor(
      "document.body.innerText.includes('Credential offer preview') && document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('Who is issuing it?') && document.body.innerText.includes('What will you receive?') && document.body.innerText.includes('Which identity receives it?') && document.body.innerText.includes('Why add it?') && document.body.innerText.includes('Unverified endpoint')",
      "four-question OID4VCI credential offer preview",
    );
    await evaluate(`(() => {
      const consent = document.querySelector('#credential-issuance-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Accept and issue credential");
    await waitFor(
      "document.body.innerText.includes('Credential issued, verified, and stored in the protected inventory.') && document.body.innerText.includes('Valid')",
      "issued and verified OID4VCI credential",
    );
    // The holder-bound standalone credential ID commits to its issuance
    // second. Cross that boundary before issuing a second matching passport
    // so the chooser is exercised with two distinct stored credentials.
    await evaluate("new Promise((resolve) => setTimeout(resolve, 1200))");
    await clickButton("Use standalone demo offer");
    await clickButton("Preview credential offer");
    await waitFor(
      "document.body.innerText.includes('Credential offer preview')",
      "second OID4VCI credential offer preview",
    );
    await evaluate(`(() => {
      const consent = document.querySelector('#credential-issuance-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Accept and issue credential");
    await waitFor(
      "document.querySelectorAll('.credential-record').length === 2",
      "second distinct Digital Passport",
    );
    await clickButton("Use standalone verifier request");
    await clickButton("Preview presentation request");
    await waitFor(
      "document.body.innerText.includes('Presentation preview') && document.body.innerText.includes('Who is asking?') && document.body.innerText.includes('What will be shared?') && document.body.innerText.includes('Which document?') && document.body.innerText.includes('Why is it requested?') && document.body.innerText.includes('Unverified endpoint') && document.body.innerText.includes(\"Confirms you're over 18. Your date of birth will not be shared.\") && document.body.innerText.includes('No presentation or vp_token has been generated.')",
      "four-question OpenID4VP presentation preview",
    );
    const credentialChooserRequired = await evaluate(`(() => {
      const choices = Array.from(document.querySelectorAll(
        '.presentation-credential-option input[type="radio"]'
      ));
      const consent = document.querySelector('#credential-presentation-consent');
      return choices.length === 2 && Boolean(consent && consent.disabled);
    })()`);
    if (!credentialChooserRequired) {
      throw new Error("presentation consent did not require an exact credential selection");
    }
    await evaluate(`(() => {
      const choices = Array.from(document.querySelectorAll(
        '.presentation-credential-option input[type="radio"]'
      ));
      if (choices.length !== 2) return false;
      choices[1].click();
      return true;
    })()`);
    await waitFor(
      `(() => {
        const choices = Array.from(document.querySelectorAll(
          '.presentation-credential-option input[type="radio"]'
        ));
        const consent = document.querySelector('#credential-presentation-consent');
        return choices.length === 2 && choices[1].checked && Boolean(consent && !consent.disabled);
      })()`,
      "explicit second credential selection",
    );
    const credentialChooserValidated = true;
    if (!credentialChooserValidated) {
      throw new Error("presentation credential chooser did not require an exact selection");
    }
    await evaluate(`(() => {
      const consent = document.querySelector('#credential-presentation-consent');
      if (!consent) return false;
      consent.click();
      return consent.checked;
    })()`);
    await clickButton("Share proof");
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
      "document.body.innerText.includes('Disclosure preview ready · local preview only · no presentation generated')",
      "claim-free local disclosure preview",
    );
    const disclosurePreviewed = await evaluate(
      "document.body.innerText.includes('Disclosure preview ready · local preview only · no presentation generated')",
    );
    await clickButton("Reverify");
    await waitForButton("Reverify");
    const credentialVerified = await evaluate(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('Valid')",
    );
    const credentialPolicyChecked = await evaluate(
      "document.body.innerText.includes('Credential policy · issuer passed · time passed · trust passed · revocation not checked')",
    );

    await openPassportVault();
    await waitFor(
      "document.body.innerText.includes('Owner-private saved conformance ledger') && document.body.innerText.includes('survives app restart')",
      "durable standalone Passport Vault state label",
    );
    const vaultStatePersistent = await evaluate(
      "document.body.innerText.includes('Owner-private saved conformance ledger') && document.body.innerText.includes('no on-chain transaction submitted')",
    );
    await waitFor(
      "document.body.innerText.includes('Simulated — runs locally, nothing on Midnight') && document.body.innerText.includes('no node broadcast occurs')",
      "truthfully labelled deterministic vault-call mode",
    );
    await clickButton("Read contract state");
    await waitFor(
      "document.body.innerText.includes('Contract state') && document.body.innerText.includes('Simulated — runs locally, nothing on Midnight')",
      "simulated Passport Vault contract state",
    );
    await clickButton("Review contract call");
    await clickButton("Authorize exact call");
    await clickButton("Prove and submit");
    await waitFor(
      "document.body.innerText.includes('Passport Vault call completed') && document.body.innerText.includes('Mode: Simulated — runs locally, nothing on Midnight')",
      "simulated native Passport Vault call lifecycle",
    );
    const nativeVaultCallFlow = await evaluate(
      "document.body.innerText.includes('Passport Vault call completed') && document.body.innerText.includes('Mode: Simulated — runs locally, nothing on Midnight') && document.body.innerText.includes('Transaction') && document.body.innerText.includes('Block')",
    );
    await waitForButton("Create confirmed lock");
    await setInput("Vault required issuing state", "US");
    await setInput("Vault required document number", "AB1234567");
    await clickButton("Create confirmed lock");
    await waitFor(
      "document.body.innerText.includes('100 NIGHT remaining') && document.body.innerText.includes('state US') && document.body.innerText.includes('document AB1234567')",
      "created policy-bound Passport Vault lock",
    );
    await clickButton("Deposit");
    await waitFor(
      "document.body.innerText.includes('110 NIGHT remaining')",
      "Passport Vault deposit",
    );
    await clickButton("Claim with credential");
    await waitFor(
      "document.body.innerText.includes('100 NIGHT remaining') && document.body.innerText.includes('Claims 1')",
      "credential-gated Passport Vault claim",
    );
    await clickButton("Withdraw");
    await waitFor(
      "document.body.innerText.includes('90 NIGHT remaining')",
      "Passport Vault creator withdrawal",
    );
    const vaultFlow = await evaluate(
      "document.body.innerText.includes('Passport Vault') && document.body.innerText.includes('90 NIGHT remaining') && document.body.innerText.includes('Claims 1')",
    );

    await openIdentities();
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
    await openDocuments();
    await waitFor(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('Valid') && document.body.innerText.includes('Proof')",
      "verified issued credential",
    );
    const result = { ...walletResult, homeComposed: true, claimsHiddenByDefault, credentialChooserValidated, credentialPolicyChecked, credentialVerified, didAuthenticated, didManaged, didResolved, disclosurePreviewed, nativeVaultCallFlow, presentationProofGated, publicAddressCopied, qrRendered, shieldedAddressRendered, thresholdAvailable, vaultFlow, vaultStatePersistent };
    if (!result.submitted || !result.simulated || !result.dustSynced || !result.shieldedSynced || !result.homeComposed || !result.claimsHiddenByDefault || !result.credentialChooserValidated || !result.credentialPolicyChecked || !result.credentialVerified || !result.didAuthenticated || !result.didManaged || !result.didResolved || !result.disclosurePreviewed || !result.nativeVaultCallFlow || !result.presentationProofGated || !result.publicAddressCopied || !result.qrRendered || !result.shieldedAddressRendered || !result.thresholdAvailable || !result.vaultFlow || !result.vaultStatePersistent) {
      throw new Error(`Android standalone wallet flow did not expose the expected public result: ${JSON.stringify(result)}`);
    }
    await clickButton("Home");
    await waitFor(
      `document.querySelector('.app-header__title strong')?.textContent === 'Home'
        && document.body.innerText.includes('Everything in one place')
        && Boolean(${buttonExpression("Receive")})`,
      "populated Home route before native share",
    );
    await clickButton("Receive");
    await waitForButton("Share");
    await clickButtonByLabel("Share Unshielded receive address");
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } else if (mode === "restored") {
    await waitForButton("Wallet");
    await assertHomeComposition();
    await openWallet();
    await waitForButton("Activate development wallet");
    const walletRestored = await evaluate(`(() => ({
      profileRestored: !document.body.innerText.includes("Create your wallet profile"),
      developmentRootReset: document.body.innerText.includes("Activate protected test account"),
      submissionRestored: document.body.innerText.includes("Transfer included"),
    }))()`);
    await openIdentities();
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
    await openDocuments();
    await waitFor(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('Valid') && Boolean(document.querySelector('.passport-claims')) && !document.body.innerText.includes('Alice') && !document.body.innerText.includes('Example')",
      "restored credential inventory",
    );
    await waitForButton("Reverify");
    await clickButton("Preview disclosure plan");
    await waitFor(
      "document.body.innerText.includes('Disclosure preview ready · local preview only · no presentation generated')",
      "restored protected disclosure preview",
    );
    const credentialRestored = await evaluate(
      "document.body.innerText.includes('Digital Passport') && document.body.innerText.includes('Valid') && document.body.innerText.includes('Credential policy · issuer passed · time passed · trust passed · revocation not checked') && !document.body.innerText.includes('Alice') && !document.body.innerText.includes('Example')",
    );
    await openPassportVault();
    await waitFor(
      "document.body.innerText.includes('90 NIGHT remaining') && document.body.innerText.includes('Claims 1') && document.body.innerText.includes('Owner-private saved conformance ledger')",
      "restored standalone Passport Vault accounting",
    );
    const vaultRestored = await evaluate(
      "document.body.innerText.includes('90 NIGHT remaining') && document.body.innerText.includes('Claims 1') && document.body.innerText.includes('survives app restart')",
    );
    const restored = { ...walletRestored, credentialRestored, didRestored, managedDidRestored, vaultRestored };
    if (!restored.profileRestored || !restored.developmentRootReset || !restored.submissionRestored || !restored.credentialRestored || !restored.didRestored || !restored.managedDidRestored || !restored.vaultRestored) {
      throw new Error(`Android restart did not restore the expected public and owner-private state: ${JSON.stringify(restored)}`);
    }
    process.stdout.write(`${JSON.stringify(restored)}\n`);
  } else if (mode === "native-authorize") {
    const createProfile = await evaluate(`Boolean(${buttonExpression("Create new wallet")})`);
    if (createProfile) await createFreshProfile();
    await openWallet();
    await waitFor(
      'Boolean(document.querySelector(\'button[aria-label="Activate protected Midnight account"]\')) || Boolean(document.querySelector(\'.address-row\')) || Boolean(document.querySelector(\'[role="alert"]\'))',
      "settled pre-authorization account state",
      90_000,
    );
    const accountLoadFailed = await evaluate(
      'Boolean(document.querySelector(\'[role="alert"]\'))',
    );
    if (accountLoadFailed) {
      throw new Error("native custody account status failed safely before authorization");
    }
    await openSettings();
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
      await waitFor(
        'Boolean(document.querySelector(\'button[aria-label="Lock wallet"]\')) || Boolean(document.querySelector(\'[role="alert"]\'))',
        "completed native custody authorization or a safe failure",
        90_000,
      );
      const authorizationFailed = await evaluate(
        'Boolean(document.querySelector(\'[role="alert"]\'))',
      );
      if (authorizationFailed) {
        throw new Error("native custody authorization failed safely");
      }
    }
    process.stdout.write(`${JSON.stringify({ mode, securityAction })}\n`);
  } else if (mode === "native-custody" || mode === "native-restored") {
    await openWallet();
    await waitFor(
      "document.body.textContent.includes('Wallet overview')",
      "Assets page before custody refresh",
    );
    await openSettings();
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
    await openWallet();
    await waitFor(
      "document.body.textContent.includes('Wallet overview')",
      "Assets page before native account activation",
    );
    try {
      await waitFor(
        'Boolean(document.querySelector(\'button[aria-label="Activate protected Midnight account"]\')) || Boolean(document.querySelector(\'.address-row\')) || Boolean(document.querySelector(\'[role="alert"]\'))',
        "settled native account state",
        90_000,
      );
    } catch (_error) {
      const boundedState = await evaluate(`(() => ({
        loading: document.body.textContent.includes('Loading the selected Midnight account boundary'),
        accountUnavailable: document.body.textContent.includes('Midnight account unavailable'),
        activationAvailable: Boolean(document.querySelector('button[aria-label="Activate protected Midnight account"]')),
        receiveAvailable: Boolean(document.querySelector('.address-row')),
        alertAvailable: Boolean(document.querySelector('[role="alert"]')),
      }))()`);
      throw new Error(`native account state did not settle: ${JSON.stringify(boundedState)}`);
    }
    const needsActivation = await evaluate(
      'Boolean(document.querySelector(\'button[aria-label="Activate protected Midnight account"]\'))',
    );
    if (needsActivation) {
      await clickButtonByLabel("Activate protected Midnight account");
    }
    const accountReadyExpression = needsActivation
      ? '!Boolean(document.querySelector(\'button[aria-label="Activate protected Midnight account"]\')) && Boolean(document.querySelector(\'.address-row\'))'
      : 'Boolean(document.querySelector(\'.address-row\'))';
    await waitFor(
      `(${accountReadyExpression}) || Boolean(document.querySelector('[role="alert"]'))`,
      needsActivation ? "native account derivation or a safe failure" : "restored native account or a safe failure",
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
    await openSettings();
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
