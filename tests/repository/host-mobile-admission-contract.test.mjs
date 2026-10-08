// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import { parseHostMobileArgs, runHostMobile } from "../../scripts/e2e/host-mobile-supervisor.mjs";

const head = "a".repeat(40);
const argv = [
  "--delivery-base", "origin/milestone-0.2.0",
  "--scenario", "canonical-ios", "--timeout-seconds", "30", "--cwd", "/repo",
  "--", "true",
];

test("host-mobile admission keeps the normalized delivery base outside the child command", () => {
  assert.deepEqual(parseHostMobileArgs(argv), {
    deliveryBase: "origin/milestone-0.2.0",
    args: ["--scenario", "canonical-ios", "--timeout-seconds", "30", "--cwd", "/repo", "--", "true"],
  });
  assert.deepEqual(parseHostMobileArgs([...argv, "--delivery-base", "child-value"]), {
    deliveryBase: "origin/milestone-0.2.0",
    args: [...argv.slice(2), "--delivery-base", "child-value"],
  });
  for (const invalid of [
    argv.filter((value, index) => index > 1),
    ["--delivery-base", "milestone-0.2.0", ...argv.slice(2)],
    ["--delivery-base", "origin/main", ...argv.slice(2)],
    ["--delivery-base", "origin/develop", "--delivery-base", "origin/develop", ...argv.slice(2)],
  ]) assert.throws(() => parseHostMobileArgs(invalid), /delivery-base/u);
});

test("missing exact-head gate rejects before host-mobile lease or child launch", async () => {
  let launched = 0;
  let output = "";
  await assert.rejects(runHostMobile(argv, {
    cwd: "/repo",
    readHead: () => head,
    verifyGate: () => { throw new Error("missing receipt"); },
    runSupervised: async () => { launched += 1; return 0; },
    stderr: { write(value) { output += value; } },
    now: () => 1,
  }), /local-gate-unavailable/u);
  assert.equal(launched, 0);
  assert.match(output, new RegExp(`result=rejected reason=missing-local-gate head=${head} cleanup=not-acquired`, "u"));
});

test("gate evidence cannot admit a child rooted in another checkout", async () => {
  let verified = 0;
  await assert.rejects(runHostMobile(argv, {
    cwd: "/other-repo",
    readHead: () => head,
    verifyGate: () => { verified += 1; },
    runSupervised: async () => 0,
    stderr: { write() {} },
    now: () => 1,
  }), /child-cwd-mismatch/u);
  assert.equal(verified, 0);
});

test("occupied lane and accepted execution emit bounded payload-free outcomes", async () => {
  let output = "";
  await assert.rejects(runHostMobile(argv, {
    cwd: "/repo",
    readHead: () => head,
    verifyGate: () => {},
    runSupervised: async () => { throw new Error("lease-busy-existing-run"); },
    stderr: { write(value) { output += value; } },
    now: () => 2,
  }), /lease-busy/u);
  assert.match(output, /result=rejected reason=occupied-lane .* cleanup=not-acquired/u);

  output = "";
  assert.equal(await runHostMobile(argv, {
    cwd: "/repo",
    readHead: () => head,
    verifyGate: () => {},
    runSupervised: async () => 0,
    stderr: { write(value) { output += value; } },
    now: () => 3,
  }), 0);
  assert.match(output, /result=passed reason=none .* cleanup=released/u);
});
