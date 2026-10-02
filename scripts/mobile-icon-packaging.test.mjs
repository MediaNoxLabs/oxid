// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function source(name) {
  return readFile(path.join(root, "scripts", name), "utf8");
}

test("native icon packager replaces Dioxus defaults with platform launcher resources", async () => {
  const packager = await source("package-mobile-icons.py");
  assert.match(packager, /AppIcon60x60@3x\.png/);
  assert.match(packager, /CFBundleIcons/);
  assert.match(packager, /mipmap-hdpi/);
  assert.match(packager, /ic_launcher_foreground/);
  assert.match(packager, /adaptive-icon/);
});

test("mobile launchers package and verify icons before receipts", async () => {
  const [ios, android] = await Promise.all([
    source("run-ios-simulator.sh"),
    source("run-android-emulator.sh"),
  ]);
  for (const launcher of [ios, android]) {
    const packaged = launcher.indexOf("package-mobile-icons.py");
    const receipt = launcher.indexOf('app-artifact-receipt.mjs" write');
    assert.ok(packaged >= 0 && packaged < receipt, "icons must be packaged before the receipt");
  }
  assert.match(ios, /package-mobile-icons\.py" ios/);
  assert.match(android, /package-mobile-icons\.py" android/);
});

test("brand generator produces an opaque iOS source and fails when ICNS cannot be generated", async () => {
  const generator = await source("generate-oxid-brand-assets.py");
  assert.match(generator, /opaque=name == "app-icon-dark-1024\.png"/);
  assert.match(generator, /iconutil is required to package the macOS icon/);
});
