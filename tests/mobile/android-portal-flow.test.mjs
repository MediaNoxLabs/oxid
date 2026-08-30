// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const source = fs.readFileSync(path.join(directory, "android-portal-flow.mjs"), "utf8");

function issueErrorTerminalWait() {
  const start = source.indexOf('} else if (mode === "issue-error")');
  const acceptance = source.indexOf('await click("Accept and issue credential");', start);
  const restoreProxy = source.indexOf('await setProxyMode("normal");', acceptance);
  assert.ok(start >= 0 && acceptance >= start && restoreProxy > acceptance);
  return source.slice(acceptance, restoreProxy);
}

test("issue-error waits for the post-failure locked-review state before restoring the proxy", () => {
  const terminal = issueErrorTerminalWait();

  assert.match(terminal, /const consent = document\.querySelector\("#credential-issuance-consent"\);/u);
  assert.match(terminal, /Boolean\(consent\) && !consent\.checked/u);
  assert.match(terminal, /const issue = \$\{button\("Accept and issue credential"\)\};/u);
  assert.match(terminal, /Boolean\(issue && issue\.disabled\)/u);
  assert.match(terminal, /const leave = \$\{button\("Leave credential review"\)\};/u);
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
});
