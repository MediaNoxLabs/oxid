// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const app = await readFile("crates/ui-dioxus/src/lib.rs", "utf8");
const assets = await readFile("crates/ui-dioxus/src/assets_page.rs", "utf8");
const vault = await readFile("crates/ui-dioxus/src/passport_vault.rs", "utf8");

test("primary routes do not repeat their route title in the shared context row", () => {
  assert.match(
    app,
    /id: "destination-heading",[\s\S]*?role: "heading",[\s\S]*?aria_level: "1"/,
  );
  assert.doesNotMatch(app, /page-context__title/);
  assert.doesNotMatch(app, /fn page_context_primary_label/);
});

test("primary page content does not repeat the visible destination heading", () => {
  assert.doesNotMatch(app, /h1 \{ "Documents" \}/);
  assert.doesNotMatch(app, /h1 \{ "Activity" \}/);
  assert.doesNotMatch(vault, /h1 \{ "Passport Vault" \}/);
  assert.doesNotMatch(assets, /Wallet overview/);
  assert.match(vault, /aria_label: "Loading Passport Vault"/);
});

test("realm and balance values remain subordinate to the route heading", () => {
  assert.match(app, /h2 \{ class: "home-hero__realm-title"/);
  assert.doesNotMatch(app, /h1 \{ class: "home-hero__realm-title"/);
  assert.match(assets, /strong \{ class: "wallet-balance/);
  assert.doesNotMatch(assets, /h1 \{/);
});
