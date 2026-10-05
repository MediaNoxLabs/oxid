// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

async function workspaceVersion() {
  const manifest = await readFile(new URL("Cargo.toml", root), "utf8");
  const workspacePackage = manifest.match(/^\[workspace\.package\]\nversion = "([^"]+)"$/mu);
  assert.ok(workspacePackage, "Cargo.toml must declare [workspace.package].version");
  return workspacePackage[1];
}

test("workspace packages and locked metadata report the release version", async () => {
  const releaseVersion = await workspaceVersion();
  const metadata = JSON.parse(execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--offline", "--no-deps"],
    { cwd: root, encoding: "utf8", timeout: 30_000 },
  ));
  const workspaceMembers = new Set(metadata.workspace_members);
  const packages = metadata.packages.filter(({ id }) => workspaceMembers.has(id));

  assert.ok(packages.length > 0, "workspace package inventory must not be empty");
  assert.deepEqual(
    packages.filter(({ version }) => version !== releaseVersion).map(({ name, version }) => `${name}@${version}`),
    [],
    "every workspace package must project the release version from Cargo metadata",
  );

  const lock = await readFile(new URL("Cargo.lock", root), "utf8");
  const lockVersions = new Map(
    [...lock.matchAll(/^\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/gmu)]
      .map(([, name, version]) => [name, version]),
  );
  assert.deepEqual(
    packages.filter(({ name }) => lockVersions.get(name) !== releaseVersion)
      .map(({ name }) => `${name}@${lockVersions.get(name) ?? "missing"}`),
    [],
    "every workspace package entry in Cargo.lock must use the release version",
  );
});

test("Nix release artifacts report the workspace release version", async () => {
  const releaseVersion = await workspaceVersion();
  const nixArtifacts = [
    "nix/packages/default.nix",
    "nix/packages/passport-vault-call-composer.nix",
    "nix/packages/passport-vault-compact-artifacts.nix",
    "nix/packages/presentation-compact-artifacts.nix",
  ];

  for (const path of nixArtifacts) {
    const source = await readFile(new URL(path, root), "utf8");
    const versions = [...source.matchAll(/^\s*version = "([^"]+)";$/gmu)]
      .map(([, version]) => version);
    assert.ok(versions.length > 0, `${path} must declare at least one release artifact version`);
    assert.deepEqual(
      versions.filter((version) => version !== releaseVersion),
      [],
      `${path} must project the workspace release version`,
    );
  }
});

test("release changelog retains Unreleased work and required operator notes", async () => {
  const releaseVersion = await workspaceVersion();
  const changelog = await readFile(new URL("CHANGELOG.md", root), "utf8");
  assert.match(changelog, /^## \[Unreleased\]$/mu);
  assert.match(changelog, new RegExp(`^## \\[${releaseVersion.replaceAll(".", "\\.")}\\] - \\d{4}-\\d{2}-\\d{2}$`, "mu"));
  assert.match(changelog, /^### Highlights$/mu);
  assert.match(changelog, /^### Security$/mu);
  assert.match(changelog, /^### Known limitations$/mu);
  assert.match(changelog, /^### Upgrade and backup compatibility$/mu);
  assert.match(changelog, /older builds cannot open v6 exports/u);
});
