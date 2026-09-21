// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function command(program, args) {
  return execFileSync(program, args, { cwd: root, encoding: "utf8" });
}

test("desktop live helper uses an optimized standalone profile while ordinary desktop stays dev", () => {
  const justfile = readFileSync(path.join(root, "Justfile"), "utf8");
  const live = justfile.match(/^desktop-live-run:\n((?:    .*\n)+)/m)?.[1] ?? "";
  assert.match(live, /cargo run --profile desktop-live -p oxid-app/);
  assert.match(live, /--no-default-features/);
  assert.match(live, /--features desktop,standalone-development,standalone-local/);

  const ordinary = justfile.match(/^desktop-run:\n((?:    .*\n)+)/m)?.[1] ?? "";
  assert.match(ordinary, /^\s*cargo run -p oxid-app\s*$/m);
  assert.doesNotMatch(ordinary, /desktop-live|standalone-development|standalone-local/);
});

test("Cargo resolves desktop-live to release runtime checks without LTO or stripping", () => {
  const output = command("cargo", [
    "check",
    "--profile", "desktop-live",
    "-p", "oxid-foundation",
    "--lib",
    "--message-format=json",
  ]);
  const artifact = output
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line))
    .find((entry) => entry.reason === "compiler-artifact"
      && entry.target?.name === "oxid_foundation");

  assert.ok(artifact, "Cargo must emit the oxid-foundation compiler artifact");
  assert.equal(artifact.profile.opt_level, "3");
  assert.equal(artifact.profile.debuginfo, 1);
  assert.equal(artifact.profile.debug_assertions, false);
  assert.equal(artifact.profile.overflow_checks, false);
  assert.equal(artifact.profile.test, false);
});
