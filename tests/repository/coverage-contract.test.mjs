// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  acquireCoverageLock,
  discoverWorkspacePackages,
  normalizeLlvmReport,
  parseArguments,
  runCoverage,
  validatePolicy,
} from "../../scripts/coverage/run.mjs";

const repoRoot = path.resolve(new URL("../..", import.meta.url).pathname);
const policyPath = path.join(repoRoot, "scripts/coverage/policy.json");
const HEAD = "852d6156ee656b3e0aa44c0bbf6900bc8aa9fa0c";
const BASE = "9db7762f970dd895c1a0f806d17d9d4939840303";

async function loadPolicy() {
  return JSON.parse(await readFile(policyPath, "utf8"));
}

function fakeGit(overrides = {}) {
  const state = { head: HEAD, base: BASE, status: "" };
  return {
    state,
    resolve(ref) {
      return ref === "HEAD" ? state.head : state.base;
    },
    status() {
      return state.status;
    },
    isAncestor() {
      return true;
    },
    diff() {
      return "";
    },
    ...overrides,
  };
}

function llvmReport(filename = path.join(repoRoot, "crates/foundation/src/lib.rs"), covered = 80) {
  return {
    data: [{
      files: [{
        filename,
        summary: { lines: { count: 100, covered, percent: covered } },
      }],
      totals: { lines: { count: 100, covered, percent: covered } },
    }],
    type: "llvm.coverage.json.export",
    version: "2.0.1",
  };
}

function llvmScopeReport(scopeId, policy, packageInventory) {
  const names = scopeId === "workspace-aggregate"
    ? [
      ...policy.classifications.core,
      ...policy.classifications.critical,
      ...policy.classifications.workspaceOnly,
    ]
    : policy.classifications.additionalScopes
      .filter(({ scope }) => scope === scopeId)
      .map(({ package: packageName }) => packageName);
  const byName = new Map(packageInventory.map((entry) => [entry.name, entry]));
  const files = names.map((name) => ({
    filename: path.join(repoRoot, byName.get(name).root, "src/lib.rs"),
    summary: { lines: { count: 100, covered: 100, percent: 100 } },
  }));
  const count = files.length * 100;
  return {
    data: [{ files, totals: { lines: { count, covered: count, percent: 100 } } }],
    type: "llvm.coverage.json.export",
    version: "2.0.1",
  };
}

async function withTemp(t, callback) {
  const parent = path.join(repoRoot, "target");
  await mkdir(parent, { recursive: true });
  const directory = await mkdtemp(path.join(parent, "coverage-contract-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return callback(directory);
}

async function listFiles(root) {
  const found = [];
  async function visit(current) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const candidate = path.join(current, entry.name);
      if (entry.isDirectory()) await visit(candidate);
      else found.push(candidate);
    }
  }
  await visit(root);
  return found;
}

async function runSynthetic(t, overrides = {}) {
  return withTemp(t, async (stateRoot) => {
    const git = overrides.git ?? fakeGit();
    const calls = [];
    let active = 0;
    let peak = 0;
    const executeScope = overrides.executeScope ?? (async (command) => {
      active += 1;
      peak = Math.max(peak, active);
      calls.push({
        id: command.scope.id,
        argv: [...command.argv],
        jobs: command.env.CARGO_BUILD_JOBS,
      });
      await new Promise((resolve) => setTimeout(resolve, 3));
      const raw = llvmScopeReport(command.scope.id, command.policy, command.packageInventory);
      await writeFile(command.rawReportPath, `${JSON.stringify(raw)}\n`, { mode: 0o600 });
      active -= 1;
    });
    const result = await runCoverage({
      repoRoot,
      stateRoot,
      base: "origin/develop",
      policy: await loadPolicy(),
      git,
      executeScope,
      env: overrides.env ?? {},
      now: () => new Date("2026-09-01T00:00:00.000Z"),
    });
    return { ...result, calls, peak, stateRoot, git };
  });
}

test("the policy is closed and classifies every workspace package exactly once", async () => {
  const policy = await loadPolicy();
  const packages = await discoverWorkspacePackages(repoRoot);
  assert.equal(packages.length, 45);
  assert.doesNotThrow(() => validatePolicy(policy, packages));
  assert.deepEqual(policy.classifications.plainTestExclusions.map(({ package: name }) => name), ["oxid-app"]);
  assert.deepEqual(policy.classifications.plainTestExclusions[0].command, ["cargo", "test", "-p", "oxid-app"]);

  const missing = structuredClone(policy);
  missing.classifications.workspaceOnly.pop();
  assert.throws(() => validatePolicy(missing, packages), /missing classification/u);

  const duplicate = structuredClone(policy);
  duplicate.classifications.workspaceOnly.push(duplicate.classifications.core[0]);
  assert.throws(() => validatePolicy(duplicate, packages), /duplicate classification/u);

  const unknown = structuredClone(policy);
  unknown.classifications.workspaceOnly.push("oxid-not-a-package");
  assert.throws(() => validatePolicy(unknown, packages), /unknown package/u);

  const malformed = structuredClone(policy);
  malformed.unreviewedEscapeHatch = true;
  assert.throws(() => validatePolicy(malformed, packages), /unknown policy key/u);
});

test("an atomic lock refuses both active and stale ownership", async (t) => withTemp(t, async (directory) => {
  const lockPath = path.join(directory, "coverage.lock");
  const release = await acquireCoverageLock(lockPath, { pid: process.pid, sourceHead: HEAD });
  assert.equal((await stat(lockPath)).mode & 0o777, 0o600);
  await assert.rejects(
    acquireCoverageLock(lockPath, { pid: process.pid + 1, sourceHead: HEAD }),
    /coverage lock already exists/u,
  );
  await release();

  await writeFile(lockPath, '{"pid":999999,"sourceHead":"stale"}\n', { mode: 0o600 });
  await assert.rejects(
    acquireCoverageLock(lockPath, { pid: process.pid, sourceHead: HEAD }),
    /coverage lock already exists/u,
  );
}));

test("dirty source and unavailable or non-ancestor bases fail closed", async (t) => {
  await withTemp(t, async (stateRoot) => {
    const dirty = fakeGit();
    dirty.state.status = "?? untracked.rs";
    await assert.rejects(runCoverage({
      repoRoot,
      stateRoot,
      base: "origin/develop",
      policy: await loadPolicy(),
      git: dirty,
      executeScope: async () => assert.fail("dirty source launched a scope"),
    }), /source tree is dirty/u);
  });

  await withTemp(t, async (stateRoot) => {
    const unavailable = fakeGit({
      resolve(ref) {
        if (ref === "HEAD") return HEAD;
        throw new Error("unknown revision");
      },
    });
    await assert.rejects(runCoverage({
      repoRoot,
      stateRoot,
      base: "missing",
      policy: await loadPolicy(),
      git: unavailable,
      executeScope: async () => assert.fail("missing base launched a scope"),
    }), /could not resolve comparison base/u);
  });

  await withTemp(t, async (stateRoot) => {
    const unrelated = fakeGit({ isAncestor: () => false });
    await assert.rejects(runCoverage({
      repoRoot,
      stateRoot,
      base: "other",
      policy: await loadPolicy(),
      git: unrelated,
      executeScope: async () => assert.fail("unrelated base launched a scope"),
    }), /not an ancestor/u);
  });
});

test("HEAD, base, and cleanliness drift are detected between serial scopes", async (t) => {
  for (const drift of ["head", "base", "status"]) {
    await withTemp(t, async (stateRoot) => {
      const git = fakeGit();
      let calls = 0;
      await assert.rejects(runCoverage({
        repoRoot,
        stateRoot,
        base: "origin/develop",
        policy: await loadPolicy(),
        git,
        executeScope: async ({ rawReportPath }) => {
          calls += 1;
          await writeFile(rawReportPath, `${JSON.stringify(llvmReport())}\n`, { mode: 0o600 });
          if (drift === "head") git.state.head = "f".repeat(40);
          if (drift === "base") git.state.base = "e".repeat(40);
          if (drift === "status") git.state.status = " M run.sh";
        },
      }), new RegExp(`${drift === "status" ? "dirty" : drift}.*drift`, "iu"));
      assert.equal(calls, 1);
      await assert.rejects(stat(path.join(stateRoot, "coverage", HEAD)), { code: "ENOENT" });
      await assert.rejects(stat(path.join(stateRoot, ".oxid-coverage.lock")), { code: "ENOENT" });
    });
  }
});

test("workspace, headless, and desktop commands run in strict order with bounded jobs", async (t) => {
  const result = await runSynthetic(t);
  assert.equal(result.peak, 1);
  assert.deepEqual(result.calls.map(({ id }) => id), ["workspace-aggregate", "headless-host", "desktop-host"]);
  assert.deepEqual(result.calls.map(({ jobs }) => jobs), ["2", "2", "2"]);
  assert.match(result.calls[0].argv.join(" "), /--fail-under-lines 80/u);
  assert.match(
    result.calls[0].argv.join(" "),
    /--features oxid-adapter-deployment-profile\/readiness,oxid-adapter-did-midnight\/tailnet-test-did-publication,oxid-adapter-storage-dev\/development-fixture,oxid-composition\/preprod-observation,oxid-composition\/standalone-development/u,
  );
  assert.match(result.calls[1].argv.join(" "), /-p oxid-headless --all-targets/u);
  assert.match(
    result.calls[2].argv.join(" "),
    /-p oxid-ui-dioxus --all-targets --features ui-profile-demo,app-profile-authority,standalone-deployment-profile,preprod-observation/u,
  );

  const explicit = await runSynthetic(t, { env: { OXID_COVERAGE_JOBS: "4" } });
  assert.deepEqual(explicit.calls.map(({ jobs }) => jobs), ["4", "4", "4"]);
  for (const value of ["0", "9", "2.5", "many"]) {
    await assert.rejects(runSynthetic(t, { env: { OXID_COVERAGE_JOBS: value } }), /OXID_COVERAGE_JOBS/u);
  }
});

test("published evidence is private, checksummed, normalized, and path-redacted", async (t) => {
  const result = await runSynthetic(t);
  const files = await listFiles(result.reportRoot);
  const relative = files.map((file) => path.relative(result.reportRoot, file).replaceAll(path.sep, "/")).sort();
  assert.deepEqual(relative, [
    "changed-lines.json",
    "checksums.json",
    "desktop-host-summary.json",
    "evaluation.json",
    "headless-host-summary.json",
    "manifest.json",
    "workspace-aggregate-summary.json",
  ]);
  for (const file of files) assert.equal((await stat(file)).mode & 0o777, 0o600, file);
  assert.equal((await stat(result.reportRoot)).mode & 0o777, 0o700);

  const published = (await Promise.all(files.map((file) => readFile(file, "utf8")))).join("\n");
  assert.doesNotMatch(published, new RegExp(repoRoot.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  assert.doesNotMatch(published, /CARGO_|LLVM_PROFILE_FILE|\.profraw|"environment"/u);
  assert.match(published, /crates\/foundation\/src\/lib\.rs/u);
  assert.match(published, /target\/coverage-contract-[^/]+\/coverage\/[0-9a-f]{40}\/reports/u);

  const checksums = JSON.parse(await readFile(path.join(result.reportRoot, "checksums.json"), "utf8"));
  for (const [name, expected] of Object.entries(checksums.files)) {
    const bytes = await readFile(path.join(result.reportRoot, name));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected);
  }
  await assert.rejects(stat(path.join(result.headRoot, "tmp")), { code: "ENOENT" });
  await assert.rejects(stat(path.join(result.headRoot, "build")), { code: "ENOENT" });
});

test("dependency sources are omitted while untrusted source paths fail closed", () => {
  const filesystemRoot = path.parse(repoRoot).root;
  const dependencySources = [
    path.join(filesystemRoot, "home/runner/.cargo/registry/src/index.crates.io/hash/dependency.rs"),
    path.join(filesystemRoot, "home/runner/.cargo/git/checkouts/dependency/hash/src/lib.rs"),
    path.join(filesystemRoot, "home/runner/.rustup/toolchains/stable/lib/rustlib/src/rust/library/core/src/lib.rs"),
  ];
  const raw = llvmReport();
  raw.data[0].files.push(...dependencySources.map((filename) => ({
    filename,
    summary: { lines: { count: 1, covered: 1, percent: 100 } },
  })));
  raw.data[0].functions = [{
    filenames: dependencySources,
    regions: dependencySources.map((_, index) => [1, 1, 1, 2, 1, index]),
  }];

  const normalized = normalizeLlvmReport(raw, { repoRoot, scopeId: "workspace-aggregate" });
  assert.deepEqual(normalized.files.map(({ path: sourcePath }) => sourcePath), ["crates/foundation/src/lib.rs"]);
  for (const sourcePath of dependencySources) assert.doesNotMatch(JSON.stringify(normalized), new RegExp(sourcePath, "u"));

  assert.throws(
    () => normalizeLlvmReport({}, { repoRoot, scopeId: "workspace-aggregate" }),
    /malformed LLVM coverage output/u,
  );
  for (const sourcePath of ["../crates/foundation/src/lib.rs", path.join(filesystemRoot, "project/crates/foundation/src/lib.rs")]) {
    assert.throws(
      () => normalizeLlvmReport(llvmReport(sourcePath), { repoRoot, scopeId: "workspace-aggregate" }),
      /outside repository/u,
    );
  }
});

test("scope failure removes only the run-owned head output and lock", async (t) => withTemp(t, async (stateRoot) => {
  let calls = 0;
  await assert.rejects(runCoverage({
    repoRoot,
    stateRoot,
    base: "origin/develop",
    policy: await loadPolicy(),
    git: fakeGit(),
    executeScope: async ({ rawReportPath }) => {
      calls += 1;
      await writeFile(rawReportPath, "partial private output", { mode: 0o600 });
      throw new Error("synthetic scope failure");
    },
  }), /synthetic scope failure/u);
  assert.equal(calls, 1);
  await assert.rejects(stat(path.join(stateRoot, "coverage", HEAD)), { code: "ENOENT" });
  await assert.rejects(stat(path.join(stateRoot, ".oxid-coverage.lock")), { code: "ENOENT" });
}));

test("the explicit dry-run seam emits non-coverage evidence without an executor", async (t) => withTemp(
  t,
  async (stateRoot) => {
    const result = await runCoverage({
      repoRoot,
      stateRoot,
      base: "origin/develop",
      policy: await loadPolicy(),
      git: fakeGit(),
      dryRun: true,
      env: { OXID_COVERAGE_JOBS: "2", PATH: "" },
      now: () => new Date("2026-09-01T00:00:00.000Z"),
    });
    assert.equal(result.manifest.mode, "dry-run");
    assert.deepEqual(result.manifest.commands.map(({ id }) => id), [
      "workspace-aggregate",
      "headless-host",
      "desktop-host",
    ]);
  },
));

test("the CLI rejects missing, duplicate, malformed, and unknown arguments", () => {
  assert.throws(() => parseArguments([]), /--base is required/u);
  assert.throws(() => parseArguments(["--base"]), /requires a value/u);
  assert.throws(() => parseArguments(["--base", "one", "--base", "two"]), /specified more than once/u);
  assert.throws(() => parseArguments(["--base", "HEAD", "--wat"]), /unknown argument/u);
  assert.deepEqual(parseArguments(["--base", "origin/develop", "--dry-run"]), {
    base: "origin/develop",
    dryRun: true,
    enforce: false,
    policyPath: undefined,
  });
});

test("run.sh wires the repository contract once and delegates coverage to the harness", async () => {
  const runScript = await readFile(path.join(repoRoot, "run.sh"), "utf8");
  assert.equal(runScript.match(/node --test tests\/repository\/coverage-contract\.test\.mjs/gu)?.length, 1);
  const coverageBlock = runScript.slice(runScript.indexOf("run_coverage()"), runScript.indexOf("require_command()"));
  assert.match(coverageBlock, /node scripts\/coverage\/run\.mjs/u);
  assert.doesNotMatch(coverageBlock, /cargo llvm-cov/u);
});

test("hosted coverage supplies a fetched, non-empty source comparison base", async () => {
  const workflow = await readFile(path.join(repoRoot, ".github/workflows/ci.yml"), "utf8");
  const coverageJob = workflow.slice(workflow.indexOf("\n  coverage_linux:\n"), workflow.indexOf("\n  repository_gate:"));
  assert.match(coverageJob, /fetch-depth: 0/u);
  assert.match(
    coverageJob,
    /OXID_COVERAGE_BASE: \$\{\{ github\.event\.pull_request\.base\.sha \|\| github\.event\.before \|\| 'origin\/develop' \}\}/u,
  );
  assert.match(coverageJob, /\.\/run\.sh coverage --strict/u);
});
