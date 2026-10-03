// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("Receive keeps its close action outside the scrollable address content", async () => {
  const [styles, source, iosJourney] = await Promise.all([
    text("crates/ui-dioxus/assets/styles.css"),
    text("crates/ui-dioxus/src/lib.rs"),
    text("tests/mobile/ios/OxidUITests/ProfileFlowTests.swift"),
  ]);

  assert.match(
    styles,
    /\.receive-sheet \{[\s\S]*?overflow: hidden;[\s\S]*?display: flex;/,
  );
  assert.match(styles, /\.receive-sheet__body \{[\s\S]*?overflow-y: auto;/);
  assert.match(source, /class: "receive-sheet__body"[\s\S]*?\{content\}/);
  assert.match(
    source,
    /fn ReceiveSheet[\s\S]*?window\.scrollTo\(\{ top: 0[\s\S]*?querySelector\('\.page-content'\)\?\.scrollTo\(\{ top: 0/,
  );
  assert.match(
    iosJourney,
    /privateSelector\.tap\(\)[\s\S]*?closeReceiveAfterSwitch\.waitForExistence[\s\S]*?closeReceiveAfterSwitch\.isHittable[\s\S]*?closeReceiveAfterSwitch\.tap\(\)/,
  );
});
