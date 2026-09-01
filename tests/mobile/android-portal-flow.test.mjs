// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const source = fs.readFileSync(path.join(directory, "android-portal-flow.mjs"), "utf8");
const supportSource = fs.readFileSync(
  path.resolve(directory, "../../scripts/e2e/portal-android-support.mjs"),
  "utf8",
);

function issueErrorTerminalWait() {
  const start = source.indexOf('} else if (mode === "issue-error")');
  const acceptance = source.indexOf('await click("Accept and issue credential");', start);
  const restoreProxy = source.indexOf('await setProxyMode("normal");', acceptance);
  assert.ok(start >= 0 && acceptance >= start && restoreProxy > acceptance);
  return source.slice(acceptance, restoreProxy);
}

function unavailableProxyBranch() {
  const start = supportSource.indexOf('if (counter !== "kyc" && proxyMode === "unavailable")');
  const nextBranch = supportSource.indexOf('if (counter === "issuerMetadata" && proxyMode === "malformed")', start);
  assert.ok(start >= 0 && nextBranch > start);
  return supportSource.slice(start, nextBranch);
}

test("issue-error waits for the post-failure locked-review state before restoring the proxy", () => {
  const terminal = issueErrorTerminalWait();

  assert.match(terminal, /const consent = document\.querySelector\("#credential-issuance-consent"\);/u);
  assert.match(terminal, /Boolean\(consent\) && !consent\.checked/u);
  assert.match(terminal, /const issue = \$\{button\("Accept and issue credential"\)\};/u);
  assert.match(terminal, /Boolean\(issue && issue\.disabled\)/u);
  assert.match(terminal, /const leave = \$\{button\("Leave credential review"\)\};/u);
});

test("unavailable issuer proxy uses one deterministic HTTP failure response", () => {
  const branch = unavailableProxyBranch();

  assert.match(branch, /sendJson\(response, 503, \{ error: "unavailable" \}\);/u);
  assert.doesNotMatch(branch, /response\.destroy\(\);/u);
});

test("issue accepts either the completion notice or the protected valid-record state", () => {
  const issueStart = source.indexOf('} else if (mode === "issue")');
  const result = source.indexOf('    const result = await evaluate', issueStart);
  const terminal = source.slice(issueStart, result);

  assert.ok(issueStart >= 0 && result > issueStart);
  assert.match(source, /function issuanceCompletionExpression\(\)/u);
  assert.match(terminal, /await waitFor\(\s*issuanceCompletionExpression\(\),\s*"Portal issuance",\s*90_000\s*\)/u);
  assert.match(source, /credential-record/u);
  assert.match(source, /textContent\.trim\(\) === "Valid"/u);
  assert.match(source, /function issuanceEvidenceExpression\(\)/u);
  assert.match(terminal, /await waitFor\(issuanceEvidenceExpression\(\), "protected credential inventory", 30_000\);/u);
  assert.match(source, /offerReviewClosed/u);
  assert.match(source, /standaloneInboxAbsent/u);
  assert.match(source, /attributesListed/u);
  assert.match(source, /"Saved to your wallet"/u);
  assert.match(source, /"Document number"/u);
  assert.match(source, /"Issuing state"/u);
  assert.match(source, /function issuanceDiagnosticExpression\(\)/u);
  assert.match(source, /invalidCredential/u);
  assert.match(source, /invalidCredentialResponse/u);
  assert.match(source, /credentialStoreUnavailable/u);
  assert.match(source, /issuedCredentialVerificationFailed/u);
  assert.match(source, /issuedCredentialPersistenceFailed/u);
  assert.match(source, /issuedCredentialStorageUnavailable/u);
  assert.match(source, /const hasStatus = \(value\) => statuses\.some\(\(text\) => text\.includes\(value\)\);/u);
  assert.match(terminal, /const diagnosticState = await evaluate\(issuanceDiagnosticExpression\(\)\);/u);
  assert.match(terminal, /payload-free counters \$\{JSON\.stringify\(diagnosticCounts\)\} and state \$\{JSON\.stringify\(diagnosticState\)\}/u);
});

test("Android issuance consent crosses a real touch boundary", () => {
  assert.match(source, /async function touchCheckbox\(selector, description\)/u);
  assert.match(source, /Input\.dispatchTouchEvent/u);
  assert.equal(
    source.match(/await touchCheckbox\("#credential-issuance-consent", "issuance consent"\);/gu)?.length,
    2,
  );
  assert.doesNotMatch(
    source,
    /evaluate\('document\.querySelector\("#credential-issuance-consent"\)\.click\(\)'\)/u,
  );
});
