// SPDX-License-Identifier: Apache-2.0

export const EXPECTED_PI_PACKAGES = new Map([
  ["dev-loops", "1.0.2"],
  ["@dev-loops/core", "1.0.2"],
  ["pi-subagents", "0.70.0"],
  ["@playwright/test", "1.60.0"],
  ["@axe-core/playwright", "4.10.0"],
  ["typebox", "1.3.9"],
  ["pi-taskflow", "0.2.10"],
  ["@input-output-hk/agent-review-pi", "0.6.0"],
  ["@grafana/agento11y-pi", "0.25.0"],
]);

export const expectedPiPackageVersion = (name) => {
  const version = EXPECTED_PI_PACKAGES.get(name);
  if (!version) throw new Error(`no tracked Pi package policy for ${name}`);
  return version;
};
