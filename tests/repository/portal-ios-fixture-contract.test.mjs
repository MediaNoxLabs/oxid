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
    /staticTexts\["Credential offer preview"\]\.waitForNonExistence/u,
  );
  assert.match(fixture, /let leave = application\.buttons\["Leave credential review"\]/u);
  assert.doesNotMatch(fixture, /buttons\["Receive"\]\.waitForExistence/u);
  assert.match(dids, /aria_label: "Copy DID"/u);
});

test("Portal iOS acceptance isolates a retained issuance failure before success", async () => {
  const runner = await readFile(
    new URL("scripts/test-ios-portal-exact-sequence-simulator.sh", root),
    "utf8",
  );

  assert.match(
    runner,
    /run_measured_offer issue-error[\s\S]*?oxid_ios_owned_simctl[^\n]*terminate "\$PACKAGE"[\s\S]*?run_measured_offer issue testIssue/u,
  );
  assert.match(runner, /fail issue-error-reset/u);
});

test("Portal acceptance uses only named development authority and bounded cleanup", async () => {
  const [appManifest, profile, headless, lifecycle, liveFlow] = await Promise.all([
    readFile(new URL("apps/oxid/Cargo.toml", root), "utf8"),
    readFile(
      new URL("crates/composition/src/profile_mobile.rs", root),
      "utf8",
    ),
    readFile(new URL("scripts/e2e/portal-headless-e2e.sh", root), "utf8"),
    readFile(new URL("scripts/portal-consumer-lifecycle.sh", root), "utf8"),
    readFile(
      new URL("apps/oxid-headless/tests/portal_live_flow.rs", root),
      "utf8",
    ),
  ]);

  const localPortalFeature = appManifest.match(
    /^standalone-portal = \[(?<members>[\s\S]*?)^\]/mu,
  );
  const tailnetPortalFeature = appManifest.match(
    /^standalone-portal-tailnet = \[(?<members>[\s\S]*?)^\]/mu,
  );
  assert.ok(localPortalFeature?.groups?.members);
  assert.ok(tailnetPortalFeature?.groups?.members);
  assert.match(
    localPortalFeature.groups.members,
    /oxid-composition\/development-did-approval/u,
  );
  assert.doesNotMatch(
    tailnetPortalFeature.groups.members,
    /development-did-approval/u,
  );
  assert.match(
    profile,
    /let did_approvals = Some\(super::profile_headless::development_did_approval_service\(\)\)/u,
  );
  assert.match(headless, /--features development-did-approval-fixture/u);
  assert.match(lifecycle, /timeout -k 5s 40s docker compose/u);
  assert.match(lifecycle, /force_remove_owned_project/u);
  assert.doesNotMatch(lifecycle, /compose_bounded down --volumes/u);
  assert.match(
    lifecycle,
    /docker inspect --format '[^']*com\.docker\.compose\.project/u,
  );
  assert.match(lifecycle, /docker rm --force "\$id"/u);
  assert.match(lifecycle, /\^\[0-9a-f\]\{12,64\}\$/u);
  assert.match(lifecycle, /for attempt in 1 2/u);
  assert.match(liveFlow, /if !thread::panicking\(\)/u);
  assert.match(liveFlow, /remove_dir_all\(&self\.0\)/u);
});
