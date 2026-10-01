// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const modulePath = "crates/identity/application/src/approval.rs";
const fixturePath = "crates/identity/application/src/approval/tests.rs";
const unavailableImplementation = `impl TrustedDidApprovalPort for UnavailableApproval {
    fn approve(&self, _: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError> {
        Err(TrustedDidApprovalError::Unavailable)
    }
}`;

function violations(files) {
  const failures = [];
  for (const [path, source] of files) {
    if (path === fixturePath) continue;
    if (path !== modulePath) {
      if (/\b(?:TrustedDidApprovalPort|with_trusted_did_port)\b/u.test(source)) failures.push(path);
      continue;
    }
    const implementations = [...source.matchAll(/impl\s+TrustedDidApprovalPort\s+for\s+(\w+)/gu)];
    if (implementations.length !== 1 || implementations[0][1] !== "UnavailableApproval"
      || !source.includes(unavailableImplementation)
      || !/#\[cfg\(test\)\]\s*mod tests;/u.test(source)) failures.push(path);
  }
  return failures;
}

test("DID approval composition has no incoming injection or production approving port", () => {
  const paths = execFileSync("git", ["ls-files", "-z", "--", "*.rs"], { encoding: "utf8" }).split("\0").filter(Boolean);
  assert(paths.includes(modulePath));
  assert.deepEqual(violations(paths.map(path => [path, readFileSync(path, "utf8")])), []);
});

test("DID guard rejects incoming injection, alias imports, and production auto approval", () => {
  for (const source of [
    "DidApprovalService::with_trusted_did_port(clock, port)",
    "use oxid_identity_application::TrustedDidApprovalPort as Port;",
    "impl TrustedDidApprovalPort for AutoApprove {}",
  ]) assert.deepEqual(violations([["crates/adapters/untrusted/src/lib.rs", source]]), ["crates/adapters/untrusted/src/lib.rs"]);
  const source = readFileSync(modulePath, "utf8");
  assert.deepEqual(violations([[modulePath, source.replace("Err(TrustedDidApprovalError::Unavailable)", "Ok(())")]]), [modulePath]);
  assert.deepEqual(violations([[modulePath, source + "\nimpl TrustedDidApprovalPort for AutoApprove {}"]]), [modulePath]);
  assert.deepEqual(violations([[modulePath, source.replace("#[cfg(test)]\nmod tests;", "mod tests;")]]), [modulePath]);
});
