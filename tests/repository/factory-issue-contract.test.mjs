// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import { validateFactoryIssueContract } from "../../scripts/lib/factory-issue-contract.mjs";

const complete = `## Implementation surface

- scripts/lib/example.mjs

## AC / DoD matrix

| Acceptance criterion | Completion evidence |
| --- | --- |
| AC-1: one admission path is guarded | focused contract fixture proves rejection |
| AC-2: one retry path is guarded | retry fixture proves no implementation redispatch |

## Verification

- node --test tests/repository/factory-issue-contract.test.mjs

## Size

M

## Delivery target

develop

## Non-goals

- remote mutation
`;

test("factory issue admission requires an explicit stable implementation contract", () => {
  const result = validateFactoryIssueContract({ title: "fix(harness): admit only complete work", body: complete });
  assert.equal(result.ok, true);
  assert.deepEqual(result.acceptanceIds, ["AC-1", "AC-2"]);
});

test("incomplete issue #858-shaped contracts fail before execution admission", () => {
  const result = validateFactoryIssueContract({
    title: "fix(harness): incomplete contract",
    body: complete.replace("## Implementation surface\n\n- scripts/lib/example.mjs\n\n", "").replace("retry fixture proves no implementation redispatch", "D2"),
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /missing Implementation surface/u);
  assert.match(result.errors.join("\n"), /lacks concrete completion evidence for AC-2/u);
});

test("identifier-only issue matrices fail before factory admission", () => {
  const result = validateFactoryIssueContract({
    title: "fix(harness): reject tautological matrices",
    body: complete.replace("AC-1: one admission path is guarded", "AC-1").replace("focused contract fixture proves rejection", "D1"),
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /concrete outcome/u);
});
