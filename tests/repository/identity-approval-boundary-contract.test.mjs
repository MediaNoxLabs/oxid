// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const modulePath = "crates/identity/application/src/approval.rs";
const fixturePath = "crates/identity/application/src/approval/development.rs";
const unitTests = "crates/identity/application/src/approval/tests.rs";
const compositionPath = "crates/composition/src/profile_in_memory.rs";
const persistentCompositionPath = "crates/composition/src/profile_headless.rs";
const environmentCompositionPath = "crates/composition/src/profile_environment.rs";
const portalCompositionPath = "crates/composition/src/profile_mobile.rs";
const fixtureExecutablePath = "apps/oxid-headless/tests/support/development_did_approval_fixture_main.rs";
const headlessManifestPath = "apps/oxid-headless/Cargo.toml";
const demoApplicationPath = "apps/oxid/src/main.rs";
const demoManifestPath = "apps/oxid/Cargo.toml";
const fixtureGate = '#[cfg(any(test, feature = "development-approval"))]';
const compositionGate = '#[cfg(any(test, feature = "development-did-approval"))]';
const environmentFixtureGate = `#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "headless-portal-local",
    feature = "development-did-approval",
    not(any(target_os = "ios", target_os = "android"))
))]`;
const portalFixtureGate = `#[cfg(all(
    feature = "development-did-approval",
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]`;
const testConsumers = new Set([
  "crates/adapters/openid4vci/src/lib.rs",
  "crates/adapters/siopv2/src/lib.rs",
]);
// The method name is shared with wallet services; retain only those existing
// wallet sites and the direct identity consumer/composition sites.
const injectionPaths = new Set([
  "crates/identity/application/src/lib.rs",
  "crates/identity/application/src/lifecycle.rs",
  "crates/identity/application/src/lifecycle/tests/approval_enforcement.rs",
  "crates/composition/src/wiring.rs",
  "crates/composition/tests/direct_key_approval.rs",
  "crates/wallet/application/src/lib.rs",
  "crates/wallet/application/src/security.rs",
  "crates/wallet/application/src/dust_registration.rs",
  "crates/wallet/application/src/transaction.rs",
  "crates/wallet/application/src/approval/movement_tests.rs",
]);
const unavailableImplementation = `impl TrustedDidApprovalPort for UnavailableApproval {
    fn approve(&self, _: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError> {
        Err(TrustedDidApprovalError::Unavailable)
    }
}`;

function violations(files) {
  const failures = [];
  for (const [path, original] of files) {
    if (path === unitTests) continue;
    let source = original;
    // Only the test module, not an entire protocol adapter, can select a fixture.
    if (testConsumers.has(path)) source = source.split("#[cfg(test)]\nmod tests {")[0];
    if (/(?:\.|::)\s*with_approvals\b/u.test(source) && !injectionPaths.has(path)) {
      failures.push(path); continue;
    }
    if (path === modulePath) {
      const implementations = [...source.matchAll(/impl\s+TrustedDidApprovalPort\s+for\s+(\w+)/gu)];
      if (implementations.length !== 1 || implementations[0][1] !== "UnavailableApproval"
        || !source.includes(unavailableImplementation)
        || !/#\[cfg\(test\)\]\s*pub\(crate\) mod tests;/u.test(source)
        || !source.includes(`${fixtureGate}\nmod development;`)
        || !source.includes(`${fixtureGate}\npub use development::development_did_approvals;`)) failures.push(path);
      continue;
    }
    if (path === fixturePath) {
      if (!source.includes("impl TrustedDidApprovalPort for DevelopmentApproval")
        || [...source.matchAll(/impl\s+TrustedDidApprovalPort\s+for/gu)].length !== 1
        || /(?:std::env|serde|Deserialize)/u.test(source)) failures.push(path);
      continue;
    }
    if (/\b(?:TrustedDidApprovalPort|with_trusted_did_port)\b/u.test(source)) { failures.push(path); continue; }
    if (path === compositionPath) {
      if (!source.includes(`${compositionGate}\n#[must_use]\npub fn compose_in_memory_with_development_did_approval()`)
        || /(?:std::env|Deserialize)/u.test(source)
        || source.split("development_did_approvals(").length !== 2) failures.push(path);
      continue;
    }
    if (path === persistentCompositionPath) {
      if (!source.includes('#[cfg(feature = "development-did-approval")]\n#[must_use]\npub fn compose_headless_with_development_did_approval()')
        || /(?:std::env|Deserialize)/u.test(source)
        || source.split("development_did_approvals(").length !== 2) failures.push(path);
      continue;
    }
    if (path === environmentCompositionPath) {
      if (!source.includes(`${environmentFixtureGate}\npub fn compose_native_headless_process_with_development_did_approval_from_environment(`)
        || /(?:std::env|Deserialize)/u.test(source)) failures.push(path);
      continue;
    }
    if (path === portalCompositionPath) {
      if (!source.includes(`${portalFixtureGate}\npub(super) fn compose_development_portal_from_config_with_did_approvals(`)
        || source.split("Some(did_approvals)").length !== 2) failures.push(path);
      continue;
    }
    if (path === fixtureExecutablePath) {
      if (!source.includes("compose_native_headless_process_with_development_did_approval_from_environment()")
        || /(?:std::env|serde|Deserialize)/u.test(source)) failures.push(path);
      continue;
    }
    if (path === demoApplicationPath) {
      const fixtureCall = "oxid_composition::compose_headless_with_development_did_approval()";
      const fixtureGate = `#[cfg(all(
        feature = "standalone-development",
        not(feature = "standalone-native-custody"),
        feature = "ui-profile-demo",
        not(feature = "standalone-tailnet"),
        not(feature = "standalone-local"),
        not(feature = "standalone-portal-tailnet"),
        not(feature = "desktop-portal-test"),
        not(target_arch = "wasm32")
    ))]`;
      if (!source.includes(`${fixtureGate}\n    let application = ${fixtureCall};`)
        || source.split(fixtureCall).length !== 2) failures.push(path);
      continue;
    }
    if (path.startsWith("apps/oxid-headless/tests/") || [
      "crates/composition/src/profile_in_memory/tests.rs",
      "crates/composition/src/passport_vault/tests.rs",
      "crates/composition/src/verification.rs",
    ].includes(path)) continue;
    if (/\b(?:development_did_approvals|compose_in_memory_with_development_did_approval|compose_headless_with_development_did_approval)\b/u.test(source)) { failures.push(path); continue; }
    if (/\bDidApprovalService\b/u.test(source) && ![
      "crates/identity/application/src/lib.rs",
      "crates/identity/application/src/lifecycle.rs",
      "crates/identity/application/src/lifecycle/tests/approval_enforcement.rs",
      "crates/composition/src/wiring.rs",
    ].includes(path)) failures.push(path);
  }
  return failures;
}

test("DID authority is unavailable except for the feature-gated explicit fixture", () => {
  const paths = execFileSync("git", ["ls-files", "-z", "--", "*.rs"], { encoding: "utf8" }).split("\0").filter(Boolean);
  assert(paths.includes(modulePath));
  assert(paths.includes(fixturePath));
  assert.deepEqual(violations(paths.map(path => [path, readFileSync(path, "utf8")])), []);

  const manifest = readFileSync(headlessManifestPath, "utf8");
  assert.match(manifest, /development-did-approval-fixture = \["oxid-composition\/development-did-approval"\]/u);
  assert.match(manifest, /name = "oxid-headless-development-did-approval-fixture"[\s\S]*required-features = \["development-did-approval-fixture"\]/u);
  assert.doesNotMatch(manifest, /default = \[[^\]]*development-did-approval-fixture/u);

  const demoManifest = readFileSync(demoManifestPath, "utf8");
  assert.match(
    demoManifest,
    /ui-profile-demo = \[[\s\S]*?"oxid-composition\/development-did-approval"[\s\S]*?\]/u,
  );
  assert.doesNotMatch(demoManifest, /default = \[[^\]]*ui-profile-demo/u);

  assert.match(fixtureExecutablePath, /^apps\/oxid-headless\/tests\/support\//u);
});

test("guard rejects injection, alias imports, new producers and ungated fixtures", () => {
  for (const source of [
    "DidApprovalService::with_trusted_did_port(clock, port)",
    "use oxid_identity_application::TrustedDidApprovalPort as Port;",
    "impl TrustedDidApprovalPort for AutoApprove {}",
    "use oxid_identity_application::development_did_approvals as approve;",
    "compose_in_memory_with_development_did_approval()",
    "compose_headless_with_development_did_approval()",
    "use oxid_identity_application::DidApprovalService as Authority;",
    "service.with_approvals(authority, hash)",
    "DidService::with_approvals(service, authority, hash)",
  ]) {
    const path = "crates/adapters/untrusted/src/lib.rs";
    assert.deepEqual(violations([[path, source]]), [path]);
    for (const adapter of testConsumers) assert.deepEqual(violations([[adapter, source]]), [adapter]);
  }
  const source = readFileSync(modulePath, "utf8");
  for (const changed of [
    source.replace("Err(TrustedDidApprovalError::Unavailable)", "Ok(())"),
    source + "\nimpl TrustedDidApprovalPort for AutoApprove {}",
    source.replace("#[cfg(test)]\npub(crate) mod tests;", "pub(crate) mod tests;"),
    source.replace(`${fixtureGate}\nmod development;`, "mod development;"),
    source.replace(`${fixtureGate}\npub use`, "pub use"),
  ]) assert.deepEqual(violations([[modulePath, changed]]), [modulePath]);
  const composition = readFileSync(compositionPath, "utf8");
  assert.deepEqual(violations([[compositionPath, composition.replace(compositionGate, "")]]), [compositionPath]);
  assert.deepEqual(violations([[compositionPath, composition + '\nstd::env::var("APPROVE");']]), [compositionPath]);
  const persistentComposition = readFileSync(persistentCompositionPath, "utf8");
  assert.deepEqual(
    violations([[persistentCompositionPath, persistentComposition.replace('#[cfg(feature = "development-did-approval")]\n', "")]]),
    [persistentCompositionPath],
  );
  const fixtureExecutable = readFileSync(fixtureExecutablePath, "utf8");
  assert.deepEqual(
    violations([[fixtureExecutablePath, fixtureExecutable + '\nstd::env::var("APPROVE");']]),
    [fixtureExecutablePath],
  );
  const environmentComposition = readFileSync(environmentCompositionPath, "utf8");
  assert.deepEqual(
    violations([[environmentCompositionPath, environmentComposition.replace(environmentFixtureGate, "")]]),
    [environmentCompositionPath],
  );
  const portalComposition = readFileSync(portalCompositionPath, "utf8");
  assert.deepEqual(
    violations([[portalCompositionPath, portalComposition.replace(portalFixtureGate, "")]]),
    [portalCompositionPath],
  );
});
