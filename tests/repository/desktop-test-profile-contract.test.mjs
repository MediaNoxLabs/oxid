// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const text = (relative) => readFile(new URL(relative, root), "utf8");

test("ARM64 Darwin desktop Portal remains owner-invoked and outside HostedTarget", async () => {
  const [justfile, planner, harness] = await Promise.all([
    text("Justfile"),
    text("scripts/ci/target-plan.mjs"),
    text("scripts/e2e/portal-desktop-e2e.sh"),
  ]);
  assert.match(justfile, /portal-desktop-e2e:\n\s+\.\/scripts\/e2e\/portal-desktop-e2e\.sh/);
  assert.doesNotMatch(planner, /darwin|macos|desktop-portal/i);
  assert.match(harness, /target\/debug\/oxid-app/);
  assert.match(harness, /Mach-O 64-bit arm64/);
  assert.match(harness, /CGWindowListCopyWindowInfo/);
  assert.match(harness, /kCGWindowOwnerPID/);
  assert.doesNotMatch(harness, /kCGWindowLayer/);
  assert.match(harness, /"\$x" =~ \^-\?\[0-9\]\+\$ && "\$y" =~ \^-\?\[0-9\]\+\$/);
  assert.match(harness, /\/Applications\/Xcode\.app\/Contents\/Developer/);
  assert.match(harness, /\/usr\/bin\/xcrun --sdk macosx swiftc/);
  assert.doesNotMatch(harness, /System Events|osascript|xcode-select -p/);
  assert.doesNotMatch(harness, /Xvfb|Openbox|xdotool|WebKitGTK|x86_64-linux/i);
  assert.match(harness, /\.state == "empty"/);
  assert.doesNotMatch(harness, /\.state == "consumed"/);
  assert.match(harness, /rm -f -- "\$CONTROL_ROOT\/driver-admitted"/);
});

test("canonical macOS laptop lane runs headless before desktop and validates both exact-head records", async () => {
  const [justfile, runner] = await Promise.all([text("Justfile"), text("run.sh")]);
  const match = justfile.match(/^portal-macos-laptop-e2e:\n((?:    .*\n)+)/m);
  assert.ok(match, "missing no-argument portal-macos-laptop-e2e recipe");

  const recipe = match[1];
  const headless = [...recipe.matchAll(/just portal-headless-e2e/g)];
  const desktop = [...recipe.matchAll(/just portal-desktop-e2e/g)];
  assert.equal(headless.length, 1);
  assert.equal(desktop.length, 1);
  assert.ok(headless[0].index < desktop[0].index, "headless must precede desktop");
  const aggregateStart = recipe.indexOf("jq -s -e");
  assert.ok(desktop[0].index < aggregateStart, "both harnesses must precede aggregation");
  const aggregate = recipe.slice(aggregateStart, recipe.indexOf("\n    echo"));
  assert.equal([...aggregate.matchAll(/\bjq /g)].length, 1);
  assert.match(aggregate, /--arg head "\$\(git rev-parse HEAD\)"/);
  assert.match(aggregate, /--arg tree "\$\(git rev-parse 'HEAD\^\{tree\}'\)"/);
  assert.match(aggregate, /length == 2 and all\(\.\[\]; \.oxid == \{head:\$head,tree:\$tree\}\)/);
  assert.match(aggregate, /target\/portal-headless-e2e\/evidence\.json/);
  assert.match(aggregate, /target\/portal-desktop-e2e\/evidence\.json/);
  assert.match(recipe, /portal-macos-laptop-e2e: PASS evidence=target\/portal-headless-e2e\/evidence\.json,target\/portal-desktop-e2e\/evidence\.json/);

  const registration = "node --test tests/repository/desktop-test-profile-contract.test.mjs";
  assert.equal(runner.split(registration).length - 1, 1, "desktop contract must be registered exactly once");
  assert.match(runner.match(/run_repository\(\) \{([\s\S]*?)\n\}/)?.[1] ?? "", new RegExp(registration.replaceAll(".", "\\.")));
});

test("macOS runbook keeps standalone cleanup authority process-local and fail-scoped", async () => {
  const runbook = await text("docs/factory/portal-macos-laptop.md");
  const commands = runbook.match(/## Owner-safe execution[\s\S]*?```bash\n([\s\S]*?)\n```/)?.[1];
  assert.ok(commands, "missing owner-safe command block");

  const execute = async ({ dockerOutput = "", dockerStatus = 0, scenarioStatus = 0, downStatus = 0 }) => {
    const sandbox = await mkdtemp(path.join(tmpdir(), "oxid-portal-ownership-"));
    const ownershipFile = path.join(sandbox, "tmp/portal-macos-laptop/ownership.txt");
    const bin = path.join(sandbox, "bin");
    const justLog = path.join(sandbox, "just.log");
    const dockerLog = path.join(sandbox, "docker.log");
    try {
      await mkdir(path.dirname(ownershipFile), { recursive: true });
      await mkdir(bin);
      await writeFile(ownershipFile, "standalone_preexisting=false\n");
      await writeFile(path.join(bin, "docker"), `#!/bin/sh
printf '%s\\n' "$*" >> "$DOCKER_LOG"
if [ "$DOCKER_STATUS" -eq 0 ]; then
  printf '%s' "$DOCKER_OUTPUT"
fi
exit "$DOCKER_STATUS"
`);
      await writeFile(path.join(bin, "just"), `#!/bin/sh
printf '%s\\n' "$1" >> "$JUST_LOG"
[ "$#" -eq 1 ] || exit 96
case "$1" in
  standalone-up) exit 0 ;;
  portal-macos-laptop-e2e) exit "$SCENARIO_STATUS" ;;
  standalone-down) exit "$DOWN_STATUS" ;;
  *) exit 97 ;;
esac
`);
      await writeFile(path.join(bin, "git"), `#!/bin/sh
if [ "$#" -eq 3 ] && [ "$1" = status ] && [ "$2" = --porcelain ] && [ "$3" = --untracked-files=no ]; then
  exit 0
fi
if [ "$#" -eq 2 ] && [ "$1" = rev-parse ] && [ "$2" = HEAD ]; then
  printf '%040d\\n' 0
  exit 0
fi
if [ "$#" -eq 2 ] && [ "$1" = rev-parse ] && [ "$2" = 'HEAD^{tree}' ]; then
  printf '%040d\\n' 1
  exit 0
fi
exit 98
`);
      await writeFile(path.join(bin, "jq"), "#!/bin/sh\nexit 0\n");
      await Promise.all(["docker", "just", "git", "jq"].map((name) => chmod(path.join(bin, name), 0o755)));

      const result = spawnSync("/bin/bash", ["-x", "-c", commands], {
        cwd: sandbox,
        encoding: "utf8",
        env: {
          ...process.env,
          DOCKER_LOG: dockerLog,
          DOCKER_OUTPUT: dockerOutput,
          DOCKER_STATUS: String(dockerStatus),
          DOWN_STATUS: String(downStatus),
          JUST_LOG: justLog,
          PATH: `${bin}:${process.env.PATH ?? ""}`,
          SCENARIO_STATUS: String(scenarioStatus),
        },
      });
      const optionalText = async (file) => {
        try {
          return await readFile(file, "utf8");
        } catch (error) {
          if (error.code === "ENOENT") return null;
          throw error;
        }
      };
      const logLines = async (file) => (await optionalText(file))?.trim().split("\n").filter(Boolean) ?? [];
      return {
        dockerCalls: await logLines(dockerLog),
        justTargets: await logLines(justLog),
        legacyOwnership: await optionalText(ownershipFile),
        result,
      };
    } finally {
      await rm(sandbox, { recursive: true, force: true });
    }
  };

  const queryFailure = await execute({ dockerStatus: 23 });
  assert.equal(queryFailure.result.status, 1, `Docker query failure must exit 1: ${queryFailure.result.stderr}`);
  assert.deepEqual(queryFailure.justTargets, [], "Docker query failure must not invoke any just target");
  assert.equal(queryFailure.legacyOwnership, "standalone_preexisting=false\n", "legacy ownership state must remain untouched and untrusted");
  assert.match(queryFailure.result.stderr, /standalone ownership query failed; no cleanup authority installed and no stack command run/);

  const ownedFailure = await execute({ scenarioStatus: 42 });
  assert.equal(ownedFailure.result.status, 42, `scenario status must survive owned cleanup: ${ownedFailure.result.stderr}`);
  assert.deepEqual(ownedFailure.justTargets, ["standalone-up", "portal-macos-laptop-e2e", "standalone-down"]);

  const preexistingFailure = await execute({ dockerOutput: "preexisting-container", scenarioStatus: 42 });
  assert.equal(preexistingFailure.result.status, 42, `pre-existing scenario failure must be preserved: ${preexistingFailure.result.stderr}`);
  assert.deepEqual(preexistingFailure.justTargets, ["standalone-up", "portal-macos-laptop-e2e"]);

  const success = await execute({ dockerOutput: "" });
  assert.equal(success.result.status, 0, success.result.stderr);
  assert.deepEqual(success.justTargets, ["standalone-up", "portal-macos-laptop-e2e"]);
  assert.match(success.result.stderr, /\+ trap - EXIT/, "successful execution must disarm the EXIT trap");

  const cleanupFailure = await execute({ scenarioStatus: 42, downStatus: 43 });
  assert.equal(cleanupFailure.result.status, 42, "cleanup failure must not hide the scenario failure");
  assert.deepEqual(cleanupFailure.justTargets, ["standalone-up", "portal-macos-laptop-e2e", "standalone-down"]);
  assert.match(cleanupFailure.result.stderr, /owned standalone cleanup failed \(exit 43\); preserving stack state for owner review; no force deletion attempted/);

  assert.equal(queryFailure.dockerCalls.length, 1);
  assert.doesNotMatch(commands, /ownership\.txt|ownership_file/, "the command block must not use persisted ownership state");
  const queryEnd = commands.indexOf("fi", commands.indexOf("docker ps"));
  const ownershipAssignment = commands.indexOf("standalone_owned=false");
  const trapInstall = commands.indexOf("trap cleanup_owned_standalone_on_failure EXIT");
  const trapDisarm = commands.lastIndexOf("trap - EXIT");
  assert.ok(queryEnd >= 0 && queryEnd < ownershipAssignment, "the Docker baseline must succeed before ownership is established");
  assert.ok(ownershipAssignment < trapInstall, "ownership must be established before installing the failure trap");
  assert.ok(trapInstall < trapDisarm, "the success path must disarm the installed failure trap");
  assert.match(runbook, /legacy `tmp\/portal-macos-laptop\/ownership\.txt` files? (?:is|are) untrusted\s+historical state/i);
  assert.match(runbook, /(?:it|they)\s+never authorize cleanup/i);
});

test("desktop test feature is exact and its rendered-control driver has no direct capability calls", async () => {
  const [appManifest, compositionManifest, ingressManifest, driver, ui, main] = await Promise.all([
    text("apps/oxid/Cargo.toml"),
    text("crates/composition/Cargo.toml"),
    text("crates/adapters/identity-ingress/Cargo.toml"),
    text("crates/ui-dioxus/src/desktop_test_driver.rs"),
    text("crates/ui-dioxus/src/lib.rs"),
    text("apps/oxid/src/main.rs"),
  ]);
  assert.match(appManifest, /desktop-portal-test = \[[\s\S]*"desktop"[\s\S]*"standalone-development"[\s\S]*"oxid-composition\/desktop-portal-test"[\s\S]*"oxid-ui-dioxus\/desktop-test-click-driver"[\s\S]*\]/);
  assert.match(compositionManifest, /desktop-portal-test = \["oxid-adapter-identity-ingress\/desktop-test-qr-scanner"\]/);
  assert.match(ingressManifest, /desktop-test-qr-scanner = \["dep:zeroize"\]/);
  assert.match(main, /OXID_DESKTOP_PORTAL_TEST_PROFILE/);
  assert.match(ui, /desktop_test_driver::use_desktop_test_driver\(\);/);
  assert.match(driver, /pub\(super\) fn use_desktop_test_driver\(\)/);
  assert.match(driver, /\.click\(\)/);
  assert.match(driver, /dioxus_document::eval/);
  assert.doesNotMatch(driver, /qr_scanner|route_identity_request|UseCase|\.execute\(/);
});

test("normal release gate excludes every desktop-test marker and localhost route", async () => {
  const release = await text("scripts/check-ui-profile-release.sh");
  assert.match(release, /OXID_DESKTOP_PORTAL_TEST_PROFILE/);
  assert.match(release, /desktop-portal-test compiled outside ARM64 macOS/);
  assert.match(release, /portal-offer\\\.capability/);
});
