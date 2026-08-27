// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import { ChangeTier, classifyChangedPaths } from "../../scripts/ci/change-tier.mjs";

test("documentation-only changes avoid toolchain setup", () => {
  assert.equal(classifyChangedPaths(["README.md", "docs/factory/runbook.md"]), ChangeTier.DOCS);
});

test("repository harness changes run hermetic contract tests", () => {
  assert.equal(
    classifyChangedPaths([".devloops", "scripts/loop/pre-flight-gate.mjs", "docs/dev-loop-stability.md"]),
    ChangeTier.HARNESS,
  );
});

test("ordinary source changes run the Rust gate", () => {
  assert.equal(classifyChangedPaths(["crates/foundation/src/lib.rs"]), ChangeTier.RUST);
  assert.equal(classifyChangedPaths(["scripts/build-docs-site.sh"]), ChangeTier.RUST);
});

test("delivery, build, protocol, and custody surfaces fail closed to full", () => {
  for (const candidate of [
    ".github/workflows/ci.yml",
    "Cargo.lock",
    "nix/devshells/default.nix",
    "contracts/passport.compact",
    "crates/protocol/domain/src/lib.rs",
    "crates/credential/domain/src/lib.rs",
    "crates/adapters/custody-software/src/lib.rs",
    "crates/adapters/openid4vp/src/lib.rs",
  ]) {
    assert.equal(classifyChangedPaths([candidate]), ChangeTier.FULL, candidate);
  }
});

test("a mixed change uses its highest-risk path", () => {
  assert.equal(classifyChangedPaths(["docs/README.md", "crates/foundation/src/lib.rs"]), ChangeTier.RUST);
  assert.equal(classifyChangedPaths([".devloops", "flake.lock"]), ChangeTier.FULL);
  assert.equal(classifyChangedPaths([]), ChangeTier.FULL);
});
