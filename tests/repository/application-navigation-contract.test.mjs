// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("responsive application navigation keeps one accessible leading Back action", async () => {
  const [ui, styles, design, androidActivity] = await Promise.all([
    text("crates/ui-dioxus/src/lib.rs"),
    text("crates/ui-dioxus/assets/styles.css"),
    text("docs/design/application-navigation.md"),
    text("apps/oxid/android/MainActivity.kt"),
  ]);

  assert.match(ui, /div \{ class: "app-header__leading",[\s\S]*?class: "back-action"/);
  assert.match(ui, /aria_label: "Go back"/);
  assert.match(ui, /navigation\.write\(\)\.pop\(\)/);
  assert.match(ui, /class: "app-header__actions",[\s\S]*?profile-shortcut[\s\S]*?GlobalMenuTrigger/);
  assert.equal((ui.match(/aria_label: "Go back"/g) ?? []).length, 1);

  assert.match(styles, /grid-template-columns: auto minmax\(0, 1fr\) auto/);
  assert.match(styles, /\.back-action \{[\s\S]*?width: 3rem;[\s\S]*?min-height: 3rem;/);
  assert.match(styles, /\.app-header__title strong \{[\s\S]*?max-width: 100%;[\s\S]*?text-overflow: ellipsis/);
  assert.match(styles, /\[dir="rtl"\] \.back-action__icon/);
  assert.match(styles, /\.profile-sheet \{[\s\S]*?right: max\(1rem, env\(safe-area-inset-right\)\);[\s\S]*?left: auto;/);
  assert.match(design, /390.*430.*768/s);
  assert.match(design, /48dp.*44pt/s);

  assert.match(androidActivity, /override val handleBackNavigation: Boolean = false/);
  assert.match(androidActivity, /OnBackPressedCallback/);
  assert.match(androidActivity, /document\.querySelector\('button\.back-action'\)/);
  assert.match(androidActivity, /action\.click\(\)/);
  assert.match(androidActivity, /onBackPressedDispatcher\.onBackPressed\(\)/);
  assert.match(design, /Android system and gesture Back.*activity bridge/s);
  assert.match(design, /without claiming a native Back input/s);
});
