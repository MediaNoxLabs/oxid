// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import { computeOxidSizeBudget, classifyOxidSizePath } from "../../scripts/loop/oxid-size-budget.mjs";
import { resolveOxidCompatibilityRoute } from "../../scripts/dev-loops.mjs";

const sizeConfig = { testDiscount: 0.25, absoluteHardLoc: 100, tiers: { default: { softLoc: 10, waiverLoc: 50 } } };
const result = (entries, config = sizeConfig) => computeOxidSizeBudget({ numstatOutput: entries.map(([add, del, file]) => `${add}\t${del}\t${file}\0`).join(""), sizeConfig: config });

test("Oxid native source languages receive deterministic logic LOC", () => {
  assert.equal(result([[8, 2, "crates/core/src/lib.rs"]]).wholeLogicLoc, 10);
  assert.equal(result([[4, 0, "apps/android/Main.kt"], [6, 0, "apps/ios/App.swift"]]).wholeLogicLoc, 10);
});

test("recognized embedded and path native tests retain the configured discount", () => {
  assert.equal(result([[8, 0, "crates/core/tests/login.rs"]]).wholeLogicLoc, 2);
  assert.equal(result([[8, 0, "apps/android/src/test/AuthTest.kt"]]).wholeLogicLoc, 2);
  assert.equal(result([[8, 0, "apps/ios/WalletTests.swift"]]).wholeLogicLoc, 2);
});

test("docs, config, CI, generated paths, and lockfiles are excluded", () => {
  for (const file of ["docs/guide.rs", ".github/workflows/check.swift", ".pi/settings.json", "generated/api.kt", "Cargo.lock", "Cargo.toml", "flake.lock"]) assert.equal(classifyOxidSizePath(file), "excluded");
  const outcome = result([[100, 0, "docs/guide.rs"], [100, 0, "Cargo.lock"], [100, 0, "Cargo.toml"], [100, 0, ".pi/settings.json"]]);
  assert.equal(outcome.outcome, "pass");
  assert.equal(outcome.wholeLogicLoc, 0);
});

test("majority unknown source remains fail-closed", () => {
  assert.equal(result([[20, 0, "src/unknown.go"]]).outcome, "block");
  const minorityUnknown = result([[20, 0, "src/unknown.go"], [30, 0, "crates/core/src/lib.rs"]]);
  assert.notEqual(minorityUnknown.outcome, "block");
  assert.equal(minorityUnknown.wholeLogicLoc, 30);
});

test("soft and hard thresholds remain upstream-enforced", () => {
  assert.equal(result([[12, 0, "crates/core/src/lib.rs"]]).outcome, "escalate");
  assert.equal(result([[101, 0, "crates/core/src/lib.rs"]]).outcome, "block");
});

test("only sanctioned size and ready wrapper routes use the compatibility adapter", () => {
  assert.equal(typeof resolveOxidCompatibilityRoute(["gate", "size-budget"]), "function");
  assert.equal(typeof resolveOxidCompatibilityRoute(["pr", "ready-for-review"]), "function");
  assert.equal(resolveOxidCompatibilityRoute(["loop", "startup"]), null);
  assert.equal(resolveOxidCompatibilityRoute(["pr", "create"]), null);
});
