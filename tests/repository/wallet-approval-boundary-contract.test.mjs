// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const modulePath = "crates/wallet/application/src/approval.rs";
const fixturePath = "crates/wallet/application/src/approval/tests.rs";
const unavailableImplementation = `impl TrustedWalletApprovalPort for UnavailableApproval {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        Err(TrustedWalletApprovalError::Unavailable)
    }
}`;

function violations(files) {
  const failures = [];
  for (const [path, source] of files) {
    if (path === fixturePath || path === "crates/composition/tests/direct_key_approval.rs") continue;
    if (path !== modulePath) {
      if (/\b(?:TrustedWalletApprovalPort|with_trusted_port)\b/u.test(source)) failures.push(path);
      continue;
    }
    // Freeze the sole production producer until a trusted adapter is reviewed.
    const implementations = [...source.matchAll(/impl\s+TrustedWalletApprovalPort\s+for\s+(\w+)/gu)];
    if (implementations.length !== 1 || implementations[0][1] !== "UnavailableApproval"
      || !source.includes(unavailableImplementation)
      || !/#\[cfg\(test\)\]\s*mod tests;/u.test(source)) failures.push(path);
  }
  return failures;
}

test("approval composition has no incoming injection or production approving port", () => {
  const paths = execFileSync("git", ["ls-files", "-z", "--", "*.rs"], { encoding: "utf8" }).split("\0").filter(Boolean);
  assert(paths.includes(modulePath));
  assert.deepEqual(violations(paths.map(path => [path, readFileSync(path, "utf8")])), []);
});

test("guard rejects incoming injection, alias imports, and production auto approval", () => {
  for (const source of [
    "WalletApprovalService::with_trusted_port(clock, port)",
    "use oxid_wallet_application::TrustedWalletApprovalPort as Port;",
    "impl TrustedWalletApprovalPort for AutoApprove {}",
  ]) assert.deepEqual(violations([["crates/adapters/untrusted/src/lib.rs", source]]), ["crates/adapters/untrusted/src/lib.rs"]);
  const source = readFileSync(modulePath, "utf8");
  assert.deepEqual(violations([[modulePath, source.replace("Err(TrustedWalletApprovalError::Unavailable)", "Ok(())")]]), [modulePath]);
  assert.deepEqual(violations([[modulePath, source.replace(
    unavailableImplementation,
    unavailableImplementation.replace(
      "Err(TrustedWalletApprovalError::Unavailable)",
      "if auto_approve() { return Ok(()); }\n        Err(TrustedWalletApprovalError::Unavailable)",
    ),
  )]]), [modulePath]);
  assert.deepEqual(violations([[modulePath, source + "\nimpl TrustedWalletApprovalPort for AutoApprove {}"]]), [modulePath]);
  assert.deepEqual(violations([[modulePath, source.replace("#[cfg(test)]", "")]]), [modulePath]);
});
