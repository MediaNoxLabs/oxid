// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);

test("Portal iOS holder preparation follows the current DID detail contract", async () => {
  const [fixture, dids] = await Promise.all([
    readFile(
      new URL("tests/mobile/ios/OxidUITests/PortalFlowTests.swift", root),
      "utf8",
    ),
    readFile(new URL("crates/ui-dioxus/src/dids.rs", root), "utf8"),
  ]);

  assert.match(fixture, /buttons\["Copy DID"\]/u);
  assert.doesNotMatch(fixture, /staticTexts\["Identity"\]/u);
  assert.match(
    fixture,
    /protocolUnavailableErrorStatus\(in: application\)\.waitForExistence/u,
  );
  assert.match(
    fixture,
    /XCTAssertFalse\(application\.buttons\["Leave credential review"\]\.exists\)/u,
  );
  assert.match(dids, /aria_label: "Copy DID"/u);
});
